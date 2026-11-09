//! 版本分支對應（22 design D3）：以可解釋的前綴規則把清冊元件的版本對應到一個版本分支。
//!
//! ```text
//! tokens(s)       = s 以 . - _ 空白 ( ) / 切開、去掉空段、轉小寫
//! token_eq(c, v)  = c 為純數字 ? v 開頭的連續數字存在且數值等於 c : c 與 v 相等
//! is_prefix(c, v) = tokens(c) 的段數 ≤ tokens(v) 的段數，且逐段 token_eq
//! ```
//!
//! 規則一：分支名稱為版本的前綴，取段數最多者；規則二（只用於 endoflife.date）：以分支的
//! 最新版本名稱做同樣的比對。沒有唯一結果時不猜測。

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::version_text::esxi;

const SEPARATORS: [char; 7] = ['.', '-', '_', ' ', '(', ')', '/'];

/// 只填 build 的 ESXi 版本（`build 24414501`、`(build-24414501)`、`24414501`）。
static ESXI_BUILD_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\(?(?:build[- ]?)?(\d{6,9})\)?$").expect("固定的正規表示式必定可編譯")
});

pub fn tokens(s: &str) -> Vec<String> {
    s.split(|c: char| SEPARATORS.contains(&c) || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// 分支段 `c` 與版本段 `v` 是否相等：`c` 為純數字時只比 `v` 開頭的連續數字（`4a`、`1p3`）。
pub fn token_eq(c: &str, v: &str) -> bool {
    if !c.is_empty() && c.chars().all(|ch| ch.is_ascii_digit()) {
        let lead: String = v.chars().take_while(char::is_ascii_digit).collect();
        if lead.is_empty() {
            return false;
        }
        return match (c.parse::<u128>(), lead.parse::<u128>()) {
            (Ok(a), Ok(b)) => a == b,
            // 超長數字：去掉前導 0 後以字串比較
            _ => c.trim_start_matches('0') == lead.trim_start_matches('0'),
        };
    }
    c.eq_ignore_ascii_case(v)
}

/// 分支名稱是否為版本的前綴；回傳分支的段數。
pub fn prefix_len(cycle: &str, version: &str) -> Option<usize> {
    let c = tokens(cycle);
    let v = tokens(version);
    (!c.is_empty() && c.len() <= v.len() && c.iter().zip(&v).all(|(a, b)| token_eq(a, b)))
        .then_some(c.len())
}

/// 對應結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleMatch {
    /// 對應到分支（`candidates` 中的索引）
    Found(usize),
    /// 沒有符合或最長者不唯一
    Unmapped,
}

/// 在候選名稱中找段數最多的前綴；最多段數不唯一時為 None。
fn longest_prefix<'a>(names: impl Iterator<Item = &'a str>, version: &str) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    let mut tie = false;
    for (i, name) in names.enumerate() {
        let Some(len) = prefix_len(name, version) else {
            continue;
        };
        match best {
            Some((_, b)) if len < b => {}
            Some((_, b)) if len == b => tie = true,
            _ => {
                best = Some((i, len));
                tie = false;
            }
        }
    }
    best.filter(|_| !tie).map(|(i, _)| i)
}

/// 規則一：分支名稱為版本的前綴。
pub fn by_name<'a>(names: impl Iterator<Item = &'a str>, version: &str) -> CycleMatch {
    longest_prefix(names, version).map_or(CycleMatch::Unmapped, CycleMatch::Found)
}

/// 規則一，沒有符合時再以規則二（分支的最新版本名稱；None 為沒有最新版本）。
pub fn by_name_or_latest<'a>(
    candidates: &'a [(&'a str, Option<&'a str>)],
    version: &str,
) -> CycleMatch {
    let names = candidates.iter().map(|(n, _)| *n);
    if let Some(i) = longest_prefix(names, version) {
        return CycleMatch::Found(i);
    }
    let any_name_match = candidates
        .iter()
        .any(|(n, _)| prefix_len(n, version).is_some());
    if any_name_match {
        // 規則一有符合但不唯一：不再以規則二猜測
        return CycleMatch::Unmapped;
    }
    let latest: Vec<(usize, &str)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, (_, l))| l.map(|l| (i, l)))
        .collect();
    match longest_prefix(latest.iter().map(|(_, l)| *l), version) {
        Some(j) => CycleMatch::Found(latest[j].0),
        None => CycleMatch::Unmapped,
    }
}

