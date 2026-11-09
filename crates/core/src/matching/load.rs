//! 比對所需資料一次載入記憶體（資料量為數千 CVE × 數十元件，全量重算即可）。

use std::collections::{BTreeSet, HashMap};

use rusqlite::{params, Connection};
use serde_json::Value;

use super::evidence::cna::AffectedEntry;
use super::evidence::csaf::{CsafFix, CsafRow};
use super::evidence::nvd::NvdMatch;
use super::evidence::osv::OsvHit;
use super::evidence::Source;
use super::scoring::ScoreInputs;
use crate::assets::model::Exposure;
use crate::assets::pending::PENDING_COMPONENT_SQL;
use crate::error::CoreError;
use crate::ingest::cve5::CveState;
use crate::ingest::cvss::{self, MetricRow, MetricSource};
use crate::ingest::relevance::{cpe_vendor_product, Relevance};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// 目錄產品 key
    Product(String),
    /// 套件 purl（不含版本）
    Package(String),
}

/// active 資產的元件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveComponent {
    pub id: i64,
    pub asset_tag: String,
    pub exposure: Exposure,
    /// 資產價值 MAX(C, I, A)（13 design D6；同等級內排序用）
    pub asset_value: u8,
    pub target: Target,
    pub version_raw: String,
    pub updated_at: String,
}

