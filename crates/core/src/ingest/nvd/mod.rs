//! NVD API 2.0（補充）：依目錄每組 CPE 以 `virtualMatchString` 增量查詢，
//! 取得 NVD CPE 設定、NVD CVSS 與 `vulnStatus`。整個來源一個交易，失敗時資料與游標不變。

pub mod parse;

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use self::parse::NvdCve;
use super::compress;
use super::cursor;
use super::relevance::Relevance;
use super::source::{SyncCtx, SyncStats};
use super::store;
use crate::error::CoreError;

pub const SOURCE: &str = "nvd";
pub const RESULTS_PER_PAGE: u64 = 2000;
/// `lastModStartDate`～`lastModEndDate` 的上限。
pub const MAX_RANGE_DAYS: i64 = 120;
/// 請求間隔：有 API key 50 次/30 秒、無 key 5 次/30 秒。
pub const INTERVAL_WITH_KEY: Duration = Duration::from_millis(700);
pub const INTERVAL_WITHOUT_KEY: Duration = Duration::from_millis(6100);
const API_KEY_HEADER: &str = "apiKey";
const DATE_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3fZ";

/// 每組 CPE（`part:vendor:product`）上次成功查詢到的時間點。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NvdCursor {
    pub cpes: BTreeMap<String, DateTime<Utc>>,
}

/// 一次查詢的時間區間；None 表示不帶時間參數（首次取得全部）。
type Window = Option<(DateTime<Utc>, DateTime<Utc>)>;

/// 起點到現在切成 ≤ 120 天的區段。
pub fn windows(start: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Vec<Window> {
    let Some(mut from) = start else {
        return vec![None];
    };
    let step = chrono::Duration::days(MAX_RANGE_DAYS);
    let mut out = Vec::new();
    while from < now {
        let to = (from + step).min(now);
        out.push(Some((from, to)));
        from = to;
    }
    out
}

struct Requester<'a, 'c> {
    ctx: &'a SyncCtx<'c>,
    interval: Duration,
    sent: u64,
}

impl Requester<'_, '_> {
    async fn page(
        &mut self,
        cpe: &str,
        window: Window,
        start_index: u64,
    ) -> Result<parse::Page, CoreError> {
        if self.sent > 0 {
            tokio::time::sleep(self.interval).await;
        }
        self.sent += 1;
        let mut query: Vec<(&str, String)> = vec![
            ("virtualMatchString", cpe.to_string()),
            ("resultsPerPage", RESULTS_PER_PAGE.to_string()),
            ("startIndex", start_index.to_string()),
        ];
        if let Some((from, to)) = window {
            query.push(("lastModStartDate", from.format(DATE_FORMAT).to_string()));
            query.push(("lastModEndDate", to.format(DATE_FORMAT).to_string()));
        }
        let key = self.ctx.nvd_api_key;
        let url = url::Url::parse_with_params(&self.ctx.endpoints.nvd, &query)
            .map_err(|e| CoreError::Invalid(format!("NVD 網址錯誤：{e}")))?;
        let resp = self
            .ctx
            .http
            .send(|c| {
                let req = c.get(url.clone());
                match key {
                    Some(k) => req.header(API_KEY_HEADER, k),
                    None => req,
                }
            })
            .await?;
        parse::parse_page(&self.ctx.http.bytes(resp).await?)
    }
}

pub async fn sync(ctx: &SyncCtx<'_>, conn: &mut Connection) -> Result<SyncStats, CoreError> {
    let mut stats = SyncStats::default();
    let previous: NvdCursor = cursor::get(conn, SOURCE)?.unwrap_or_default();
    let interval = ctx.nvd_interval.unwrap_or(if ctx.nvd_api_key.is_some() {
        INTERVAL_WITH_KEY
    } else {
        INTERVAL_WITHOUT_KEY
    });
    let mut req = Requester {
        ctx,
        interval,
        sent: 0,
    };
    let now = ctx.now;
    let now_str = ctx.now_str();
    let mut fetched: Vec<Value> = Vec::new();
    let mut next = previous.clone();
    for cpe in catalog_cpes(ctx) {
        let start = ctx.options.since.map_or_else(
            || previous.cpes.get(&cpe).copied(),
            |d| d.and_hms_opt(0, 0, 0).map(|t| t.and_utc()),
        );
        let match_string = format!("cpe:2.3:{cpe}");
        for window in windows(start, now) {
            let mut start_index = 0;
            loop {
                let page = req.page(&match_string, window, start_index).await?;
                let count = page.vulnerabilities.len() as u64;
                let total = page.total_results;
                fetched.extend(page.vulnerabilities.into_iter().map(|item| item.cve));
                start_index += count;
                if count == 0 || start_index >= total {
                    break;
                }
            }
        }
        next.cpes.insert(cpe, now);
    }
    // 全部抓完才開啟寫入交易：抓取期間（含速率限制的等待）不持有資料庫寫入鎖，
    // 其他命令（finding set、pending resolve、match）不會因此逾時；失敗時游標不變
    let tx = conn.transaction()?;
    for raw in &fetched {
        apply_cve(&tx, raw, &now_str, &mut stats)?;
    }
    cursor::set(&tx, SOURCE, &next, &now_str)?;
    ctx.commit(tx, stats.changes() > 0)?;
    Ok(stats)
}

/// 目錄中的 CPE（`part:vendor:product`），去重排序。
fn catalog_cpes(ctx: &SyncCtx<'_>) -> Vec<String> {
    let mut cpes: Vec<String> = ctx
        .catalog
        .products()
        .iter()
        .flat_map(|p| p.cpe.iter())
        .map(|c| format!("{}:{}:{}", c.part, c.vendor, c.product))
        .collect();
    cpes.sort();
    cpes.dedup();
    cpes
}

/// 寫入一筆 NVD CVE：本機沒有時新增（原因 nvd），更新 NVD 欄位、NVD CVSS 與 configurations。
fn apply_cve(
    conn: &Connection,
    raw: &Value,
    now: &str,
    stats: &mut SyncStats,
) -> Result<(), CoreError> {
    let cve: NvdCve = match parse::parse_cve(raw) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "略過無法解析的 NVD 紀錄");
            return Ok(());
        }
    };
    let inserted = store::ensure_stub(conn, &cve.id, Relevance::Nvd, cve.description_en(), now)?;
    let packed = compress::compress(&serde_json::to_vec(raw).unwrap_or_default())?;
    conn.execute(
        "UPDATE cve SET raw_nvd_json = ?2, nvd_vuln_status = ?3, nvd_last_modified = ?4,
                description_en = COALESCE(description_en, ?5), updated_at = ?6
         WHERE id = ?1",
        params![
            cve.id,
            packed,
            cve.vuln_status,
            cve.last_modified,
            cve.description_en(),
            now
        ],
    )?;
    replace_metrics(conn, &cve)?;
    replace_matches(conn, &cve)?;
    if inserted {
        stats.added += 1;
    } else {
        stats.updated += 1;
    }
    Ok(())
}

