//! `cvewatch cve show` / `cve search` 的資料查詢（輸出格式在執行檔）。

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use super::compress;
use super::cvelist::search_index::{self, IndexEntry};
use super::cvss::{self, MetricRow};
use crate::error::CoreError;

/// 顯示的參考連結上限。
pub const MAX_REFERENCES: usize = 10;
/// `cve search` 顯示的結果上限。
pub const MAX_SEARCH_RESULTS: usize = 50;

#[derive(Debug, Clone, PartialEq)]
pub struct CveDetail {
    pub id: String,
    pub state: String,
    pub assigner_short: Option<String>,
    pub date_published: Option<String>,
    pub date_updated: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub relevance_reason: String,
    pub has_record: bool,
    pub nvd_status: Option<String>,
    pub nvd_match_count: i64,
    pub metrics: Vec<MetricRow>,
    pub affected: Vec<AffectedRow>,
    pub ssvc: Option<(Option<String>, Option<String>, Option<String>)>,
    pub kev: Option<KevRow>,
    pub epss: Option<(f64, f64, String)>,
    pub advisories: Vec<AdvisoryRow>,
    pub fixes: Vec<FixRow>,
    pub osv: Vec<String>,
    pub references: Vec<String>,
}

impl CveDetail {
    pub fn preferred_metric(&self) -> Option<&MetricRow> {
        cvss::preferred(&self.metrics)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedRow {
    pub source: String,
    pub vendor: String,
    pub product: String,
    pub default_status: Option<String>,
    pub versions_json: String,
    pub purl: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KevRow {
    pub date_added: String,
    pub due_date: Option<String>,
    pub ransomware: bool,
    pub required_action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisoryRow {
    pub provider: String,
    pub tracking_id: String,
    pub doc_version: String,
    pub product: String,
    pub version_expr: String,
    pub status: String,
    pub justification: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixRow {
    pub provider: String,
    pub product: String,
    pub fixed_version: Option<String>,
    pub details: Option<String>,
    pub url: Option<String>,
}

/// CVE ID 標準化（大寫、去空白）；格式不符時為錯誤。
pub fn normalize_id(raw: &str) -> Result<String, CoreError> {
    let id = raw.trim().to_uppercase();
    let mut parts = id.split('-');
    let ok = parts.next() == Some("CVE")
        && parts
            .next()
            .is_some_and(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()))
        && parts
            .next()
            .is_some_and(|n| n.len() >= 4 && n.chars().all(|c| c.is_ascii_digit()))
        && parts.next().is_none();
    if ok {
        Ok(id)
    } else {
        Err(CoreError::Invalid(format!(
            "`{raw}` 不是有效的 CVE ID（例如 CVE-2024-3400）"
        )))
    }
}

type BaseRow = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<Vec<u8>>,
    Option<String>,
);

pub fn load(conn: &Connection, id: &str) -> Result<Option<CveDetail>, CoreError> {
    let base: Option<BaseRow> = conn
        .query_row(
            "SELECT state, assigner_short, date_published, date_updated, title, description_en,
                    relevance_reason, raw_cna_json, nvd_vuln_status
             FROM cve WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ))
            },
        )
        .optional()?;
    let Some((state, assigner, published, updated, title, description, reason, raw, nvd)) = base
    else {
        return Ok(None);
    };
    let references = match &raw {
        Some(blob) => references(&compress::decompress(blob)?),
        None => Vec::new(),
    };
    Ok(Some(CveDetail {
        id: id.to_string(),
        state,
        assigner_short: assigner,
        date_published: published,
        date_updated: updated,
        title,
        description,
        relevance_reason: reason,
        has_record: raw.is_some(),
        nvd_status: nvd,
        nvd_match_count: conn.query_row(
            "SELECT count(*) FROM cve_nvd_match WHERE cve_id = ?1",
            params![id],
            |r| r.get(0),
        )?,
        metrics: cvss::load_metrics(conn, id)?,
        affected: affected(conn, id)?,
        ssvc: conn
            .query_row(
                "SELECT exploitation, automatable, technical_impact FROM cve_ssvc WHERE cve_id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?,
        kev: kev(conn, id)?,
        epss: conn
            .query_row(
                "SELECT score, percentile, score_date FROM epss WHERE cve_id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?,
        advisories: advisories(conn, id)?,
        fixes: fixes(conn, id)?,
        osv: osv(conn, id)?,
        references,
    }))
}

/// 原始 CVE 紀錄中 CNA 的參考連結。
pub fn references(raw: &[u8]) -> Vec<String> {
    let Ok(v) = serde_json::from_slice::<Value>(raw) else {
        return Vec::new();
    };
    v.pointer("/containers/cna/references")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| r.get("url").and_then(Value::as_str).map(str::to_string))
        .take(MAX_REFERENCES)
        .collect()
}

fn affected(conn: &Connection, id: &str) -> Result<Vec<AffectedRow>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT source, COALESCE(vendor_raw, ''), COALESCE(product_raw, ''), default_status,
                versions_json, purl
         FROM cve_affected WHERE cve_id = ?1 ORDER BY rowid",
    )?;
    let rows = stmt
        .query_map(params![id], |r| {
            Ok(AffectedRow {
                source: r.get(0)?,
                vendor: r.get(1)?,
                product: r.get(2)?,
                default_status: r.get(3)?,
                versions_json: r.get(4)?,
                purl: r.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

fn kev(conn: &Connection, id: &str) -> Result<Option<KevRow>, CoreError> {
    Ok(conn
        .query_row(
            "SELECT date_added, due_date, known_ransomware, required_action FROM kev WHERE cve_id = ?1",
            params![id],
            |r| {
                Ok(KevRow {
                    date_added: r.get(0)?,
                    due_date: r.get(1)?,
                    ransomware: r.get(2)?,
                    required_action: r.get(3)?,
                })
            },
        )
        .optional()?)
}

fn advisories(conn: &Connection, id: &str) -> Result<Vec<AdvisoryRow>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT s.provider, s.tracking_id, d.version, p.display_name, s.version_expr, s.status,
                s.justification
         FROM csaf_status s
         JOIN csaf_document d ON d.provider = s.provider AND d.tracking_id = s.tracking_id
         JOIN catalog_product p ON p.id = s.catalog_product_id
         WHERE s.cve_id = ?1 ORDER BY s.provider, s.tracking_id, s.status, s.version_expr",
    )?;
    let rows = stmt
        .query_map(params![id], |r| {
            Ok(AdvisoryRow {
                provider: r.get(0)?,
                tracking_id: r.get(1)?,
                doc_version: r.get(2)?,
                product: r.get(3)?,
                version_expr: r.get(4)?,
                status: r.get(5)?,
                justification: r.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

fn fixes(conn: &Connection, id: &str) -> Result<Vec<FixRow>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT f.provider, p.display_name, f.fixed_version, f.details, f.url
         FROM csaf_fix f JOIN catalog_product p ON p.id = f.catalog_product_id
         WHERE f.cve_id = ?1 ORDER BY f.provider",
    )?;
    let rows = stmt
        .query_map(params![id], |r| {
            Ok(FixRow {
                provider: r.get(0)?,
                product: r.get(1)?,
                fixed_version: r.get(2)?,
                details: r.get(3)?,
                url: r.get(4)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// 命中此 CVE 的 OSV 套件（`資產 套件 → OSV ID`）。
fn osv(conn: &Connection, id: &str) -> Result<Vec<String>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT a.asset_tag || ' ' || c.purl || ' → ' || h.osv_id
         FROM osv_hit h, json_each(h.cve_ids_json) j
         JOIN component c ON c.id = h.component_id JOIN asset a ON a.id = c.asset_id
         WHERE j.value = ?1 ORDER BY 1",
    )?;
    let rows = stmt
        .query_map(params![id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// 搜尋結果：索引中的紀錄與是否已在本機。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub entry: IndexEntry,
    pub local: bool,
}

/// 先搜 baseline 索引；索引不存在時為 None（呼叫端改提示先 sync）。
pub fn search(
    conn: &Connection,
    cache_dir: &Path,
    keyword: &str,
) -> Result<Option<Vec<SearchHit>>, CoreError> {
    if keyword.trim().is_empty() {
        return Err(CoreError::Invalid("搜尋關鍵字不可為空".into()));
    }
    let Some(entries) = search_index::search(cache_dir, keyword)? else {
        return Ok(None);
    };
    let mut stmt = conn.prepare_cached("SELECT EXISTS (SELECT 1 FROM cve WHERE id = ?1)")?;
    let mut hits = Vec::with_capacity(entries.len());
    for entry in entries {
        let local: bool = stmt.query_row(params![entry.id], |r| r.get(0))?;
        hits.push(SearchHit { entry, local });
    }
    Ok(Some(hits))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_normalization() {
        assert_eq!(normalize_id(" cve-2024-3400 ").unwrap(), "CVE-2024-3400");
        assert_eq!(normalize_id("CVE-2026-104080").unwrap(), "CVE-2026-104080");
        for bad in [
            "CVE-24-3400",
            "CVE-2024-12",
            "GHSA-1",
            "CVE-2024-3400-1",
            "",
        ] {
            assert!(normalize_id(bad).is_err(), "{bad}");
        }
    }
}
