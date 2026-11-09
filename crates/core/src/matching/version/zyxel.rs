//! `zyxel`：Zyxel 韌體 `V主.次(型號代碼.修補)C數字`，`V` 可省略。
//!
//! `V2.80(ABCD.1)C0` → (2, 80, "abcd", 1, 0)。型號代碼不同的韌體不可排序（比較結果為未知），
//! 等值判斷則確定不相等。型號代碼由使用者照設備畫面填寫；目錄不維護「型號 → 代碼」對照。

use std::cmp::Ordering;

use super::{canonical, Version, VersionScheme};
use crate::version_text::vendor::ZYXEL;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zyxel {
    pub major: u64,
    pub minor: u64,
    /// 型號代碼（小寫）
    pub model: String,
    pub patch: u64,
    pub c: u64,
}

impl Zyxel {
    pub fn parse(raw: &str) -> Option<Self> {
        let text = canonical(raw);
        let caps = ZYXEL.captures(&text)?;
        let num = |i: usize| -> Option<u64> { caps[i].parse().ok() };
        Some(Zyxel {
            major: num(1)?,
            minor: num(2)?,
            model: caps[3].to_string(),
            patch: num(4)?,
            c: num(5)?,
        })
    }

    fn key(&self) -> (u64, u64, u64, u64) {
        (self.major, self.minor, self.patch, self.c)
    }
}

pub struct ZyxelScheme;

impl VersionScheme for ZyxelScheme {
    fn name(&self) -> &'static str {
        "zyxel"
    }

    fn parse(&self, raw: &str) -> Option<Version> {
        Zyxel::parse(raw).map(Version::Zyxel)
    }

    fn cmp(&self, a: &Version, b: &Version) -> Option<Ordering> {
        match (a, b) {
            (Version::Zyxel(x), Version::Zyxel(y)) if x.model == y.model => {
                Some(x.key().cmp(&y.key()))
            }
            _ => None,
        }
    }

    fn covers(&self, bound: &Version, v: &Version) -> Option<bool> {
        match (bound, v) {
            (Version::Zyxel(b), Version::Zyxel(x)) => Some(b == x),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(a: &str, b: &str) -> Option<Ordering> {
        super::super::compare(&ZyxelScheme, a, b)
    }

    #[test]
    fn zyxel_firmware_ordering() {
        assert_eq!(
            cmp("V2.70(ABCD.3)C0", "V2.80(ABCD.0)C0"),
            Some(Ordering::Less)
        );
        assert_eq!(
            cmp("V2.80(ABCD.0)C0", "V2.80(ABCD.1)C0"),
            Some(Ordering::Less)
        );
        assert_eq!(
            cmp("2.80(abcd.1)c0", "V2.80(ABCD.1)C0"),
            Some(Ordering::Equal)
        );
        assert_eq!(
            cmp("V2.80(ABCD.1)C0", "V2.80(ABCD.1)C1"),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn zyxel_firmware_for_another_model() {
        assert_eq!(cmp("V2.80(ABCD.1)C0", "V2.80(WXYZ.1)C0"), None);
        let a = ZyxelScheme.parse("V2.80(ABCD.1)C0").unwrap();
        let b = ZyxelScheme.parse("V2.80(WXYZ.1)C0").unwrap();
        assert_eq!(ZyxelScheme.covers(&b, &a), Some(false));
    }

    #[test]
    fn rejects_other_spellings() {
        assert!(Zyxel::parse("V2.80(ABCD.1)").is_none());
        assert!(Zyxel::parse("2.80").is_none());
    }
}
