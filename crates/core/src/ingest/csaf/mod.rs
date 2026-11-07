//! 原廠公告：通用 CSAF 2.0/2.1 讀取器。新增原廠只需在設定檔加一筆 `[[sources.csaf]]`。
//!
//! 不提供 CSAF 的原廠可實作 [`AdvisorySource`]，輸出相同的 `csaf_status` / `csaf_fix` 列。

pub mod discovery;
pub mod fetch;
pub mod status;
pub mod store;
pub mod tree;
pub mod vers;

use std::cmp::Ordering;
use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use self::fetch::Fetched;
use super::compress;
use super::cursor;
use super::source::{SyncCtx, SyncStats};
use crate::config::CsafSourceConfig;
use crate::error::CoreError;

/// 每次 sync 每個來源最多處理的文件數（其餘下次繼續）。
pub const MAX_DOCUMENTS_PER_SYNC: usize = 200;
/// 首次同步只取最近這麼多天的文件，避免一次下載整個公告庫。
pub const INITIAL_WINDOW_DAYS: i64 = 365;

pub type SyncFuture<'a> = Pin<Box<dyn Future<Output = Result<SyncStats, CoreError>> + 'a>>;

/// 原廠公告來源。
pub trait AdvisorySource {
    /// 來源 id（`sync_run.source` 為 `csaf:<id>`）
    fn id(&self) -> &str;
    fn sync<'a>(&'a self, ctx: &'a SyncCtx<'_>, conn: &'a mut Connection) -> SyncFuture<'a>;
}