impl ActiveComponent {
    pub fn product_key(&self) -> Option<&str> {
        match &self.target {
            Target::Product(k) => Some(k),
            Target::Package(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CveInfo {
    pub id: String,
    pub state: CveState,
    /// 無法辨識的值為 None
    pub relevance: Option<Relevance>,
    pub description: Option<String>,
}

#[derive(Debug, Default)]
pub struct MatchData {
    pub components: Vec<ActiveComponent>,
    pub cves: Vec<CveInfo>,
    pub affected: HashMap<String, Vec<AffectedEntry>>,
    pub nvd: HashMap<String, Vec<NvdMatch>>,
    /// cve_id → product key → 狀態列
    pub csaf: CsafMap<CsafRow>,
    pub csaf_fix: CsafMap<CsafFix>,
    pub osv: HashMap<i64, Vec<OsvHit>>,
    /// OSV 命中資料損壞的元件：以空清單判定會誤把命中當成「不再列出」，比對時與等待 OSV 一樣排除
    pub osv_corrupt: BTreeSet<i64>,
    /// CNA／ADP affected 資料損壞的 CVE：以空清單判定會誤判為不受影響或不再列出，本次不比對
    pub corrupt_cves: BTreeSet<String>,
    /// 最近一次成功的 OSV 同步開始時間
    pub last_osv_sync: Option<String>,
    kev: HashMap<String, String>,
    epss: HashMap<String, f64>,
    metrics: HashMap<String, Vec<MetricRow>>,
}

/// 載入範圍；全量比對兩者皆為 None。人工處理單一 CVE 時只載入該 CVE 與指定資產。
#[derive(Debug, Clone, Copy, Default)]
pub struct Scope<'a> {
    pub cve_id: Option<&'a str>,
    /// 資產編號（不分大小寫）
    pub asset_tag: Option<&'a str>,
}

impl MatchData {
    /// 全部資料（`cvewatch match`）。
    pub fn load(conn: &Connection) -> Result<Self, CoreError> {
        Self::load_scoped(conn, Scope::default())
    }

    pub fn load_scoped(conn: &Connection, scope: Scope<'_>) -> Result<Self, CoreError> {
        let (osv, osv_corrupt) = osv(conn, scope.asset_tag)?;
        let (affected, corrupt_cves) = affected(conn, scope.cve_id)?;
        Ok(MatchData {
            components: active_components(conn, scope.asset_tag)?,
            cves: cves(conn, scope.cve_id)?,
            affected,
            corrupt_cves,
            nvd: nvd(conn, scope.cve_id)?,
            csaf: csaf(conn, scope.cve_id)?,
            csaf_fix: csaf_fix(conn, scope.cve_id)?,
            osv,
            osv_corrupt,
            last_osv_sync: crate::ingest::runlog::last_success(conn, crate::ingest::osv::SOURCE)?,
            kev: pairs(
                conn,
                "SELECT cve_id, date_added FROM kev WHERE ?1 IS NULL OR cve_id = ?1",
                scope.cve_id,
            )?,
            epss: pairs(
                conn,
                "SELECT cve_id, score FROM epss WHERE ?1 IS NULL OR cve_id = ?1",
                scope.cve_id,
            )?,
            metrics: metrics(conn, scope.cve_id.map(|id| vec![id.to_string()]).as_deref())?,
        })
    }

    /// 只保留符合條件的元件（其餘不參與本次比對，命中維持原狀）。
    pub fn retain_components(self, keep: impl Fn(&ActiveComponent) -> bool) -> Self {
        let components = self.components.into_iter().filter(|c| keep(c)).collect();
        MatchData { components, ..self }
    }

    /// 已撤銷（REJECTED）的 CVE。
    pub fn rejected_cves(&self) -> BTreeSet<String> {
        self.cves
            .iter()
            .filter(|c| c.state == CveState::Rejected)
            .map(|c| c.id.clone())
            .collect()
    }

    pub fn scores(&self, cve_id: &str) -> ScoreInputs {
        ScoreInputs {
            kev_date: self.kev.get(cve_id).cloned(),
            epss: self.epss.get(cve_id).copied(),
            cvss: self
                .metrics
                .get(cve_id)
                .and_then(|m| cvss::preferred(m))
                .map(|m| m.base_score),
        }
    }
}

fn pairs<T: rusqlite::types::FromSql>(
    conn: &Connection,
    sql: &str,
    cve_id: Option<&str>,
) -> Result<HashMap<String, T>, CoreError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![cve_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// 一列元件；kind 與 product/purl 不一致的列（資料庫約束下不應出現）記錄警告後略過。
fn component_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Option<ActiveComponent>> {
    let id: i64 = r.get(0)?;
    let kind: String = r.get(4)?;
    let target = match (
        kind.as_str(),
        r.get::<_, Option<String>>(5)?,
        r.get::<_, Option<String>>(6)?,
    ) {
        ("product", Some(k), _) => Target::Product(k),
        ("package", _, Some(p)) => Target::Package(p),
        _ => {
            tracing::warn!(component = id, kind = %kind, "元件的類型與產品／套件欄位不一致，本次不比對");
            return Ok(None);
        }
    };
    let exposure: String = r.get(2)?;
    let exposure = Exposure::parse(&exposure).unwrap_or_else(|| {
        tracing::warn!(component = id, exposure = %exposure, "資產的曝露度無法辨識，以預設值計算風險等級");
        Exposure::default()
    });
    Ok(Some(ActiveComponent {
        id,
        asset_tag: r.get(1)?,
        exposure,
        asset_value: r.get(3)?,
        target,
        version_raw: r.get(7)?,
        updated_at: r.get(8)?,
    }))
}

/// 參與比對的元件：排除版本待補的目錄產品元件（18 design D2）。沒被載入的元件，其命中與
/// 來源矛盾都不會被更新，因此待補期間保留原狀，填上版本後下一次比對沿用。
fn active_components(
    conn: &Connection,
    asset_tag: Option<&str>,
) -> Result<Vec<ActiveComponent>, CoreError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT c.id, c.asset_tag, c.exposure, c.asset_value, c.kind, p.key, c.purl,
                c.version_raw, c.updated_at
         FROM active_component c LEFT JOIN catalog_product p ON p.id = c.product_id
         WHERE (?1 IS NULL OR c.asset_tag = ?1 COLLATE NOCASE)
           AND NOT ({PENDING_COMPONENT_SQL})
         ORDER BY c.asset_tag, c.id"
    ))?;
    let rows = stmt.query_map(params![asset_tag], component_from_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.extend(row?);
    }
    Ok(out)
}

fn cves(conn: &Connection, cve_id: Option<&str>) -> Result<Vec<CveInfo>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, state, relevance_reason, description_en FROM cve
             WHERE ?1 IS NULL OR id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![cve_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get(2)?,
            r.get(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, state, relevance, description): (String, String, String, Option<String>) = row?;
        out.push(CveInfo {
            state: CveState::parse(&state).unwrap_or_else(|| {
                // 資料庫 CHECK 約束下不應出現；以已發布處理並記錄
                tracing::warn!(cve = %id, state = %state, "無法辨識的 CVE 狀態，以已發布處理");
                CveState::Published
            }),
            id,
            relevance: Relevance::parse(&relevance),
            description,
        });
    }
    Ok(out)
}

/// ID 清單的 JSON 陣列（供 `IN (SELECT value FROM json_each(?))` 綁定）。
pub(crate) fn ids_json(ids: &[i64]) -> Result<String, CoreError> {
    serde_json::to_string(ids).map_err(|e| CoreError::Invalid(format!("ID 清單序列化失敗：{e}")))
}

/// 資料庫中的 JSON 陣列；損壞時記錄警告並視為空清單（只用於不影響判定結論的欄位）。
pub(crate) fn json_list<T: serde::de::DeserializeOwned>(
    text: &str,
    field: &str,
    owner: &str,
) -> Vec<T> {
    try_json_list(text, field, owner).unwrap_or_default()
}

