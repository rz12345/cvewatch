//! 匯出用的 CycloneDX 1.6 JSON 結構（只含 VEX 需要的欄位；欄位名稱依規格的 camelCase）。

use serde::Serialize;

pub const BOM_FORMAT: &str = "CycloneDX";
pub const SPEC_VERSION: &str = "1.6";
pub const TOOL_NAME: &str = "cvewatch";
/// 元件上的資產編號 property。
pub const PROP_ASSET_TAG: &str = "cvewatch:asset_tag";
pub const PROP_PRIORITY: &str = "cvewatch:priority";
pub const PROP_CONFIDENCE: &str = "cvewatch:confidence";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub bom_format: &'static str,
    pub spec_version: &'static str,
    pub serial_number: String,
    pub version: u32,
    pub metadata: Metadata,
    pub components: Vec<Component>,
    pub vulnerabilities: Vec<Vulnerability>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Metadata {
    pub timestamp: String,
    pub tools: Tools,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tools {
    pub components: Vec<Tool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub name: &'static str,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Named {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Property {
    pub name: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Component {
    #[serde(rename = "bom-ref")]
    pub bom_ref: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier: Option<Named>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purl: Option<String>,
    pub properties: Vec<Property>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Source {
    pub name: &'static str,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rating {
    pub source: Named,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    pub method: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Affect {
    #[serde(rename = "ref")]
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justification: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub response: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub first_issued: String,
    pub last_updated: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Vulnerability {
    pub id: String,
    pub source: Source,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ratings: Vec<Rating>,
    pub affects: Vec<Affect>,
    pub analysis: Analysis,
    pub properties: Vec<Property>,
}

/// CVE.org 紀錄頁。
pub fn cve_url(id: &str) -> String {
    format!("https://www.cve.org/CVERecord?id={id}")
}

/// CVSS 版本 → CycloneDX `ratings[].method`；未知版本為 `other`。
pub fn rating_method(version: &str) -> &'static str {
    match version {
        "2.0" => "CVSSv2",
        "3.0" => "CVSSv3",
        "3.1" => "CVSSv31",
        "4.0" => "CVSSv4",
        _ => "other",
    }
}

/// CVSS 嚴重度 → CycloneDX `severity` 允許值；無法對應時省略。
pub fn severity(text: Option<&str>) -> Option<String> {
    const ALLOWED: [&str; 7] = [
        "critical", "high", "medium", "low", "info", "none", "unknown",
    ];
    let lower = text?.trim().to_ascii_lowercase();
    ALLOWED.contains(&lower.as_str()).then_some(lower)
}
