//! `esxi`：VMware ESXi / vCenter 版本。
//!
//! - 名稱：`8.0 U3c`、`8.0 Update 3c`、`8.0u3 P01`、`7.0 U3 EP 1`、`8.0c` → (major, minor, update, letter, patch, ep)
//! - 數字段：`8.0.3` 的第三段即 Update 編號
//! - build：`24414501`、`build-24414501`；透過目錄的 build 對照表轉成名稱，
//!   對照不到時只能與其他 build 比較（與名稱比較時回傳 None → 判定為未知）
//! - 名稱 + build：`8.0.3 build-24414501`，名稱相同時再比較 build

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use super::{canonical, Version, VersionScheme};
pub use crate::version_text::esxi::{parse_named, parse_release, Release};

static BUILD_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:build[- ]?)?(\d{6,9})$").expect("固定的正規表示式必定可編譯")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Esxi {
    pub release: Option<Release>,
    pub build: Option<u64>,
    /// 實際 Update 字母未知：數字寫法（`8.0.3`）或名稱 + 對照表沒有的 build，且沒寫字母
    pub letter_unknown: bool,
}

pub struct EsxiScheme {
    builds: BTreeMap<u64, Release>,
}

impl EsxiScheme {
    pub fn new(builds: BTreeMap<u64, Release>) -> Self {
        EsxiScheme { builds }
    }

    pub fn parse_esxi(&self, raw: &str) -> Option<Esxi> {
        let text = canonical(raw);
        if let Some(caps) = BUILD_ONLY.captures(&text) {
            let build: u64 = caps[1].parse().ok()?;
            return Some(Esxi {
                release: self.builds.get(&build).copied(),
                build: Some(build),
                letter_unknown: false,
            });
        }
        let (named, build, unspecified) = parse_named(&text)?;
        // 名稱 + build（`8.0.3 build-24414501`、`ESXi80U3d-24585383`）：對照表較精確（含 Update 字母），以對照表為準
        let mapped = build.and_then(|b| self.builds.get(&b).copied());
        Some(Esxi {
            release: Some(mapped.unwrap_or(named)),
            build,
            letter_unknown: mapped.is_none()
                && named.letter == 0
                && (build.is_some() || unspecified),
        })
    }
}

/// 兩邊都有名稱 → 只比名稱（build 已透過對照表轉成名稱）；兩邊都只有 build → 比 build；
/// 混合時無法比較。如此在可比較的版本之間維持全序（可遞移），排序區間時不會形成循環。
///
/// 字母未知的版本（`8.0.3 build-<對照表沒有的 build>`）與已知字母的版本只在 Update 編號不同時可比較；
/// 同一 Update 時結果取決於字母，回傳 None（判定為未知，不猜）。兩者同為字母未知時比較 build
/// （同一版本線的 build 隨發布遞增）；都沒有 build（`8.0.3` 對 `8.0.3`）時名稱相同即相等。
fn compare(a: &Esxi, b: &Esxi) -> Option<Ordering> {
    match (a.release, b.release) {
        (Some(x), Some(y)) => {
            let line = |r: Release| (r.major, r.minor, r.update);
            let by_line = line(x).cmp(&line(y));
            if by_line != Ordering::Equal || !(a.letter_unknown || b.letter_unknown) {
                return Some(by_line.then(x.cmp(&y)));
            }
            if !(a.letter_unknown && b.letter_unknown) {
                return None;
            }
            match (a.build, b.build) {
                (Some(p), Some(q)) => Some(p.cmp(&q)),
                (None, None) => (x == y).then_some(Ordering::Equal),
                _ => None,
            }
        }
        (None, None) => Some(a.build?.cmp(&b.build?)),
        _ => None,
    }
}

impl VersionScheme for EsxiScheme {
    fn name(&self) -> &'static str {
        "esxi"
    }

    fn parse(&self, raw: &str) -> Option<Version> {
        self.parse_esxi(raw).map(Version::Esxi)
    }

    fn cmp(&self, a: &Version, b: &Version) -> Option<Ordering> {
        match (a, b) {
            (Version::Esxi(x), Version::Esxi(y)) => compare(x, y),
            _ => None,
        }
    }

    /// `8.0` 的版本線涵蓋 `8.0 U3c` 等所有 Update；`8.0.3` 涵蓋 U3 的所有字母與 Patch。
    fn same_line(&self, base: &Version, v: &Version) -> Option<bool> {
        match (base, v) {
            (Version::Esxi(b), Version::Esxi(x)) => match (b.release, x.release) {
                (Some(b), Some(x)) => Some(
                    (b.major, b.minor) == (x.major, x.minor)
                        && (b.update == 0 || b.update == x.update),
                ),
                _ => self.covers(base, v),
            },
            _ => None,
        }
    }
}
