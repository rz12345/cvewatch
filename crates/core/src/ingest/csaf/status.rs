//! CSAF `vulnerabilities[]` → `csaf_status` / `csaf_fix` 列。
//!
//! - known_affected、first_affected、last_affected → affected
//! - fixed、first_fixed → fixed（該版本不受影響）
//! - known_not_affected → not_affected（理由取自 `flags`）
//! - under_investigation → under_investigation（04 視為未知）
//! - `remediations[]` 中 `vendor_fix` → 修正版本（同一目錄產品在 fixed 狀態下的確切版本）

use std::collections::BTreeSet;

use serde_json::Value;

use super::tree::{ProductMap, ProductRef, VersionKind};
use crate::ingest::show::normalize_id;

pub const AFFECTED: &str = "affected";
pub const NOT_AFFECTED: &str = "not_affected";
pub const FIXED: &str = "fixed";
pub const UNDER_INVESTIGATION: &str = "under_investigation";

/// CSAF product_status 分類 → 本機狀態。
const STATUS_MAP: [(&str, &str); 7] = [
    ("known_affected", AFFECTED),
    ("first_affected", AFFECTED),
    ("last_affected", AFFECTED),
    ("fixed", FIXED),
    ("first_fixed", FIXED),
    ("known_not_affected", NOT_AFFECTED),
    ("under_investigation", UNDER_INVESTIGATION),
];

const VENDOR_FIX: &str = "vendor_fix";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRow {
    pub cve_id: String,
    pub catalog_key: String,
    pub csaf_product_id: String,
    pub version_expr: String,
    pub kind: VersionKind,
    pub status: &'static str,
    pub justification: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixRow {
    pub cve_id: String,
    pub catalog_key: String,
    pub fixed_version: Option<String>,
    pub details: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Extracted {
    pub statuses: Vec<StatusRow>,
    pub fixes: Vec<FixRow>,
    /// 至少有一個產品對應到目錄的 CVE
    pub cves: BTreeSet<String>,
    /// 格式不符而略過的 CVE ID
    pub invalid_cves: Vec<String>,
}

fn ids(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect()
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// `flags[]` 中套用到此產品的理由（label）。
fn justification(vuln: &Value, product_id: &str) -> Option<String> {
    vuln.get("flags")
        .and_then(Value::as_array)?
        .iter()
        .find(|f| ids(f, "product_ids").iter().any(|p| p == product_id))
        .and_then(|f| str_field(f, "label"))
}

pub fn extract(vulnerabilities: &[Value], products: &ProductMap) -> Extracted {
    let mut out = Extracted::default();
    for vuln in vulnerabilities {
        let Some(raw_cve) = str_field(vuln, "cve") else {
            continue;
        };
        let Ok(cve) = normalize_id(&raw_cve) else {
            out.invalid_cves.push(raw_cve);
            continue;
        };
        let statuses = statuses_of(vuln, &cve, products);
        let fixes = fixes_of(vuln, &cve, products, &statuses);
        if !statuses.is_empty() || !fixes.is_empty() {
            out.cves.insert(cve);
        }
        out.statuses.extend(statuses);
        out.fixes.extend(fixes);
    }
    out
}

fn statuses_of(vuln: &Value, cve: &str, products: &ProductMap) -> Vec<StatusRow> {
    let Some(ps) = vuln.get("product_status") else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for (category, status) in STATUS_MAP {
        for pid in ids(ps, category) {
            let Some(refs) = products.get(&pid) else {
                continue; // 對應不到目錄的產品忽略
            };
            let why = (status == NOT_AFFECTED)
                .then(|| justification(vuln, &pid))
                .flatten();
            rows.extend(refs.iter().map(|r: &ProductRef| StatusRow {
                cve_id: cve.to_string(),
                catalog_key: r.catalog_key.clone(),
                csaf_product_id: pid.clone(),
                version_expr: r.version_expr.clone(),
                kind: r.kind,
                status,
                justification: why.clone(),
            }));
        }
    }
    rows
}

fn fixes_of(vuln: &Value, cve: &str, products: &ProductMap, statuses: &[StatusRow]) -> Vec<FixRow> {
    let remediations = vuln
        .get("remediations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|r| r.get("category").and_then(Value::as_str) == Some(VENDOR_FIX));
    let mut rows: Vec<FixRow> = Vec::new();
    for r in remediations {
        let keys: BTreeSet<String> = ids(r, "product_ids")
            .iter()
            .filter_map(|pid| products.get(pid))
            .flatten()
            .map(|p| p.catalog_key.clone())
            .collect();
        for key in keys {
            let fixed: BTreeSet<&str> = statuses
                .iter()
                .filter(|s| {
                    s.catalog_key == key && s.status == FIXED && s.kind == VersionKind::Exact
                })
                .map(|s| s.version_expr.as_str())
                .collect();
            let row = FixRow {
                cve_id: cve.to_string(),
                catalog_key: key,
                fixed_version: (!fixed.is_empty())
                    .then(|| fixed.into_iter().collect::<Vec<_>>().join(", ")),
                details: str_field(r, "details"),
                url: str_field(r, "url"),
            };
            if !rows.contains(&row) {
                rows.push(row);
            }
        }
    }
    rows
}
