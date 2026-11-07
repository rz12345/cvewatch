//! `panos`：`major.minor[.maint][-hN]`，正規化為 (major, minor, maint, hotfix)；無 `-h` 視為 hotfix 0。
//!
//! `11.1.4-h7 < 11.1.4-h12 < 11.1.5`。

use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

use super::{canonical, Version, VersionScheme};

static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d+)\.(\d+)(?:\.(\d+))?(?:-h(\d+))?$").expect("固定的正規表示式必定可編譯")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Panos {
    pub major: u64,
    pub minor: u64,
    pub maint: u64,
    pub hotfix: u64,
}

impl Panos {
    pub fn parse(raw: &str) -> Option<Self> {
        let text = canonical(raw);
        let caps = PATTERN.captures(&text)?;
        let num =
            |i: usize| -> Option<u64> { caps.get(i).map_or(Some(0), |m| m.as_str().parse().ok()) };
        Some(Panos {
            major: num(1)?,
            minor: num(2)?,
            maint: num(3)?,
            hotfix: num(4)?,
        })
    }
}

pub struct PanosScheme;

impl VersionScheme for PanosScheme {
    fn name(&self) -> &'static str {
        "panos"
    }

    fn parse(&self, raw: &str) -> Option<Version> {
        Panos::parse(raw).map(Version::Panos)
    }

    fn cmp(&self, a: &Version, b: &Version) -> Option<Ordering> {
        match (a, b) {
            (Version::Panos(x), Version::Panos(y)) => Some(x.cmp(y)),
            _ => None,
        }
    }

    fn change_applies(&self, at: &Version, v: &Version) -> Option<bool> {
        match (at, v) {
            (Version::Panos(a), Version::Panos(x)) => {
                // Palo Alto 逐條版本線列出（`>=11.1.0` 不代表 11.2 也受影響）：維護版本的變更點只適用
                // 同一功能版本線，hotfix 變更點只適用同一維護版本線
                let same_feature = (a.major, a.minor) == (x.major, x.minor);
                let same_maint = same_feature && a.maint == x.maint;
                Some(a <= x && same_feature && (a.hotfix == 0 || same_maint))
            }
            _ => None,
        }
    }

    fn same_line(&self, base: &Version, v: &Version) -> Option<bool> {
        match (base, v) {
            (Version::Panos(b), Version::Panos(x)) => {
                Some((b.major, b.minor, b.maint) == (x.major, x.minor, x.maint))
            }
            _ => None,
        }
    }
}
