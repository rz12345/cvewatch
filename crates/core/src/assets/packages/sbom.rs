//! CycloneDX / SPDX JSON：只取各元件的 purl。

use serde_json::Value;

use super::RawEntry;
use crate::error::CoreError;

fn invalid_json(kind: &str, e: serde_json::Error) -> CoreError {
    CoreError::Invalid(format!("{kind} JSON 格式錯誤：{e}"))
}

/// `components[].purl`，含巢狀 `components`。
pub fn cyclonedx(text: &str) -> Result<Vec<RawEntry>, CoreError> {
    let doc: Value = serde_json::from_str(text).map_err(|e| invalid_json("CycloneDX", e))?;
    let mut out = Vec::new();
    walk_cyclonedx(doc.get("components"), &mut out);
    Ok(out)
}

fn walk_cyclonedx(components: Option<&Value>, out: &mut Vec<RawEntry>) {
    let Some(list) = components.and_then(Value::as_array) else {
        return;
    };
    for c in list {
        out.push(match c.get("purl").and_then(Value::as_str) {
            Some(p) if !p.is_empty() => RawEntry::Purl(p.to_string()),
            _ => RawEntry::NoPurl,
        });
        walk_cyclonedx(c.get("components"), out);
    }
}

/// `packages[].externalRefs[]` 中 `referenceType = purl` 的 `referenceLocator`。
pub fn spdx(text: &str) -> Result<Vec<RawEntry>, CoreError> {
    let doc: Value = serde_json::from_str(text).map_err(|e| invalid_json("SPDX", e))?;
    let packages = doc
        .get("packages")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    Ok(packages.iter().map(spdx_purl).collect())
}

fn spdx_purl(package: &Value) -> RawEntry {
    package
        .get("externalRefs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|r| r.get("referenceType").and_then(Value::as_str) == Some("purl"))
        .and_then(|r| r.get("referenceLocator").and_then(Value::as_str))
        .map_or(RawEntry::NoPurl, |p| RawEntry::Purl(p.to_string()))
}