fn replace_metrics(conn: &Connection, cve: &NvdCve) -> Result<(), CoreError> {
    conn.execute(
        "DELETE FROM cve_metric WHERE cve_id = ?1 AND source = 'nvd'",
        params![cve.id],
    )?;
    let mut stmt = conn.prepare_cached(
        "INSERT INTO cve_metric (cve_id, source, provider, version, base_score, vector, severity)
         VALUES (?1, 'nvd', ?2, ?3, ?4, ?5, ?6)",
    )?;
    for m in cve.primary_metrics() {
        stmt.execute(params![
            cve.id,
            m.provider,
            m.version,
            m.base_score,
            m.vector,
            m.severity
        ])?;
    }
    Ok(())
}

fn replace_matches(conn: &Connection, cve: &NvdCve) -> Result<(), CoreError> {
    conn.execute(
        "DELETE FROM cve_nvd_match WHERE cve_id = ?1",
        params![cve.id],
    )?;
    let mut stmt = conn.prepare_cached(
        "INSERT INTO cve_nvd_match (cve_id, config_group, config_operator, node_group, node_operator,
                                    node_negate, criteria_cpe, vuln, ver_start_incl, ver_start_excl,
                                    ver_end_incl, ver_end_excl)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?;
    for (gi, config) in cve.configurations.iter().enumerate() {
        for (ni, node) in config.nodes.iter().enumerate() {
            for m in &node.cpe_match {
                stmt.execute(params![
                    cve.id,
                    gi as i64,
                    config.operator,
                    ni as i64,
                    node.operator,
                    node.negate,
                    m.criteria,
                    m.vulnerable,
                    m.version_start_including,
                    m.version_start_excluding,
                    m.version_end_including,
                    m.version_end_excluding
                ])?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn windows_are_split_at_120_days() {
        let now = t("2026-11-07T00:00:00Z");
        assert_eq!(windows(None, now), vec![None]);
        let w = windows(Some(t("2026-01-01T00:00:00Z")), now);
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].unwrap().1, t("2026-05-01T00:00:00Z"));
        assert_eq!(w.last().unwrap().unwrap().1, now);
        assert!(windows(Some(now), now).is_empty());
    }

    #[test]
    fn date_format_matches_nvd() {
        let s = t("2026-10-01T00:00:00Z").format(DATE_FORMAT).to_string();
        assert_eq!(s, "2026-10-01T00:00:00.000Z");
    }
}
