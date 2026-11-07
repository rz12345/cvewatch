//! NVD API 2.0 回應解析。未知欄位一律忽略。

use serde::Deserialize;
use serde_json::Value;

use crate::error::CoreError;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    #[serde(default)]
    pub total_results: u64,
    #[serde(default)]
    pub vulnerabilities: Vec<Item>,
}

#[derive(Debug, Deserialize)]
pub struct Item {
    /// 原始 cve 物件，保存為 `raw_nvd_json`
    pub cve: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NvdCve {
    pub id: String,
    pub vuln_status: Option<String>,
    pub last_modified: Option<String>,
    #[serde(default)]
    pub descriptions: Vec<Description>,
    #[serde(default)]
    pub metrics: Metrics,
    #[serde(default)]
    pub configurations: Vec<Configuration>,
}

#[derive(Debug, Deserialize)]
pub struct Description {
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct Metrics {
    #[serde(rename = "cvssMetricV40", default)]
    pub v40: Vec<Value>,
    #[serde(rename = "cvssMetricV31", default)]
    pub v31: Vec<Value>,
    #[serde(rename = "cvssMetricV30", default)]
    pub v30: Vec<Value>,
    #[serde(rename = "cvssMetricV2", default)]
    pub v2: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMetric {
    source: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    cvss_data: RawCvssData,
    /// CVSS 2.0 的嚴重度在 cvssData 之外
    base_severity: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCvssData {
    base_score: f64,
    vector_string: Option<String>,
    base_severity: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NvdMetric {
    pub provider: Option<String>,
    pub version: &'static str,
    pub base_score: f64,
    pub vector: Option<String>,
    pub severity: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Configuration {
    pub operator: Option<String>,
    #[serde(default)]
    pub nodes: Vec<Node>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub operator: Option<String>,
    #[serde(default)]
    pub negate: bool,
    #[serde(default)]
    pub cpe_match: Vec<CpeMatch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CpeMatch {
    pub vulnerable: bool,
    pub criteria: String,
    pub version_start_including: Option<String>,
    pub version_start_excluding: Option<String>,
    pub version_end_including: Option<String>,
    pub version_end_excluding: Option<String>,
}

/// NVD 自己（`type = Primary`）給的分數類型。
const PRIMARY: &str = "Primary";

pub fn parse_page(bytes: &[u8]) -> Result<Page, CoreError> {
    serde_json::from_slice(bytes).map_err(|e| CoreError::Invalid(format!("NVD 回應格式錯誤：{e}")))
}

pub fn parse_cve(value: &Value) -> Result<NvdCve, CoreError> {
    serde_json::from_value(value.clone())
        .map_err(|e| CoreError::Invalid(format!("NVD CVE 物件格式錯誤：{e}")))
}

impl NvdCve {
    pub fn description_en(&self) -> Option<&str> {
        self.descriptions
            .iter()
            .find(|d| d.lang.eq_ignore_ascii_case("en"))
            .map(|d| d.value.trim())
            .filter(|v| !v.is_empty())
    }

    /// NVD 的 Primary 分數（CNA 透過 NVD 轉發的 Secondary 分數已由 cvelistV5 取得）。
    pub fn primary_metrics(&self) -> Vec<NvdMetric> {
        let groups: [(&[Value], &'static str); 4] = [
            (&self.metrics.v40, "4.0"),
            (&self.metrics.v31, "3.1"),
            (&self.metrics.v30, "3.0"),
            (&self.metrics.v2, "2.0"),
        ];
        groups
            .iter()
            .flat_map(|(items, version)| {
                items.iter().filter_map(move |v| {
                    let m: RawMetric = serde_json::from_value(v.clone()).ok()?;
                    if m.kind.as_deref() != Some(PRIMARY) {
                        return None;
                    }
                    Some(NvdMetric {
                        provider: m.source,
                        version,
                        base_score: m.cvss_data.base_score,
                        vector: m.cvss_data.vector_string,
                        severity: m
                            .cvss_data
                            .base_severity
                            .or(m.base_severity)
                            .map(|s| s.to_uppercase()),
                    })
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secondary_metrics_are_skipped() {
        let v = serde_json::json!({
            "id": "CVE-2099-1",
            "metrics": {"cvssMetricV31": [
                {"source": "psirt@x", "type": "Secondary", "cvssData": {"baseScore": 9.0}},
                {"source": "nvd@nist.gov", "type": "Primary",
                 "cvssData": {"baseScore": 7.5, "baseSeverity": "high", "vectorString": "CVSS:3.1/AV:N"}}
            ],
            "cvssMetricV2": [{"source": "nvd@nist.gov", "type": "Primary", "baseSeverity": "MEDIUM",
                              "cvssData": {"baseScore": 5.0}}]}
        });
        let cve = parse_cve(&v).unwrap();
        let m = cve.primary_metrics();
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].version, m[0].base_score), ("3.1", 7.5));
        assert_eq!(m[0].severity.as_deref(), Some("HIGH"));
        assert_eq!(m[1].severity.as_deref(), Some("MEDIUM"));
    }
}