/// 資料庫中的 JSON 陣列；損壞時記錄警告並回傳 None，由呼叫端決定如何處理。
fn try_json_list<T: serde::de::DeserializeOwned>(
    text: &str,
    field: &str,
    owner: &str,
) -> Option<Vec<T>> {
    match serde_json::from_str(text) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(field, owner, error = %e, "資料庫中的 JSON 無法解析");
            None
        }
    }
}

/// 各 CVE 的 affected 條目，以及資料損壞（JSON 無法解析）的 CVE。
type AffectedRows = (HashMap<String, Vec<AffectedEntry>>, BTreeSet<String>);

fn affected(conn: &Connection, cve_id: Option<&str>) -> Result<AffectedRows, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT cve_id, source, provider, vendor_raw, product_raw, vendor_norm, product_norm,
                default_status, versions_json, cpes_json
         FROM cve_affected WHERE ?1 IS NULL OR cve_id = ?1 ORDER BY rowid",
    )?;
    let mut rows = stmt.query(params![cve_id])?;
    let mut out: HashMap<String, Vec<AffectedEntry>> = HashMap::new();
    let mut corrupt = BTreeSet::new();
    while let Some(r) = rows.next()? {
        let cve_id: String = r.get(0)?;
        let source: String = r.get(1)?;
        let versions: String = r.get(8)?;
        let cpes: String = r.get(9)?;
        let vendor_raw: Option<String> = r.get(3)?;
        let parsed = try_json_list::<Value>(&versions, "cve_affected.versions_json", &cve_id).zip(
            try_json_list::<String>(&cpes, "cve_affected.cpes_json", &cve_id),
        );
        let Some((versions, cpe_list)) = parsed else {
            corrupt.insert(cve_id);
            continue;
        };
        let entry = AffectedEntry {
            source: if source == "adp" {
                Source::Adp
            } else {
                Source::Cna
            },
            provider: r.get(2)?,
            vendor_raw_norm: vendor_raw.as_deref().map(crate::catalog::normalize),
            vendor_raw,
            product_raw: r.get(4)?,
            vendor_norm: r.get(5)?,
            product_norm: r.get(6)?,
            default_status: r.get(7)?,
            versions,
            cpe_pairs: cpe_list
                .iter()
                .filter_map(|c| cpe_vendor_product(c))
                .collect(),
            cpes: cpe_list,
        };
        out.entry(cve_id).or_default().push(entry);
    }
    Ok((out, corrupt))
}

fn nvd(
    conn: &Connection,
    cve_id: Option<&str>,
) -> Result<HashMap<String, Vec<NvdMatch>>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT cve_id, config_group, config_operator, node_negate, criteria_cpe, vuln,
                ver_start_incl, ver_start_excl, ver_end_incl, ver_end_excl
         FROM cve_nvd_match WHERE ?1 IS NULL OR cve_id = ?1 ORDER BY rowid",
    )?;
    let mut rows = stmt.query(params![cve_id])?;
    let mut out: HashMap<String, Vec<NvdMatch>> = HashMap::new();
    while let Some(r) = rows.next()? {
        let criteria: String = r.get(4)?;
        let fields = crate::cpe::fields(&criteria);
        let field = |i: usize| fields.get(i).cloned().unwrap_or_else(|| "*".into());
        out.entry(r.get(0)?).or_default().push(NvdMatch {
            config_group: r.get(1)?,
            config_operator: r.get(2)?,
            node_negate: r.get(3)?,
            vendor_product: cpe_vendor_product(&criteria),
            cpe_version: field(5),
            cpe_update: field(6),
            criteria,
            vulnerable: r.get(5)?,
            start_incl: r.get(6)?,
            start_excl: r.get(7)?,
            end_incl: r.get(8)?,
            end_excl: r.get(9)?,
        });
    }
    Ok(out)
}

/// cve_id → 產品 key → 列。
pub type CsafMap<T> = HashMap<String, HashMap<String, Vec<T>>>;

