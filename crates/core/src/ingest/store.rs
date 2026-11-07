//! 將 CVE 5.x 紀錄寫入 `cve` 與 CNA/ADP 子表。NVD 欄位由 [`super::nvd`] 維護，這裡不動。

use rusqlite::{params, Connection, OptionalExtension};

use super::compress;
use super::cve5::{Container, CveRecord};
use super::cvss::MetricSource;
use super::relevance::Relevance;
use crate::catalog::{normalize, normalize_vendor};
use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    Inserted,
    Updated,
    /// 本機已有相同 `dateUpdated` 的紀錄（例如重複套用當日累積 delta）
    Unchanged,
}

/// 本機既有紀錄的摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingCve {
    pub date_updated: Option<String>,
    pub has_record: bool,
    pub reason: Relevance,
}

pub fn existing(conn: &Connection, id: &str) -> Result<Option<ExistingCve>, CoreError> {
    let row: Option<(Option<String>, bool, String)> = conn
        .query_row(
            "SELECT date_updated, raw_cna_json IS NOT NULL, relevance_reason FROM cve WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(row.map(|(date_updated, has_record, reason)| ExistingCve {
        date_updated,
        has_record,
        reason: Relevance::parse(&reason).unwrap_or(Relevance::Cna),
    }))
}

/// 寫入 CVE.org 紀錄。`reason` 為本次篩選結果；本機已有時保留較具體的原因
/// （見 [`Relevance::merge`]）。
pub fn upsert_record(
    conn: &Connection,
    record: &CveRecord,
    raw: &[u8],
    reason: Relevance,
    now: &str,
) -> Result<UpsertOutcome, CoreError> {
    let current = existing(conn, &record.id)?;
    if let Some(cur) = &current {
        if cur.has_record && !is_newer(record.date_updated.as_deref(), cur.date_updated.as_deref())
        {
            // 相同或較舊的版本（重複套用累積 delta、--since 重播）：不覆寫資料，
            // 但目錄變更後（rescan）的保留原因仍要更新
            let merged = Relevance::merge(cur.reason, reason);
            if merged == cur.reason {
                return Ok(UpsertOutcome::Unchanged);
            }
            conn.execute(
                "UPDATE cve SET relevance_reason = ?2, updated_at = ?3 WHERE id = ?1",
                params![record.id, merged.as_str(), now],
            )?;
            return Ok(UpsertOutcome::Updated);
        }
    }
    let reason = current
        .as_ref()
        .map_or(reason, |cur| Relevance::merge(cur.reason, reason));
    let packed = compress::compress(raw)?;
    conn.execute(
        "INSERT INTO cve (id, state, data_version, assigner_short, date_published, date_updated,
                          title, description_en, relevance_reason, raw_cna_json, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT (id) DO UPDATE SET state = excluded.state,
             data_version = excluded.data_version, assigner_short = excluded.assigner_short,
             date_published = excluded.date_published, date_updated = excluded.date_updated,
             title = excluded.title, description_en = excluded.description_en,
             relevance_reason = excluded.relevance_reason, raw_cna_json = excluded.raw_cna_json,
             updated_at = excluded.updated_at",
        params![
            record.id,
            record.state.as_str(),
            record.data_version,
            record.assigner_short,
            record.date_published,
            record.date_updated,
            record.title,
            record.description_en,
            reason.as_str(),
            packed,
            now
        ],
    )?;
    replace_children(conn, record)?;
    Ok(if current.is_some() {
        UpsertOutcome::Updated
    } else {
        UpsertOutcome::Inserted
    })
}

/// 來源紀錄的 `dateUpdated` 是否比本機新。兩者都能解析時比較時間，否則只要不同就視為較新。
fn is_newer(incoming: Option<&str>, stored: Option<&str>) -> bool {
    let parse = |s: Option<&str>| s.and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok());
    match (parse(incoming), parse(stored)) {
        (Some(new), Some(old)) => new > old,
        _ => incoming != stored,
    }
}

/// 尚未取得 CVE.org 紀錄的 CVE（由 NVD 或 CSAF 先發現）。已存在時不變更。
pub fn ensure_stub(
    conn: &Connection,
    id: &str,
    reason: Relevance,
    description: Option<&str>,
    now: &str,
) -> Result<bool, CoreError> {
    let inserted = conn.execute(
        "INSERT INTO cve (id, state, relevance_reason, description_en, updated_at)
         VALUES (?1, 'PUBLISHED', ?2, ?3, ?4)
         ON CONFLICT (id) DO NOTHING",
        params![id, reason.as_str(), description, now],
    )?;
    Ok(inserted > 0)
}

fn replace_children(conn: &Connection, record: &CveRecord) -> Result<(), CoreError> {
    conn.execute(
        "DELETE FROM cve_metric WHERE cve_id = ?1 AND source IN ('cna', 'adp')",
        params![record.id],
    )?;
    conn.execute(
        "DELETE FROM cve_affected WHERE cve_id = ?1",
        params![record.id],
    )?;
    conn.execute("DELETE FROM cve_ssvc WHERE cve_id = ?1", params![record.id])?;
    insert_container(conn, &record.id, MetricSource::Cna, &record.cna)?;
    for adp in &record.adp {
        insert_container(conn, &record.id, MetricSource::Adp, adp)?;
    }
    let ssvc = record
        .adp
        .iter()
        .find_map(|a| a.ssvc.as_ref().map(|s| (a, s)));
    if let Some((adp, s)) = ssvc {
        conn.execute(
            "INSERT INTO cve_ssvc (cve_id, provider, exploitation, automatable, technical_impact)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.id,
                adp.provider,
                s.exploitation,
                s.automatable,
                s.technical_impact
            ],
        )?;
    }
    Ok(())
}

fn insert_container(
    conn: &Connection,
    id: &str,
    source: MetricSource,
    c: &Container,
) -> Result<(), CoreError> {
    let mut metric = conn.prepare_cached(
        "INSERT INTO cve_metric (cve_id, source, provider, version, base_score, vector, severity)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for m in &c.metrics {
        metric.execute(params![
            id,
            source.as_str(),
            c.provider,
            m.version,
            m.base_score,
            m.vector,
            m.severity
        ])?;
    }
    let mut affected = conn.prepare_cached(
        "INSERT INTO cve_affected (cve_id, source, provider, vendor_raw, product_raw, vendor_norm,
                                   product_norm, default_status, versions_json, cpes_json, purl)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for a in &c.affected {
        let versions = serde_json::to_string(&a.versions).unwrap_or_else(|_| "[]".into());
        let cpes = serde_json::to_string(&a.cpes).unwrap_or_else(|_| "[]".into());
        affected.execute(params![
            id,
            source.as_str(),
            c.provider,
            a.vendor,
            a.product,
            a.vendor
                .as_deref()
                .map(normalize_vendor)
                .unwrap_or_default(),
            a.product.as_deref().map(normalize).unwrap_or_default(),
            a.default_status,
            versions,
            cpes,
            a.package_url
        ])?;
    }
    Ok(())
}
