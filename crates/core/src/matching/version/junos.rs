//! `junos`：Juniper Junos `21.4R3-S5`、`15.1X49-D200`，Ivanti `22.7R2.4`。
//!
//! 正規化為 (主, 次, 類型, release, 小數, 服務字母, 服務數字, 服務小數)：
//! `21.4R3-S5` → (21, 4, "r", 3, 0, "s", 5, 0)。只寫 `21.4` 為該版本線的起點（早於同版本線的任何 release）。
//! 依主、次、release、小數、服務數字依序比較，跨主次版本也可比較（Juniper CNA 區間以 `22.2` 起、
//! `22.2R3-S3` 止）。兩者都有類型且類型不同時（`R` 與 `X`／`F`／`D` 等特殊版本）比較結果為未知。
//! Ivanti 25.x 起改用點分寫法 `25.1.1.3`，視同 `25.1R1.3`（一般 release）。
//! NVD CPE 的 update 為 `*` 時（`junos:21.4:*`），基準只比對有寫的部分：`21.4` 涵蓋 21.4 的所有 release，
//! `21.4R3` 涵蓋 `21.4R3-S5`。

use std::cmp::Ordering;

use super::{canonical, Version, VersionScheme};
use crate::version_text::vendor::{DOTTED_FOUR, JUNOS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Junos {
    pub major: u64,
    pub minor: u64,
    /// 版本類型（`r`、`x`、`f`、`d`）；只寫主次版本時為 None
    pub kind: Option<String>,
    pub release: u64,
    /// Ivanti 的小數 release（`22.7R2.4` 的 4）
    pub sub: u64,
    pub service: u64,
    pub service_sub: u64,
}

impl Junos {
    pub fn parse(raw: &str) -> Option<Self> {
        let text = canonical(raw);
        if let Some(caps) = DOTTED_FOUR.captures(&text) {
            let num = |i: usize| -> Option<u64> {
                caps.get(i).map_or(Some(0), |m| m.as_str().parse().ok())
            };
            return Some(Junos {
                major: num(1)?,
                minor: num(2)?,
                kind: Some("r".to_string()),
                release: num(3)?,
                sub: num(4)?,
                service: 0,
                service_sub: 0,
            });
        }
        let caps = JUNOS.captures(&text)?;
        let num =
            |i: usize| -> Option<u64> { caps.get(i).map_or(Some(0), |m| m.as_str().parse().ok()) };
        Some(Junos {
            major: num(1)?,
            minor: num(2)?,
            kind: caps.get(3).map(|m| m.as_str().to_string()),
            release: num(4)?,
            sub: num(5)?,
            service: num(7)?,
            service_sub: num(8)?,
        })
    }

    fn comparable(&self, other: &Junos) -> bool {
        match (&self.kind, &other.kind) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
    }

    /// `v` 屬於本版本寫出的版本線（沒寫的 release、小數、服務版本不限）。
    fn line_contains(&self, v: &Junos) -> bool {
        let same_line = (self.major, self.minor) == (v.major, v.minor);
        let Some(kind) = &self.kind else {
            return same_line && self.comparable(v);
        };
        let unset_or = |base: u64, actual: u64| base == 0 || base == actual;
        same_line
            && v.kind.as_ref() == Some(kind)
            && self.release == v.release
            && unset_or(self.sub, v.sub)
            && unset_or(self.service, v.service)
            && unset_or(self.service_sub, v.service_sub)
    }

    fn key(&self) -> (u64, u64, bool, u64, u64, u64, u64) {
        (
            self.major,
            self.minor,
            self.kind.is_some(),
            self.release,
            self.sub,
            self.service,
            self.service_sub,
        )
    }
}

pub struct JunosScheme;

impl VersionScheme for JunosScheme {
    fn name(&self) -> &'static str {
        "junos"
    }

    fn parse(&self, raw: &str) -> Option<Version> {
        Junos::parse(raw).map(Version::Junos)
    }

    fn cmp(&self, a: &Version, b: &Version) -> Option<Ordering> {
        match (a, b) {
            (Version::Junos(x), Version::Junos(y)) if x.comparable(y) => {
                Some(x.key().cmp(&y.key()))
            }
            _ => None,
        }
    }

    fn same_line(&self, base: &Version, v: &Version) -> Option<bool> {
        match (base, v) {
            (Version::Junos(b), Version::Junos(x)) if b.comparable(x) => Some(b.line_contains(x)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(a: &str, b: &str) -> Option<Ordering> {
        super::super::compare(&JunosScheme, a, b)
    }

    #[test]
    fn juniper_service_releases() {
        assert_eq!(cmp("21.4R3", "21.4R3-S5"), Some(Ordering::Less));
        assert_eq!(cmp("21.4R3-S5", "21.4R3-S10"), Some(Ordering::Less));
        assert_eq!(cmp("21.4R3-S5", "21.4r3-s5"), Some(Ordering::Equal));
    }

    #[test]
    fn juniper_versions_from_different_lines() {
        assert_eq!(cmp("21.4R3-S5", "22.2"), Some(Ordering::Less));
        assert_eq!(cmp("22.2", "22.2R1"), Some(Ordering::Less));
        assert_eq!(cmp("21.4R3-S5", "21.4R3-S6"), Some(Ordering::Less));
    }

    #[test]
    fn ivanti_decimal_releases() {
        assert_eq!(cmp("22.7R2.3", "22.7R2.4"), Some(Ordering::Less));
        assert_eq!(cmp("22.7R2.4", "22.7R2"), Some(Ordering::Greater));
        assert_eq!(cmp("9.1R18.9", "22.7R1"), Some(Ordering::Less));
        assert_eq!(cmp("22.8R2.6", "25.1.1.3"), Some(Ordering::Less));
        assert_eq!(cmp("25.1.1.3", "25.1R1.3"), Some(Ordering::Equal));
    }

    #[test]
    fn special_release_types_only_compare_with_the_same_type() {
        assert_eq!(cmp("15.1X49-D200", "15.1X49-D100"), Some(Ordering::Greater));
        assert_eq!(cmp("15.1X49-D200", "21.4R3"), None);
        assert_eq!(cmp("15.1", "15.1X49-D200"), Some(Ordering::Less));
    }

    #[test]
    fn nvd_any_update_covers_the_written_line() {
        let v = JunosScheme.parse("21.4R3-S5").unwrap();
        let line = |s: &str| JunosScheme.same_line(&JunosScheme.parse(s).unwrap(), &v);
        assert_eq!(line("21.4"), Some(true));
        assert_eq!(line("21.4R3"), Some(true));
        assert_eq!(line("21.4R2"), Some(false));
        assert_eq!(line("22.2"), Some(false));
        assert_eq!(line("21.4R3-S6"), Some(false));
        assert_eq!(line("21.4X49-D10"), None);
    }

    #[test]
    fn rejects_other_spellings() {
        assert!(Junos::parse("21.4R3-S5-EVO").is_none());
        assert!(Junos::parse("21").is_none());
    }
}
