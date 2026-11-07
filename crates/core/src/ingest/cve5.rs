//! CVE Record Format 5.x 解析（5.0 / 5.1 / 5.2）。
//!
//! 只解析需要的欄位，未知欄位一律忽略。`affected[]` 與 `metrics[]` 逐項解析，
//! 單一項目格式不符時跳過該項而不讓整筆紀錄失敗。

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::error::CoreError;

string_enum!(
    /// CVE 狀態。
    CveState {
        Published => "PUBLISHED",
        Rejected => "REJECTED",
    }
);

/// 解析後的紀錄。
#[derive(Debug, Clone, PartialEq)]
pub struct CveRecord {
    pub id: String,
    pub state: CveState,
    pub data_version: Option<String>,
    pub assigner_short: Option<String>,
    pub date_published: Option<String>,
    pub date_updated: Option<String>,
    pub title: Option<String>,
    pub description_en: Option<String>,
    pub cna: Container,
    pub adp: Vec<Container>,
}

/// CNA 或 ADP container。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Container {
    pub provider: Option<String>,
    pub affected: Vec<Affected>,
    pub metrics: Vec<CvssMetric>,
    pub ssvc: Option<Ssvc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Affected {
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub default_status: Option<String>,
    /// 原始 `versions[]` 陣列（JSON），判定在 04
    pub versions: Vec<Value>,
    pub cpes: Vec<String>,
    /// 5.2 `packageURL`
    pub package_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CvssMetric {
    /// `2.0`、`3.0`、`3.1`、`4.0`
    pub version: &'static str,
    pub base_score: f64,
    pub vector: Option<String>,
    pub severity: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ssvc {
    pub exploitation: Option<String>,
    pub automatable: Option<String>,
    pub technical_impact: Option<String>,
}

// ── serde 原始結構 ───────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRecord {
    data_version: Option<String>,
    cve_metadata: RawMetadata,
    #[serde(default)]
    containers: RawContainers,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMetadata {
    cve_id: String,
    state: String,
    assigner_short_name: Option<String>,
    date_published: Option<String>,
    date_updated: Option<String>,
    date_rejected: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawContainers {
    cna: Option<RawContainer>,
    #[serde(default, deserialize_with = "null_as_default")]
    adp: Vec<RawContainer>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawContainer {
    provider_metadata: Option<RawProvider>,
    title: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    descriptions: Vec<RawDescription>,
    #[serde(default, deserialize_with = "null_as_default")]
    rejected_reasons: Vec<RawDescription>,
    #[serde(default, deserialize_with = "null_as_default")]
    affected: Vec<Value>,
    #[serde(default, deserialize_with = "null_as_default")]
    metrics: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawProvider {
    short_name: Option<String>,
}

#[derive(Deserialize)]
struct RawDescription {
    #[serde(default)]
    lang: String,
    #[serde(default)]
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAffected {
    vendor: Option<String>,
    product: Option<String>,
    default_status: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    versions: Vec<Value>,
    #[serde(default, deserialize_with = "null_as_default")]
    cpes: Vec<String>,
    #[serde(rename = "packageURL")]
    package_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCvss {
    base_score: f64,
    vector_string: Option<String>,
    base_severity: Option<String>,
}

#[derive(Deserialize)]
struct RawOther {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    content: Value,
}

fn null_as_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// `metrics[]` 中各 CVSS 版本的鍵名。
const CVSS_KEYS: [(&str, &str); 4] = [
    ("cvssV4_0", "4.0"),
    ("cvssV3_1", "3.1"),
    ("cvssV3_0", "3.0"),
    ("cvssV2_0", "2.0"),
];

const SSVC_TYPE: &str = "ssvc";

// ── 解析 ────────────────────────────────────────────────────

pub fn parse(bytes: &[u8]) -> Result<CveRecord, CoreError> {
    let raw: RawRecord = serde_json::from_slice(bytes)
        .map_err(|e| CoreError::Invalid(format!("CVE 紀錄格式錯誤：{e}")))?;
    let meta = raw.cve_metadata;
    let state = CveState::parse(&meta.state).ok_or_else(|| {
        CoreError::Invalid(format!("{}：未知的 state `{}`", meta.cve_id, meta.state))
    })?;
    let cna_raw = raw.containers.cna.unwrap_or_default();
    let description_en =
        english(&cna_raw.descriptions).or_else(|| english(&cna_raw.rejected_reasons));
    let title = cna_raw.title.clone().filter(|t| !t.trim().is_empty());
    Ok(CveRecord {
        id: meta.cve_id,
        state,
        data_version: raw.data_version,
        assigner_short: meta.assigner_short_name,
        date_published: meta.date_published,
        date_updated: meta.date_updated.or(meta.date_rejected),
        title,
        description_en,
        cna: container(cna_raw),
        adp: raw.containers.adp.into_iter().map(container).collect(),
    })
}

/// 第一個 `en` 或 `en-*` 描述。
fn english(descriptions: &[RawDescription]) -> Option<String> {
    descriptions
        .iter()
        .find(|d| {
            d.lang.eq_ignore_ascii_case("en") || d.lang.to_ascii_lowercase().starts_with("en-")
        })
        .map(|d| d.value.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn container(raw: RawContainer) -> Container {
    let affected = lenient::<RawAffected>(raw.affected)
        .map(|a| Affected {
            vendor: a.vendor,
            product: a.product,
            default_status: a.default_status,
            versions: a.versions,
            cpes: a.cpes,
            package_url: a.package_url.filter(|p| !p.trim().is_empty()),
        })
        .collect();
    let mut metrics = Vec::new();
    let mut ssvc = None;
    for metric in &raw.metrics {
        metrics.extend(cvss_entries(metric));
        if ssvc.is_none() {
            ssvc = ssvc_entry(metric);
        }
    }
    Container {
        provider: raw.provider_metadata.and_then(|p| p.short_name),
        affected,
        metrics,
        ssvc,
    }
}

/// 逐項轉換，格式不符的項目略過。
fn lenient<T: DeserializeOwned>(items: Vec<Value>) -> impl Iterator<Item = T> {
    items
        .into_iter()
        .filter_map(|v| match serde_json::from_value(v) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::debug!(error = %e, "略過格式不符的項目");
                None
            }
        })
}

fn cvss_entries(metric: &Value) -> Vec<CvssMetric> {
    CVSS_KEYS
        .iter()
        .filter_map(|(key, version)| {
            let raw: RawCvss = serde_json::from_value(metric.get(*key)?.clone()).ok()?;
            Some(CvssMetric {
                version,
                base_score: raw.base_score,
                vector: raw.vector_string,
                severity: raw.base_severity.map(|s| s.to_uppercase()),
            })
        })
        .collect()
}

/// `other.type = "ssvc"`，`content.options` 為 `[{"Exploitation": "none"}, ...]`。
fn ssvc_entry(metric: &Value) -> Option<Ssvc> {
    let other: RawOther = serde_json::from_value(metric.get("other")?.clone()).ok()?;
    if !other.kind.eq_ignore_ascii_case(SSVC_TYPE) {
        return None;
    }
    let options = other.content.get("options")?.as_array()?;
    let pick = |name: &str| {
        options.iter().find_map(|o| {
            o.as_object()?
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .and_then(|(_, v)| v.as_str())
                .map(str::to_string)
        })
    };
    Some(Ssvc {
        exploitation: pick("Exploitation"),
        automatable: pick("Automatable"),
        technical_impact: pick("Technical Impact"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_record_parses() {
        let json =
            br#"{"dataVersion":"5.0","cveMetadata":{"cveId":"CVE-2099-1","state":"PUBLISHED"},
                       "containers":{"cna":{"descriptions":[{"lang":"en-US","value":" x "}]}}}"#;
        let r = parse(json).unwrap();
        assert_eq!(r.id, "CVE-2099-1");
        assert_eq!(r.description_en.as_deref(), Some("x"));
        assert!(r.adp.is_empty());
    }

    #[test]
    fn bad_affected_item_is_skipped() {
        let json = br#"{"cveMetadata":{"cveId":"CVE-2099-2","state":"PUBLISHED"},
            "containers":{"cna":{"affected":[{"vendor":1},{"vendor":"V","product":"P","cpes":null}],
            "metrics":[{"cvssV3_1":{"baseScore":"high"}},{"cvssV3_1":{"baseScore":5.0,"baseSeverity":"medium"}}]}}}"#;
        let r = parse(json).unwrap();
        assert_eq!(r.cna.affected.len(), 1);
        assert_eq!(r.cna.affected[0].vendor.as_deref(), Some("V"));
        assert_eq!(r.cna.metrics.len(), 1);
        assert_eq!(r.cna.metrics[0].severity.as_deref(), Some("MEDIUM"));
    }

    #[test]
    fn unknown_state_is_an_error() {
        let json = br#"{"cveMetadata":{"cveId":"CVE-2099-3","state":"RESERVED"},"containers":{}}"#;
        assert!(parse(json).is_err());
    }

    #[test]
    fn not_json_is_an_error() {
        assert!(parse(b"<html>").is_err());
    }
}
