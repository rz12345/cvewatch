//! cvelistV5 GitHub Releases 的 asset 選擇。
//!
//! - 每小時 release `cve_YYYY-MM-DD_HHMMZ`：`YYYY-MM-DD_all_CVEs_at_midnight.zip.zip`（baseline）
//!   與 `YYYY-MM-DD_delta_CVEs_at_HHMMZ.zip`（當日累積 delta）
//! - 每日 release `cve_YYYY-MM-DD_at_end_of_day`：`YYYY-MM-DD_delta_CVEs_at_end_of_day.zip`
//!
//! 注意 00:00Z 的 release 帶的是前一天的 baseline，所以 baseline 日期一律取自 asset 名稱。

use chrono::NaiveDate;
use serde::Deserialize;

use crate::error::CoreError;

/// 每次 sync 讀取的 release 數（約一天份）。
pub const RELEASES_PER_PAGE: u32 = 30;

const BASELINE_SUFFIXES: [&str; 2] = ["_all_CVEs_at_midnight.zip.zip", "_all_CVEs_at_midnight.zip"];
const DELTA_MARKER: &str = "_delta_CVEs_at_";
const END_OF_DAY: &str = "end_of_day";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    #[serde(default)]
    pub size: u64,
}

/// 選定要下載的 asset。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRef {
    pub tag: String,
    pub name: String,
    pub date: NaiveDate,
    pub size: u64,
}

pub fn parse_releases(bytes: &[u8]) -> Result<Vec<Release>, CoreError> {
    serde_json::from_slice(bytes)
        .map_err(|e| CoreError::Invalid(format!("GitHub releases 回應格式錯誤：{e}")))
}

fn date_prefix(name: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(name.get(..10)?, "%Y-%m-%d").ok()
}

/// 最新的 baseline（列表最新在前）。
pub fn latest_baseline(releases: &[Release]) -> Option<AssetRef> {
    releases.iter().find_map(|r| {
        r.assets.iter().find_map(|a| {
            BASELINE_SUFFIXES
                .iter()
                .any(|s| a.name.ends_with(s))
                .then(|| date_prefix(&a.name))
                .flatten()
                .map(|date| AssetRef {
                    tag: r.tag_name.clone(),
                    name: a.name.clone(),
                    date,
                    size: a.size,
                })
        })
    })
}

/// 某日最新的每小時 delta（不含 end_of_day）。
pub fn latest_hourly_delta(releases: &[Release], day: NaiveDate) -> Option<AssetRef> {
    let prefix = format!("{}{DELTA_MARKER}", day.format("%Y-%m-%d"));
    releases.iter().find_map(|r| {
        r.assets
            .iter()
            .find(|a| a.name.starts_with(&prefix) && !a.name.contains(END_OF_DAY))
            .map(|a| AssetRef {
                tag: r.tag_name.clone(),
                name: a.name.clone(),
                date: day,
                size: a.size,
            })
    })
}

/// 某日的 end_of_day delta（固定命名，不需出現在列表中）。
pub fn end_of_day_delta(day: NaiveDate) -> AssetRef {
    let d = day.format("%Y-%m-%d");
    AssetRef {
        tag: format!("cve_{d}_at_end_of_day"),
        name: format!("{d}{DELTA_MARKER}{END_OF_DAY}.zip"),
        date: day,
        size: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(tag: &str, assets: &[&str]) -> Release {
        Release {
            tag_name: tag.into(),
            assets: assets
                .iter()
                .map(|n| Asset {
                    name: (*n).into(),
                    size: 1,
                })
                .collect(),
        }
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn baseline_date_comes_from_asset_name() {
        let list = vec![rel(
            "cve_2026-11-07_0000Z",
            &[
                "2026-11-06_all_CVEs_at_midnight.zip.zip",
                "2026-11-07_delta_CVEs_at_0000Z.zip",
            ],
        )];
        let b = latest_baseline(&list).unwrap();
        assert_eq!(b.date, d("2026-11-06"));
        assert_eq!(b.tag, "cve_2026-11-07_0000Z");
    }

    #[test]
    fn hourly_delta_skips_end_of_day_and_other_days() {
        let list = vec![
            rel(
                "cve_2026-11-07_0300Z",
                &["2026-11-07_delta_CVEs_at_0300Z.zip"],
            ),
            rel(
                "cve_2026-11-06_at_end_of_day",
                &["2026-11-06_delta_CVEs_at_end_of_day.zip"],
            ),
            rel(
                "cve_2026-11-06_2300Z",
                &["2026-11-06_delta_CVEs_at_2300Z.zip"],
            ),
        ];
        assert_eq!(
            latest_hourly_delta(&list, d("2026-11-07")).unwrap().name,
            "2026-11-07_delta_CVEs_at_0300Z.zip"
        );
        assert_eq!(
            latest_hourly_delta(&list, d("2026-11-06")).unwrap().name,
            "2026-11-06_delta_CVEs_at_2300Z.zip"
        );
        assert!(latest_hourly_delta(&list, d("2026-11-05")).is_none());
    }

    #[test]
    fn end_of_day_naming() {
        let a = end_of_day_delta(d("2026-11-06"));
        assert_eq!(a.tag, "cve_2026-11-06_at_end_of_day");
        assert_eq!(a.name, "2026-11-06_delta_CVEs_at_end_of_day.zip");
    }
}
