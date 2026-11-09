//! `cisco`：IOS `15.2(7)E8`、ASA `9.18(4)22`、NX-OS `10.2(3t)`。
//!
//! 正規化為 (主, 次, rebuild, rebuild 字母, train, train 數字)：`15.2(7)E8` → (15, 2, 7, "", "e", 8)。
//! ASA 的 CNA 記錄改用點分寫法 `9.18.4.22`，視同 `9.18(4)22`（train 為空、第四段為 interim 編號）。
//! 只有主次版本與 train 都相同才可排序（依 rebuild、rebuild 字母、train 數字），否則比較結果為未知；
//! 等值判斷則可確定不同版本線的版本不相等（Cisco 的 CNA 記錄多半逐版列舉跨 train 的版本）。

use std::cmp::Ordering;

use super::{canonical, Version, VersionScheme};
use crate::version_text::vendor::{CISCO, DOTTED_FOUR};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cisco {
    pub major: u64,
    pub minor: u64,
    pub rebuild: u64,
    /// rebuild 後的字母（NX-OS `3t`）；沒有時為空字串
    pub rebuild_letter: String,
    /// train（IOS `E`、`M`、`SE`）；ASA 與沒寫 train 的版本為空字串
    pub train: String,
    /// train 後的數字（ASA 的 interim 編號）；沒寫時為 0
    pub train_number: u64,
}

impl Cisco {
    pub fn parse(raw: &str) -> Option<Self> {
        let text = canonical(raw);
        if let Some(caps) = CISCO.captures(&text) {
            let num = |i: usize| number(caps.get(i).map(|m| m.as_str()));
            return Some(Cisco {
                major: num(1)?,
                minor: num(2)?,
                rebuild: num(3)?,
                rebuild_letter: caps[4].to_string(),
                train: caps[5].to_string(),
                train_number: num(6)?,
            });
        }
        let caps = DOTTED_FOUR.captures(&text)?;
        let num = |i: usize| number(caps.get(i).map(|m| m.as_str()));
        Some(Cisco {
            major: num(1)?,
            minor: num(2)?,
            rebuild: num(3)?,
            rebuild_letter: String::new(),
            train: String::new(),
            train_number: num(4)?,
        })
    }

    fn same_train(&self, other: &Cisco) -> bool {
        (self.major, self.minor, self.train.as_str())
            == (other.major, other.minor, other.train.as_str())
    }

    fn key(&self) -> (u64, &str, u64) {
        (
            self.rebuild,
            self.rebuild_letter.as_str(),
            self.train_number,
        )
    }
}

/// 沒寫或空白的數字段視為 0。
fn number(text: Option<&str>) -> Option<u64> {
    text.filter(|s| !s.is_empty())
        .map_or(Some(0), |s| s.parse().ok())
}

pub struct CiscoScheme;

impl VersionScheme for CiscoScheme {
    fn name(&self) -> &'static str {
        "cisco"
    }

    fn parse(&self, raw: &str) -> Option<Version> {
        Cisco::parse(raw).map(Version::Cisco)
    }

    fn cmp(&self, a: &Version, b: &Version) -> Option<Ordering> {
        match (a, b) {
            (Version::Cisco(x), Version::Cisco(y)) if x.same_train(y) => {
                Some(x.key().cmp(&y.key()))
            }
            _ => None,
        }
    }

    fn covers(&self, bound: &Version, v: &Version) -> Option<bool> {
        match (bound, v) {
            (Version::Cisco(b), Version::Cisco(x)) => Some(b == x),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(a: &str, b: &str) -> Option<Ordering> {
        super::super::compare(&CiscoScheme, a, b)
    }

    #[test]
    fn parses_ios_asa_and_nxos_spellings() {
        let ios = Cisco::parse("15.2(7)E8").unwrap();
        assert_eq!((ios.major, ios.minor, ios.rebuild), (15, 2, 7));
        assert_eq!((ios.train.as_str(), ios.train_number), ("e", 8));
        let asa = Cisco::parse("9.18(4)22").unwrap();
        assert_eq!((asa.train.as_str(), asa.train_number), ("", 22));
        let nxos = Cisco::parse("Version 10.2(3t)").unwrap();
        assert_eq!((nxos.rebuild, nxos.rebuild_letter.as_str()), (3, "t"));
        assert_eq!(Cisco::parse("9.18.4.22"), Some(asa));
        assert!(Cisco::parse("17.9.4a").is_none());
        assert!(Cisco::parse("7.0(3)I7(9)").is_none());
    }

    #[test]
    fn cisco_ordering_within_a_train() {
        assert_eq!(cmp("15.2(6)E2", "15.2(7)E8"), Some(Ordering::Less));
        assert_eq!(cmp("15.2(7)E8", "15.2(7)E10"), Some(Ordering::Less));
        assert_eq!(cmp("15.2(7)e8", "15.2(7)E8"), Some(Ordering::Equal));
    }

    #[test]
    fn cisco_versions_from_different_trains_are_not_comparable() {
        assert_eq!(cmp("15.2(4)M3", "15.2(7)E8"), None);
        assert_eq!(cmp("15.0(2)SE11", "15.2(7)E8"), None);
        assert_eq!(cmp("12.4(25)", "12.4(25)T"), None);
    }

    #[test]
    fn cisco_asa_interim_releases() {
        assert_eq!(cmp("9.18(4)8", "9.18(4)22"), Some(Ordering::Less));
        assert_eq!(cmp("9.18.4.8", "9.18(4)22"), Some(Ordering::Less));
        assert_eq!(cmp("9.18.4", "9.18(4)"), Some(Ordering::Equal));
        assert_eq!(cmp("9.18(4)", "9.18(4)8"), Some(Ordering::Less));
    }

    #[test]
    fn equality_across_trains_is_known_to_be_false() {
        let a = CiscoScheme.parse("15.2(7)E8").unwrap();
        let b = CiscoScheme.parse("15.2(4)M3").unwrap();
        assert_eq!(CiscoScheme.covers(&a, &b), Some(false));
        assert_eq!(CiscoScheme.covers(&a, &a), Some(true));
    }
}
