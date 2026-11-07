//! CSAF 文件與其狀態、修正版本寫入資料庫。同一追蹤 ID 只保留最新版本。

use std::cmp::Ordering;
use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};

use super::fetch::Document;
use super::status::Extracted;
use crate::error::CoreError;
use crate::ingest::compress;
use crate::ingest::relevance::Relevance;
use crate::ingest::store::ensure_stub;

/// CSAF `tracking.version`（`1.1.0` 或整數）比較；數字段逐一比較，非數字以字串比較。
pub fn version_cmp(a: &str, b: &str) -> Ordering {
    let parts = |s: &str| -> Vec<String> {
        s.trim()
            .split(['.', '+', '-'])
            .map(str::to_string)
            .collect()
    };
    let (pa, pb) = (parts(a), parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).map_or("0", String::as_str);
        let y = pb.get(i).map_or("0", String::as_str);
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(m), Ok(n)) => m.cmp(&n),
            _ => x.cmp(y),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

pub fn stored_version(
    conn: &Connection,
    provider: &str,
    tracking_id: &str,
) -> Result<Option<String>, CoreError> {
    Ok(conn
        .query_row(
            "SELECT version FROM csaf_document WHERE provider = ?1 AND tracking_id = ?2",
            params![provider, tracking_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// 取代（或新增）一份文件；回傳新增的 CVE 數（本機原本沒有、由此公告加入）。
pub fn replace_document(
    conn: &Connection,
    provider: &str,
    doc: &Document,
    extracted: &Extracted,
    product_ids: &HashMap<String, i64>,
    now: &str,
) -> Result<u64, CoreError> {
    upsert_document(conn, provider, doc, now)?;
    replace_statuses(
        conn,
        provider,
        &doc.tracking_id,
        doc.title.as_deref(),
        extracted,
        product_ids,
        now,
    )
}

fn upsert_document(
    conn: &Connection,
    provider: &str,
    doc: &Document,
    now: &str,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO csaf_document (provider, tracking_id, version, title, tlp, published, updated,
                                    url, sha_ok, raw, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT (provider, tracking_id) DO UPDATE SET version = excluded.version,
             title = excluded.title, tlp = excluded.tlp, published = excluded.published,
             updated = excluded.updated, url = excluded.url, sha_ok = excluded.sha_ok,
             raw = excluded.raw, fetched_at = excluded.fetched_at",
        params![
            provider,
            doc.tracking_id,
            doc.version,
            doc.title,
            doc.tlp,
            doc.published,
            doc.updated,
            doc.url,
            doc.sha_ok,
            compress::compress(&doc.raw)?,
            now
        ],
    )?;
    Ok(())
}

/// 以解析結果取代一份文件的狀態與修正版本（也用於目錄變更後重新對應）。
pub fn replace_statuses(
    conn: &Connection,
    provider: &str,
    tracking_id: &str,
    title: Option<&str>,
    extracted: &Extracted,
    product_ids: &HashMap<String, i64>,
    now: &str,
) -> Result<u64, CoreError> {
    for table in ["csaf_status", "csaf_fix"] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE provider = ?1 AND tracking_id = ?2"),
            params![provider, tracking_id],
        )?;
    }
    let mut status = conn.prepare_cached(
        "INSERT INTO csaf_status (provider, tracking_id, cve_id, catalog_product_id, csaf_product_id,
                                  version_expr, kind, status, justification)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for s in &extracted.statuses {
        let Some(pid) = product_ids.get(&s.catalog_key) else {
            continue;
        };
        status.execute(params![
            provider,
            tracking_id,
            s.cve_id,
            pid,
            s.csaf_product_id,
            s.version_expr,
            s.kind.as_str(),
            s.status,
            s.justification
        ])?;
    }
    let mut fix = conn.prepare_cached(
        "INSERT INTO csaf_fix (provider, tracking_id, cve_id, catalog_product_id, fixed_version,
                               details, url)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for f in &extracted.fixes {
        let Some(pid) = product_ids.get(&f.catalog_key) else {
            continue;
        };
        fix.execute(params![
            provider,
            tracking_id,
            f.cve_id,
            pid,
            f.fixed_version,
            f.details,
            f.url
        ])?;
    }
    let mut new_cves = 0;
    for cve in &extracted.cves {
        if ensure_stub(conn, cve, Relevance::Csaf, title, now)? {
            new_cves += 1;
        }
    }
    Ok(new_cves)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering() {
        assert_eq!(version_cmp("1.1.0", "1.0.0"), Ordering::Greater);
        assert_eq!(version_cmp("2", "10"), Ordering::Less);
        assert_eq!(version_cmp("1.0", "1.0.0"), Ordering::Equal);
        assert_eq!(version_cmp("1.0.0", "1.0.0"), Ordering::Equal);
    }
}