/// `sync_cursor` / `sync_run` 使用的來源名稱。
pub fn source_name(id: &str) -> String {
    format!("csaf:{id}")
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CsafCursor {
    /// 已處理到的文件時間（changes.csv / ROLIE updated / RSS pubDate）
    pub last_updated: Option<DateTime<Utc>>,
    /// 上次對應時的目錄雜湊；變更時以本機保存的原文重新對應既有公告
    #[serde(default)]
    pub catalog_hash: Option<String>,
}

/// 依設定讀取一個 CSAF 原廠。
pub struct CsafReader {
    pub config: CsafSourceConfig,
}

impl AdvisorySource for CsafReader {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn sync<'a>(&'a self, ctx: &'a SyncCtx<'_>, conn: &'a mut Connection) -> SyncFuture<'a> {
        Box::pin(run(&self.config, ctx, conn))
    }
}

async fn run(
    cfg: &CsafSourceConfig,
    ctx: &SyncCtx<'_>,
    conn: &mut Connection,
) -> Result<SyncStats, CoreError> {
    let mut stats = SyncStats::default();
    let source = source_name(&cfg.id);
    let previous: CsafCursor = cursor::get(conn, &source)?.unwrap_or_default();
    let since = match (ctx.options.since, previous.last_updated) {
        (Some(d), _) => d.and_hms_opt(0, 0, 0).map(|t| t.and_utc()),
        (None, Some(t)) => Some(t),
        (None, None) => Some(ctx.now - chrono::Duration::days(INITIAL_WINDOW_DAYS)),
    };
    let mut candidates = discovery::discover(ctx, cfg, since).await?;
    let total = candidates.len();
    if let Some(keep) = cap(&mut candidates, MAX_DOCUMENTS_PER_SYNC) {
        stats.note(format!(
            "共 {total} 份公告，本次處理最早的 {keep} 份，其餘下次繼續"
        ));
    }
    let now = ctx.now_str();
    let tx = conn.transaction()?;
    let fingerprint = catalog_fingerprint(ctx.catalog);
    let mut new_cves = 0;
    if previous.catalog_hash.as_deref() != Some(fingerprint.as_str()) {
        let (remapped, added) = remap_stored(&tx, ctx, &cfg.id, &now, &mut stats)?;
        new_cves += added;
        if remapped > 0 {
            stats.note(format!("目錄已變更，已重新對應 {remapped} 份既有公告"));
        }
    }
    let mut last = previous.last_updated;
    for c in &candidates {
        match fetch::fetch(ctx, c).await? {
            Fetched::Skipped(reason) => {
                tracing::warn!(provider = %cfg.id, reason = %reason, "略過 CSAF 文件");
                stats.note(reason);
            }
            Fetched::Accepted(doc) => {
                new_cves += apply(&tx, ctx, &cfg.id, &doc, &now, &mut stats)?;
            }
        }
        last = last.max(Some(c.updated));
    }
    if new_cves > 0 {
        stats.note(format!("由公告新增 {new_cves} 個 CVE"));
    }
    let next = CsafCursor {
        last_updated: last,
        catalog_hash: Some(fingerprint),
    };
    cursor::set(&tx, &source, &next, &now)?;
    tx.commit()?;
    Ok(stats)
}

/// 解析並寫入一份文件；同追蹤 ID 的本機版本較新或相同時不覆寫。
fn apply(
    conn: &Connection,
    ctx: &SyncCtx<'_>,
    provider: &str,
    doc: &fetch::Document,
    now: &str,
    stats: &mut SyncStats,
) -> Result<u64, CoreError> {
    let stored = store::stored_version(conn, provider, &doc.tracking_id)?;
    if let Some(v) = &stored {
        if store::version_cmp(&doc.version, v) != Ordering::Greater {
            return Ok(0);
        }
    }
    let extracted = extract(&doc.json, &doc.tracking_id, ctx, stats);
    let added = store::replace_document(conn, provider, doc, &extracted, ctx.product_ids, now)?;
    if stored.is_some() {
        stats.updated += 1;
    } else {
        stats.added += 1;
    }
    Ok(added)
}

/// product_tree 對應 + vulnerabilities 轉換；無法解析的區間與 CVE ID 記入提示。
fn extract(
    json: &Value,
    tracking_id: &str,
    ctx: &SyncCtx<'_>,
    stats: &mut SyncStats,
) -> status::Extracted {
    let tree = json.get("product_tree").cloned().unwrap_or(Value::Null);
    let (products, invalid) = tree::map_products(&tree, ctx.catalog);
    if invalid > 0 {
        stats.note(format!(
            "{tracking_id}：{invalid} 個無法解析的版本區間已略過"
        ));
    }
    let vulns = json
        .get("vulnerabilities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let extracted = status::extract(&vulns, &products);
    if !extracted.invalid_cves.is_empty() {
        stats.note(format!(
            "{tracking_id}：略過格式不符的 CVE ID {}",
            extracted.invalid_cves.join("、")
        ));
    }
    extracted
}

/// 以本機保存的原文重新對應某原廠的全部公告（目錄變更後）。回傳（文件數, 新增 CVE 數）。
fn remap_stored(
    conn: &Connection,
    ctx: &SyncCtx<'_>,
    provider: &str,
    now: &str,
    stats: &mut SyncStats,
) -> Result<(u64, u64), CoreError> {
    let docs: Vec<(String, Option<String>, Vec<u8>)> = {
        let mut stmt = conn.prepare(
            "SELECT tracking_id, title, raw FROM csaf_document WHERE provider = ?1 ORDER BY tracking_id",
        )?;
        let rows = stmt.query_map([provider], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let (mut count, mut added) = (0, 0);
    for (tracking_id, title, raw) in docs {
        let json: Value = match serde_json::from_slice(&compress::decompress(&raw)?) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(provider, tracking_id = %tracking_id, error = %e, "本機公告原文無法解析");
                continue;
            }
        };
        let extracted = extract(&json, &tracking_id, ctx, stats);
        added += store::replace_statuses(
            conn,
            provider,
            &tracking_id,
            title.as_deref(),
            &extracted,
            ctx.product_ids,
            now,
        )?;
        count += 1;
    }
    Ok((count, added))
}

/// 依時間排序的候選清單超過 `max` 時截斷；與第 `max` 份同時間的文件一併保留
/// （游標以 `>` 比較，若拆開，同時間的其餘文件下次會被濾掉）。有截斷時回傳保留數。
pub fn cap(candidates: &mut Vec<discovery::Candidate>, max: usize) -> Option<usize> {
    if candidates.len() <= max || max == 0 {
        return None;
    }
    let cut = candidates[max - 1].updated;
    let keep = candidates.iter().take_while(|c| c.updated <= cut).count();
    candidates.truncate(keep);
    Some(keep)
}

/// CSAF 對應所用的目錄識別（產品 key、CSAF 名稱、CPE）的雜湊；變更時重新對應既有公告。
pub fn catalog_fingerprint(catalog: &crate::catalog::Catalog) -> String {
    use sha2::{Digest, Sha256};
    let mut parts: Vec<String> = Vec::new();
    for p in catalog.products() {
        parts.extend(
            p.csaf
                .iter()
                .map(|n| format!("{}|csaf|{}|{}", p.key, n.vendor, n.product)),
        );
        parts.extend(
            p.cpe
                .iter()
                .map(|c| format!("{}|cpe|{}|{}", p.key, c.vendor, c.product)),
        );
    }
    parts.sort();
    data_encoding::HEXLOWER.encode(&Sha256::digest(parts.join("\n").as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(i: usize, t: &str) -> discovery::Candidate {
        discovery::Candidate {
            url: format!("u{i}"),
            updated: DateTime::parse_from_rfc3339(t).unwrap().with_timezone(&Utc),
            hash_urls: Vec::new(),
        }
    }

    #[test]
    fn cap_keeps_documents_sharing_the_cut_time() {
        let mut list = vec![
            cand(0, "2026-01-01T00:00:00Z"),
            cand(1, "2026-01-02T00:00:00Z"),
            cand(2, "2026-01-02T00:00:00Z"),
            cand(3, "2026-01-03T00:00:00Z"),
        ];
        assert_eq!(cap(&mut list, 2), Some(3));
        assert_eq!(list.len(), 3);
        let mut short = vec![cand(0, "2026-01-01T00:00:00Z")];
        assert_eq!(cap(&mut short, 2), None);
    }
}