/// 比對前的版本：`esxi` 版本格式的名稱換成 `主.次`；只填 build 時以目錄的 build 對照表換成
/// 名稱，對照不到為 None（無法對應）。其他版本格式原樣使用。
pub fn comparable_version(
    version_raw: &str,
    scheme: &str,
    esxi_builds: &BTreeMap<u64, String>,
) -> Option<String> {
    let v = version_raw.trim();
    if scheme != "esxi" {
        return Some(v.to_string());
    }
    let named = match ESXI_BUILD_ONLY.captures(&v.to_lowercase()) {
        Some(caps) => {
            let build: u64 = caps[1].parse().ok()?;
            esxi_builds.get(&build)?.clone()
        }
        None => v.to_string(),
    };
    match esxi::parse_release(&named) {
        Some(r) => Some(format!("{}.{}", r.major, r.minor)),
        None => Some(named),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(names: &[&str], version: &str) -> Option<String> {
        match by_name(names.iter().copied(), version) {
            CycleMatch::Found(i) => Some(names[i].to_string()),
            CycleMatch::Unmapped => None,
        }
    }

    fn map_latest(cands: &[(&str, Option<&str>)], version: &str) -> Option<String> {
        match by_name_or_latest(cands, version) {
            CycleMatch::Found(i) => Some(cands[i].0.to_string()),
            CycleMatch::Unmapped => None,
        }
    }

    #[test]
    fn version_within_a_cycle() {
        assert_eq!(
            map(&["10.2", "11.1", "11.2"], "11.1.6-h3").as_deref(),
            Some("11.1")
        );
        assert_eq!(
            map(&["10.2", "11.1", "11.2"], "11.1.4-h7").as_deref(),
            Some("11.1")
        );
    }

    #[test]
    fn longest_cycle_wins() {
        assert_eq!(map(&["9", "9.16.1"], "9.16.1.2").as_deref(), Some("9.16.1"));
    }

    #[test]
    fn cycle_found_through_the_latest_version() {
        let cands = [
            ("2025", Some("10.0.26100")),
            ("23h2-ac", Some("10.0.25398")),
            ("2022", Some("10.0.20348")),
        ];
        assert_eq!(
            map_latest(&cands, "10.0.20348.2700").as_deref(),
            Some("2022")
        );
    }

    #[test]
    fn no_matching_cycle() {
        assert_eq!(map(&["7", "8"], "6.4"), None);
        assert_eq!(
            map_latest(&[("7", Some("7.4")), ("8", Some("8.4"))], "6.4"),
            None
        );
    }

    #[test]
    fn design_examples() {
        let esxi = comparable_version("8.0 U3c", "esxi", &BTreeMap::new()).unwrap();
        assert_eq!(map(&["8.0", "7.0"], &esxi).as_deref(), Some("8.0"));
        assert_eq!(map(&["17", "16", "15"], "16.4").as_deref(), Some("16"));
        assert_eq!(
            map(&["24.04", "22.04"], "22.04.4 LTS").as_deref(),
            Some("22.04")
        );
        assert_eq!(
            map(&["9.16.1", "9.15.1"], "9.16.1P3").as_deref(),
            Some("9.16.1")
        );
        assert_eq!(map(&["17.9", "17.12"], "17.9.4a").as_deref(), Some("17.9"));
        assert_eq!(map(&["8", "11", "17"], "8u421").as_deref(), Some("8"));
        // 數字段以數值比較：分支 1 不等於版本段 10
        assert_eq!(map(&["1", "2"], "10.1"), None);
        // SQL Server 13.0-sp3：sp3 不是數字且不等於 6300 → 不符合
        assert_eq!(map(&["13.0-sp3", "13.0-sp3-acp"], "13.0.6300.2"), None);
    }

    #[test]
    fn tie_is_unmapped() {
        assert_eq!(map(&["1.2", "01.2"], "1.2.3"), None);
        let cands = [("a", Some("10.0")), ("b", Some("10.0"))];
        assert_eq!(map_latest(&cands, "10.0.1"), None);
    }

    #[test]
    fn esxi_build_only_uses_the_catalog_table() {
        let builds = BTreeMap::from([(24414501, "8.0 U3c".to_string())]);
        assert_eq!(
            comparable_version("build 24414501", "esxi", &builds).as_deref(),
            Some("8.0")
        );
        assert_eq!(comparable_version("24585383", "esxi", &builds), None);
        assert_eq!(
            comparable_version("ESXi80U3d-24585383", "esxi", &builds).as_deref(),
            Some("8.0")
        );
        assert_eq!(
            comparable_version(" 7.2.2-72806 ", "dsm", &builds).as_deref(),
            Some("7.2.2-72806")
        );
    }
}