fn csaf(conn: &Connection, cve_id: Option<&str>) -> Result<CsafMap<CsafRow>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT s.cve_id, p.key, s.provider, s.tracking_id, d.updated, s.version_expr, s.kind,
                s.status, s.justification
         FROM csaf_status s
         JOIN catalog_product p ON p.id = s.catalog_product_id
         JOIN csaf_document d ON d.provider = s.provider AND d.tracking_id = s.tracking_id
         WHERE ?1 IS NULL OR s.cve_id = ?1",
    )?;
    let mut rows = stmt.query(params![cve_id])?;
    let mut out: CsafMap<CsafRow> = HashMap::new();
    while let Some(r) = rows.next()? {
        let (cve_id, key): (String, String) = (r.get(0)?, r.get(1)?);
        let version_expr: String = r.get(5)?;
        let kind: String = r.get(6)?;
        let range = (kind != "exact")
            .then(|| crate::ingest::csaf::vers::parse(&version_expr))
            .flatten();
        out.entry(cve_id)
            .or_default()
            .entry(key)
            .or_default()
            .push(CsafRow {
                provider: r.get(2)?,
                tracking_id: r.get(3)?,
                doc_updated: r.get(4)?,
                version_expr,
                kind,
                range,
                status: r.get(7)?,
                justification: r.get(8)?,
            });
    }
    Ok(out)
}

fn csaf_fix(conn: &Connection, cve_id: Option<&str>) -> Result<CsafMap<CsafFix>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT f.cve_id, p.key, f.provider, f.tracking_id, f.fixed_version
         FROM csaf_fix f JOIN catalog_product p ON p.id = f.catalog_product_id
         WHERE ?1 IS NULL OR f.cve_id = ?1",
    )?;
    let mut rows = stmt.query(params![cve_id])?;
    let mut out: CsafMap<CsafFix> = HashMap::new();
    while let Some(r) = rows.next()? {
        let (cve_id, key): (String, String) = (r.get(0)?, r.get(1)?);
        out.entry(cve_id)
            .or_default()
            .entry(key)
            .or_default()
            .push(CsafFix {
                provider: r.get(2)?,
                tracking_id: r.get(3)?,
                fixed_version: r.get(4)?,
            });
    }
    Ok(out)
}

/// 各元件的 OSV 命中，以及命中資料損壞的元件（下次 OSV 同步會整批重寫）。
type OsvHits = (HashMap<i64, Vec<OsvHit>>, BTreeSet<i64>);

fn osv(conn: &Connection, asset_tag: Option<&str>) -> Result<OsvHits, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT component_id, osv_id, cve_ids_json, fixed_versions_json, queried_at FROM osv_hit
         WHERE ?1 IS NULL OR component_id IN (
             SELECT c.id FROM component c JOIN asset a ON a.id = c.asset_id
             WHERE a.asset_tag = ?1 COLLATE NOCASE)",
    )?;
    let mut rows = stmt.query(params![asset_tag])?;
    let mut out: HashMap<i64, Vec<OsvHit>> = HashMap::new();
    let mut corrupt = BTreeSet::new();
    while let Some(r) = rows.next()? {
        let component_id: i64 = r.get(0)?;
        let osv_id: String = r.get(1)?;
        let cves: String = r.get(2)?;
        let fixed: String = r.get(3)?;
        let parsed = try_json_list(&cves, "osv_hit.cve_ids_json", &osv_id).zip(try_json_list(
            &fixed,
            "osv_hit.fixed_versions_json",
            &osv_id,
        ));
        let Some((cve_ids, fixed_versions)) = parsed else {
            corrupt.insert(component_id);
            continue;
        };
        out.entry(component_id).or_default().push(OsvHit {
            cve_ids,
            fixed_versions,
            osv_id,
            queried_at: r.get(4)?,
        });
    }
    Ok((out, corrupt))
}

/// 各 CVE 的 CVSS 列；`cve_ids` 為 None 時載入全部（一次查詢，不逐筆）。
pub(crate) fn metrics(
    conn: &Connection,
    cve_ids: Option<&[String]>,
) -> Result<HashMap<String, Vec<MetricRow>>, CoreError> {
    let ids_json = cve_ids
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| CoreError::Invalid(format!("CVE 清單序列化失敗：{e}")))?;
    let mut stmt = conn.prepare(
        "SELECT cve_id, source, provider, version, base_score, vector, severity FROM cve_metric
         WHERE ?1 IS NULL OR cve_id IN (SELECT value FROM json_each(?1))",
    )?;
    let mut rows = stmt.query(params![ids_json])?;
    let mut out: HashMap<String, Vec<MetricRow>> = HashMap::new();
    while let Some(r) = rows.next()? {
        let source_text: String = r.get(1)?;
        let Some(source) = MetricSource::parse(&source_text) else {
            let cve: String = r.get(0)?;
            tracing::warn!(cve = %cve, source = %source_text, "CVSS 來源無法辨識，此列不列入風險等級");
            continue;
        };
        out.entry(r.get(0)?).or_default().push(MetricRow {
            source,
            provider: r.get(2)?,
            version: r.get(3)?,
            base_score: r.get(4)?,
            vector: r.get(5)?,
            severity: r.get(6)?,
        });
    }
    Ok(out)
}
