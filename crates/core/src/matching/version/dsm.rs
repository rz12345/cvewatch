//! `dsm`：Synology DSM `x.y[.z][-build][ Update n]`，正規化為 (x, y, z, build, update)。
//!
//! `7.2.2-72806 Update 3` → (7, 2, 2, 72806, 3)。CVE 紀錄常見的 `7.2.1-69057-2` 視為 Update 2；
//! 排序鍵為 (x, y, z, Update, build)：同一個 x.y.z 只有一個正式 build，Update 先於 build 比較，
//! 讓沒寫 build 的 `7.2.2 Update 3`（NVD CPE 常見）與 `7.2.2-72806 Update 1` 能正確排序；
//! 沒寫的 build / Update 視為 0（`lessThan: 7.2.2` 涵蓋所有 7.2.1 build、不含 7.2.2 的 build）；
//! 等值與 `≤` 上界時，沒寫 build（或沒寫 Update）的版本涵蓋整條版本線：
//! `7.2.1` 涵蓋 `7.2.1-69057 Update 3`，`7.2.1-69057` 涵蓋其所有 Update。

use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

use super::{canonical, Version, VersionScheme};

static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d+)\.(\d+)(?:\.(\d+))?(?:-(\d+))?(?:(?:-| ?update ?)(\d+))?$")
        .expect("固定的正規表示式必定可編譯")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dsm {
    pub major: u64,
    pub minor: u64,
    pub micro: u64,
    /// 沒寫 build 時為 None（排序時視為 0）
    pub build: Option<u64>,
    /// 沒寫 Update 時為 None（排序時視為 0）
    pub update: Option<u64>,
}

impl Dsm {
    pub fn parse(raw: &str) -> Option<Self> {
        let text = canonical(raw);
        let caps = PATTERN.captures(&text)?;
        let num =
            |i: usize| -> Option<u64> { caps.get(i).map_or(Some(0), |m| m.as_str().parse().ok()) };
        let opt = |i: usize| -> Option<Option<u64>> {
            caps.get(i)
                .map(|m| m.as_str().parse().ok())
                .map_or(Some(None), |v| v.map(Some))
        };
        Some(Dsm {
            major: num(1)?,
            minor: num(2)?,
            micro: num(3)?,
            build: opt(4)?,
            update: opt(5)?,
        })
    }

    fn key(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.major,
            self.minor,
            self.micro,
            self.update.unwrap_or(0),
            self.build.unwrap_or(0),
        )
    }

    /// 有寫的欄位都相同（沒寫的 build / Update 不限）。
    fn covers(&self, v: &Dsm) -> bool {
        let same = |bound: Option<u64>, actual: Option<u64>| {
            bound.is_none_or(|b| b == actual.unwrap_or(0))
        };
        (self.major, self.minor, self.micro) == (v.major, v.minor, v.micro)
            && same(self.build, v.build)
            && same(self.update, v.update)
    }
}

pub struct DsmScheme;

impl VersionScheme for DsmScheme {
    fn name(&self) -> &'static str {
        "dsm"
    }

    fn parse(&self, raw: &str) -> Option<Version> {
        Dsm::parse(raw).map(Version::Dsm)
    }

    fn cmp(&self, a: &Version, b: &Version) -> Option<Ordering> {
        match (a, b) {
            (Version::Dsm(x), Version::Dsm(y)) => Some(x.key().cmp(&y.key())),
            _ => None,
        }
    }

    fn covers(&self, bound: &Version, v: &Version) -> Option<bool> {
        match (bound, v) {
            (Version::Dsm(b), Version::Dsm(x)) => Some(b.covers(x)),
            _ => None,
        }
    }
}
