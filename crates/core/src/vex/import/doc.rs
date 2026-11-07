//! 匯入用的 CycloneDX 1.4–1.6 JSON 結構：只讀取 VEX 需要的欄位，容忍未知欄位與其他工具的擴充。

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::CoreError;

/// 支援的 `specVersion`。
pub const SUPPORTED_SPEC_VERSIONS: [&str; 3] = ["1.4", "1.5", "1.6"];
/// 讀取的 VEX 檔上限（避免誤選巨大檔案耗盡記憶體）。
pub const MAX_VEX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bom {
    pub bom_format: Option<String>,
    pub spec_version: Option<String>,
    pub metadata: Option<Metadata>,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default)]
    pub vulnerabilities: Vec<Vulnerability>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Metadata {
    pub timestamp: Option<String>,
    pub component: Option<Component>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Component {
    #[serde(rename = "bom-ref")]
    pub bom_ref: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub purl: Option<String>,
    pub cpe: Option<String>,
    /// 巢狀元件（組件內含的子元件）
    #[serde(default)]
    pub components: Vec<Component>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Vulnerability {
    pub id: Option<String>,
    /// 其他識別碼（例如 GHSA 項目的 CVE 別名）
    #[serde(default)]
    pub references: Vec<Reference>,
    #[serde(default)]
    pub affects: Vec<Affect>,
    pub analysis: Option<Analysis>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Reference {
    pub id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Affect {
    #[serde(rename = "ref")]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub state: Option<String>,
    pub justification: Option<String>,
    #[serde(default)]
    pub response: Vec<String>,
    pub detail: Option<String>,
    pub last_updated: Option<String>,
}

fn format_error(message: impl std::fmt::Display) -> CoreError {
    CoreError::Invalid(format!("不是可讀取的 CycloneDX VEX：{message}"))
}

/// 解析並檢查 `bomFormat` 與 `specVersion`。
pub fn parse(bytes: &[u8]) -> Result<Bom, CoreError> {
    if bytes.len() > MAX_VEX_BYTES {
        return Err(format_error(format!(
            "檔案超過 {} MB",
            MAX_VEX_BYTES / 1024 / 1024
        )));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| format_error("不是 UTF-8 文字"))?;
    let text = text.trim_start_matches('\u{feff}');
    let de = &mut serde_json::Deserializer::from_str(text);
    let bom: Bom = serde_path_to_error::deserialize(de)
        .map_err(|e| format_error(format!("{}（位置 {}）", e.inner(), e.path())))?;
    if bom.bom_format.as_deref() != Some("CycloneDX") {
        return Err(format_error("bomFormat 不是 CycloneDX"));
    }
    let version = bom.spec_version.as_deref().unwrap_or("");
    if !SUPPORTED_SPEC_VERSIONS.contains(&version) {
        return Err(format_error(format!(
            "specVersion「{version}」不支援；支援 {}",
            SUPPORTED_SPEC_VERSIONS.join("、")
        )));
    }
    Ok(bom)
}

impl Bom {
    /// 依 bom-ref 索引所有元件（含巢狀與 metadata.component）。
    pub fn components_by_ref(&self) -> HashMap<String, Component> {
        let mut out = HashMap::new();
        let roots = self
            .metadata
            .iter()
            .filter_map(|m| m.component.as_ref())
            .chain(self.components.iter());
        let mut stack: Vec<&Component> = roots.collect();
        while let Some(c) = stack.pop() {
            if let Some(r) = c.bom_ref.as_deref().filter(|r| !r.trim().is_empty()) {
                out.entry(r.to_string()).or_insert_with(|| Component {
                    components: Vec::new(),
                    ..c.clone()
                });
            }
            stack.extend(c.components.iter());
        }
        out
    }

    pub fn timestamp(&self) -> Option<&str> {
        self.metadata.as_ref()?.timestamp.as_deref()
    }
}

/// `CVE-YYYY-NNNN…`（不分大小寫）→ 大寫；其他識別碼回傳 `None`。
pub fn cve_id(text: &str) -> Option<String> {
    let upper = text.trim().to_ascii_uppercase();
    let rest = upper.strip_prefix("CVE-")?;
    let (year, number) = rest.split_once('-')?;
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    (year.len() == 4 && number.len() >= 4 && digits(year) && digits(number)).then_some(upper)
}

impl Vulnerability {
    /// `id` 為 CVE 時取之，否則取 `references` 中第一個 CVE 別名。
    pub fn cve(&self) -> Option<String> {
        self.id.as_deref().and_then(cve_id).or_else(|| {
            self.references
                .iter()
                .find_map(|r| cve_id(r.id.as_deref()?))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bom(extra: &str) -> String {
        format!(r#"{{"bomFormat":"CycloneDX","specVersion":"1.5"{extra}}}"#)
    }

    #[test]
    fn accepts_supported_versions_and_unknown_fields() {
        for v in SUPPORTED_SPEC_VERSIONS {
            let text = format!(
                r#"{{"bomFormat":"CycloneDX","specVersion":"{v}","x-vendor":{{"a":1}},
                    "vulnerabilities":[{{"id":"CVE-2024-0001","cwes":[79],"analysis":{{"state":"in_triage","extra":true}}}}]}}"#
            );
            let parsed = parse(text.as_bytes()).unwrap();
            assert_eq!(parsed.vulnerabilities.len(), 1, "{v}");
        }
    }

    #[test]
    fn rejects_other_formats_and_versions() {
        let spdx = r#"{"spdxVersion":"SPDX-2.3"}"#;
        assert!(parse(spdx.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("bomFormat"));
        let old = r#"{"bomFormat":"CycloneDX","specVersion":"1.3"}"#;
        assert!(parse(old.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("1.3"));
        assert!(parse(b"not json").is_err());
        assert!(parse(&[0xff, 0xfe])
            .unwrap_err()
            .to_string()
            .contains("UTF-8"));
    }

    #[test]
    fn type_errors_report_location() {
        let text = bom(r#","vulnerabilities":[{"affects":"oops"}]"#);
        let err = parse(text.as_bytes()).unwrap_err().to_string();
        assert!(err.contains("vulnerabilities[0].affects"), "{err}");
    }

    #[test]
    fn bom_is_accepted_with_utf8_bom_marker() {
        let text = format!("\u{feff}{}", bom(""));
        assert!(parse(text.as_bytes()).is_ok());
    }

    #[test]
    fn nested_and_metadata_components_are_indexed() {
        let text = bom(
            r#","metadata":{"component":{"bom-ref":"root","name":"app"}},
               "components":[{"bom-ref":"a","name":"A","components":[{"bom-ref":"b","name":"B"}]}]"#,
        );
        let map = parse(text.as_bytes()).unwrap().components_by_ref();
        let mut keys: Vec<_> = map.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["a", "b", "root"]);
        assert!(map["a"].components.is_empty());
    }

    #[test]
    fn cve_from_id_or_reference_alias() {
        assert_eq!(
            cve_id(" cve-2024-12345 ").as_deref(),
            Some("CVE-2024-12345")
        );
        assert_eq!(cve_id("GHSA-xxxx-yyyy-zzzz"), None);
        assert_eq!(cve_id("CVE-24-1"), None);
        let v = Vulnerability {
            id: Some("GHSA-abcd".into()),
            references: vec![
                Reference {
                    id: Some("OSV-1".into()),
                },
                Reference {
                    id: Some("CVE-2023-5678".into()),
                },
            ],
            ..Vulnerability::default()
        };
        assert_eq!(v.cve().as_deref(), Some("CVE-2023-5678"));
    }
}
