//! 命中 → CycloneDX 1.6 VEX 文件（`cvewatch vex export` 與 TUI `E` 共用）。
//!
//! 每筆命中輸出一個 vulnerability（同一 CVE 在不同元件可能有不同判斷），並引用其元件；
//! 元件只輸出被引用者，依命中順序排列。

use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::{DateTime, Utc};
use rand::rngs::SysRng;
use rand::TryRng;
use rusqlite::{params, Connection, OptionalExtension};

use super::bomref::ComponentRef;
use super::detail::{self, EventFacts};
use super::document::{
    self as doc, Affect, Analysis, Component, Document, Metadata, Named, Property, Rating, Source,
    Tool, Tools, Vulnerability,
};
use super::mapping::{to_analysis, Verdict};
use crate::error::CoreError;
use crate::ingest::cvss::MetricRow;
use crate::matching::load;
use crate::matching::triage::{IgnoreReason, Justification, Status};

/// 文件層級的欄位（測試注入固定值以產生 golden file）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportMeta {
    pub serial_number: String,
    pub timestamp: DateTime<Utc>,
    pub tool_version: String,
}

impl ExportMeta {
    /// 新的 `urn:uuid:` 序號、目前時間與 cvewatch 版本。
    pub fn new(now: DateTime<Utc>) -> Result<Self, CoreError> {
        Ok(ExportMeta {
            serial_number: new_serial()?,
            timestamp: now,
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }
}

/// UUID v4 形式的序號。
pub fn new_serial() -> Result<String, CoreError> {
    let mut b = [0u8; 16];
    SysRng
        .try_fill_bytes(&mut b)
        .map_err(|e| CoreError::Invalid(format!("無法取得系統亂數：{e}")))?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

/// 一筆命中的匯出資料。
struct Row {
    cve_id: String,
    priority: String,
    confidence: String,
    verdict: Verdict,
    first_seen: String,
    component: ComponentFacts,
}

struct ComponentFacts {
    asset_tag: String,
    product: Option<ProductFacts>,
    purl: Option<String>,
    purl_full: Option<String>,
    version: String,
}

struct ProductFacts {
    key: String,
    display_name: String,
    vendor: String,
    cpe: Option<(String, String, String)>,
}

/// 組成 VEX 文件；任一命中不存在時回報錯誤。
pub fn build(conn: &Connection, ids: &[i64], meta: &ExportMeta) -> Result<Document, CoreError> {
    let mut cpes = HashMap::new();
    let rows = ids
        .iter()
        .map(|id| load_row(conn, *id, &mut cpes))
        .collect::<Result<Vec<_>, _>>()?;
    // 同一 CVE 的分數只查一次
    let cve_ids: Vec<String> = rows
        .iter()
        .map(|r| r.cve_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let metrics = load::metrics(conn, Some(&cve_ids))?;
    let mut components = Vec::new();
    let mut seen = HashSet::new();
    let mut vulnerabilities = Vec::with_capacity(ids.len());
    for (id, row) in ids.iter().zip(&rows) {
        let reference = component_ref(&row.component).to_bom_ref();
        if seen.insert(reference.clone()) {
            components.push(component(&row.component, reference.clone()));
        }
        let events = load_events(conn, *id)?;
        let ratings = ratings(metrics.get(&row.cve_id).map(Vec::as_slice).unwrap_or(&[]));
        vulnerabilities.push(vulnerability(row, reference, ratings, &events));
    }
    Ok(Document {
        bom_format: doc::BOM_FORMAT,
        spec_version: doc::SPEC_VERSION,
        serial_number: meta.serial_number.clone(),
        version: 1,
        metadata: Metadata {
            timestamp: detail::format_time(meta.timestamp),
            tools: Tools {
                components: vec![Tool {
                    kind: "application",
                    name: doc::TOOL_NAME,
                    version: meta.tool_version.clone(),
                }],
            },
        },
        components,
        vulnerabilities,
    })
}

/// 縮排的 JSON 文字（結尾換行）。
pub fn to_json(document: &Document) -> Result<String, CoreError> {
    serde_json::to_string_pretty(document)
        .map(|s| s + "\n")
        .map_err(|e| CoreError::Invalid(format!("無法產生 VEX JSON：{e}")))
}

fn invalid(id: i64, what: &str, value: &str) -> CoreError {
    CoreError::Invalid(format!("命中 {id} 的{what}「{value}」無法辨識"))
}

/// 產品 id → 第一個 CPE（同一產品只查一次）。
type CpeCache = HashMap<i64, Option<(String, String, String)>>;

const ROW_SQL: &str =
    "SELECT f.cve_id, f.priority, f.confidence, f.status, f.ignore_reason, f.justification,
            f.first_seen, a.asset_tag, c.product_id, p.key, p.display_name, p.vendor_display,
            c.purl, c.purl_full, c.version_raw
     FROM finding f
     JOIN component c ON c.id = f.component_id
     JOIN asset a ON a.id = c.asset_id
     LEFT JOIN catalog_product p ON p.id = c.product_id
     WHERE f.id = ?1";

/// 資料庫欄位（尚未驗證狀態值、尚未查 CPE）。
struct RawRow {
    cve_id: String,
    priority: String,
    confidence: String,
    status: String,
    reason: Option<String>,
    justification: Option<String>,
    first_seen: String,
    asset_tag: String,
    /// (產品 id, key, 顯示名稱, 廠商)
    product: Option<(i64, String, String, String)>,
    purl: Option<String>,
    purl_full: Option<String>,
    version: String,
}

fn read_raw(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    let product = match r.get::<_, Option<i64>>(8)? {
        Some(pid) => Some((pid, r.get(9)?, r.get(10)?, r.get(11)?)),
        None => None,
    };
    Ok(RawRow {
        cve_id: r.get(0)?,
        priority: r.get(1)?,
        confidence: r.get(2)?,
        status: r.get(3)?,
        reason: r.get(4)?,
        justification: r.get(5)?,
        first_seen: r.get(6)?,
        asset_tag: r.get(7)?,
        product,
        purl: r.get(12)?,
        purl_full: r.get(13)?,
        version: r.get(14)?,
    })
}

fn cached_cpe(
    conn: &Connection,
    cpes: &mut CpeCache,
    product_id: i64,
) -> Result<Option<(String, String, String)>, CoreError> {
    if let Some(cached) = cpes.get(&product_id) {
        return Ok(cached.clone());
    }
    let found = first_cpe(conn, product_id)?;
    cpes.insert(product_id, found.clone());
    Ok(found)
}

fn load_row(conn: &Connection, id: i64, cpes: &mut CpeCache) -> Result<Row, CoreError> {
    let raw = conn
        .prepare_cached(ROW_SQL)?
        .query_row(params![id], read_raw)
        .optional()?
        .ok_or_else(|| CoreError::Invalid(format!("找不到命中 {id}")))?;
    let product = match raw.product {
        Some((pid, key, display_name, vendor)) => Some(ProductFacts {
            key,
            display_name,
            vendor,
            cpe: cached_cpe(conn, cpes, pid)?,
        }),
        None => None,
    };
    Ok(Row {
        verdict: verdict(
            id,
            &raw.status,
            raw.reason.as_deref(),
            raw.justification.as_deref(),
        )?,
        cve_id: raw.cve_id,
        priority: raw.priority,
        confidence: raw.confidence,
        first_seen: raw.first_seen,
        component: ComponentFacts {
            asset_tag: raw.asset_tag,
            product,
            purl: raw.purl,
            purl_full: raw.purl_full,
            version: raw.version,
        },
    })
}

/// 資料庫欄位 → 判定；無法辨識的值回報錯誤（匯出與匯入共用）。
pub(crate) fn verdict(
    id: i64,
    status: &str,
    reason: Option<&str>,
    justification: Option<&str>,
) -> Result<Verdict, CoreError> {
    let status = Status::parse(status).ok_or_else(|| invalid(id, "狀態", status))?;
    let reason = reason
        .map(|r| IgnoreReason::parse(r).ok_or_else(|| invalid(id, "忽略原因", r)))
        .transpose()?;
    let justification = justification
        .map(|j| Justification::parse(j).ok_or_else(|| invalid(id, "理由", j)))
        .transpose()?;
    Ok(Verdict {
        status,
        reason,
        justification,
    })
}

/// 命中目前的判定。
pub(crate) fn load_verdict(conn: &Connection, id: i64) -> Result<Verdict, CoreError> {
    let (status, reason, justification): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT status, ignore_reason, justification FROM finding WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid(format!("找不到命中 {id}")))?;
    verdict(id, &status, reason.as_deref(), justification.as_deref())
}

fn first_cpe(
    conn: &Connection,
    product_id: i64,
) -> Result<Option<(String, String, String)>, CoreError> {
    Ok(conn
        .query_row(
            "SELECT cpe_part, cpe_vendor, cpe_product FROM catalog_cpe
             WHERE product_id = ?1 ORDER BY rowid LIMIT 1",
            params![product_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?)
}

pub(crate) fn load_events(conn: &Connection, id: i64) -> Result<Vec<EventFacts>, CoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT at, actor, from_status, to_status, note, import_file, decided_at
         FROM finding_event WHERE finding_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![id], |r| {
        Ok(EventFacts {
            at: r.get(0)?,
            actor: r.get(1)?,
            from: r.get(2)?,
            to: r.get(3)?,
            note: r.get(4)?,
            import_file: r.get(5)?,
            decided_at: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn component_ref(c: &ComponentFacts) -> ComponentRef {
    match (&c.product, &c.purl) {
        (Some(p), _) => ComponentRef::Product {
            asset_tag: c.asset_tag.clone(),
            key: p.key.clone(),
        },
        (None, purl) => ComponentRef::Package {
            asset_tag: c.asset_tag.clone(),
            purl: purl.clone().unwrap_or_default(),
        },
    }
}

/// CPE 2.3 formatted string 的值：小寫，英數與 `._-` 以外的字元加跳脫；空白值為 `*`。
fn cpe_value(text: &str) -> String {
    let lower = text.trim().to_lowercase();
    if lower.is_empty() {
        return "*".into();
    }
    lower
        .chars()
        .flat_map(|c| {
            let plain = c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
            (!plain).then_some('\\').into_iter().chain([c])
        })
        .collect()
}

fn cpe_kind(part: &str) -> &'static str {
    match part {
        "o" => "operating-system",
        "h" => "device",
        _ => "application",
    }
}

fn package_name(purl: Option<&str>) -> String {
    purl.and_then(|p| p.parse::<packageurl::PackageUrl<'_>>().ok())
        .map(|p| p.name().to_string())
        .unwrap_or_default()
}

fn component(c: &ComponentFacts, bom_ref: String) -> Component {
    let properties = vec![Property {
        name: doc::PROP_ASSET_TAG,
        value: c.asset_tag.clone(),
    }];
    match &c.product {
        Some(p) => Component {
            bom_ref,
            kind: p
                .cpe
                .as_ref()
                .map_or("application", |(part, _, _)| cpe_kind(part)),
            name: p.display_name.clone(),
            version: c.version.clone(),
            supplier: Some(Named {
                name: p.vendor.clone(),
            }),
            cpe: p.cpe.as_ref().map(|(part, vendor, product)| {
                format!(
                    "cpe:2.3:{part}:{vendor}:{product}:{}:*:*:*:*:*:*:*",
                    cpe_value(&c.version)
                )
            }),
            purl: None,
            properties,
        },
        None => Component {
            bom_ref,
            kind: "library",
            name: package_name(c.purl.as_deref()),
            version: c.version.clone(),
            supplier: None,
            cpe: None,
            purl: c.purl_full.clone().or_else(|| c.purl.clone()),
            properties,
        },
    }
}

fn ratings(metrics: &[MetricRow]) -> Vec<Rating> {
    metrics
        .iter()
        .map(|m| Rating {
            source: Named {
                name: m.source.label().to_string(),
            },
            score: m.base_score,
            severity: doc::severity(m.severity.as_deref()),
            method: doc::rating_method(&m.version),
            vector: m.vector.clone(),
        })
        .collect()
}

fn vulnerability(
    row: &Row,
    reference: String,
    ratings: Vec<Rating>,
    events: &[EventFacts],
) -> Vulnerability {
    let a = to_analysis(&row.verdict);
    Vulnerability {
        id: row.cve_id.clone(),
        source: Source {
            name: "CVE",
            url: doc::cve_url(&row.cve_id),
        },
        ratings,
        affects: vec![Affect { reference }],
        analysis: Analysis {
            state: a.state.as_str(),
            justification: a.justification.map(Justification::as_str),
            response: a.response.iter().map(|r| r.as_str()).collect(),
            detail: detail::current_detail(events),
            first_issued: detail::normalize_time(&row.first_seen),
            last_updated: detail::last_updated(events, &row.first_seen),
        },
        properties: vec![
            Property {
                name: doc::PROP_PRIORITY,
                value: row.priority.clone(),
            },
            Property {
                name: doc::PROP_CONFIDENCE,
                value: row.confidence.clone(),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_is_uuid_v4() {
        let s = new_serial().unwrap();
        let re = regex::Regex::new(
            "^urn:uuid:[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
        )
        .unwrap();
        assert!(re.is_match(&s), "{s}");
        assert_ne!(s, new_serial().unwrap());
    }

    #[test]
    fn cpe_values_are_escaped() {
        assert_eq!(cpe_value("11.1.4-h7"), "11.1.4-h7");
        assert_eq!(cpe_value("7.2.1 Update 3"), r"7.2.1\ update\ 3");
        assert_eq!(cpe_value("8.0U2"), "8.0u2");
        assert_eq!(cpe_value(""), "*");
        assert_eq!(cpe_value("1:2"), r"1\:2");
    }

    #[test]
    fn cpe_part_maps_to_component_type() {
        assert_eq!(cpe_kind("o"), "operating-system");
        assert_eq!(cpe_kind("h"), "device");
        assert_eq!(cpe_kind("a"), "application");
    }

    #[test]
    fn package_name_comes_from_purl() {
        assert_eq!(
            package_name(Some("pkg:deb/debian/openssl?distro=debian-12")),
            "openssl"
        );
        assert_eq!(package_name(Some("not a purl")), "");
    }
}
