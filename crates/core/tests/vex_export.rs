//! 07 任務 2.3：vex-exchange 匯出 — CycloneDX 1.6 schema 驗證、golden file、狀態對應、篩選、加密。

mod common;

use common::vex::{
    fixed_meta, schema_errors, vex_fixture, vex_world, FW, NOTE_NOT_AFFECTED, NOT_AFFECTED_CVE,
    OPEN_CVE, PKG_CVE, RESOLVED_CVE, RISK_CVE, WEB,
};
use cvewatch_core::matching::query::ids as finding_ids;
use cvewatch_core::matching::query::FindingFilter;
use cvewatch_core::matching::triage::Status;
use cvewatch_core::vex::export::{build, to_json};
use cvewatch_core::vex::file::{decrypt, export_to_file, is_encrypted};
use serde_json::Value;

const GOLDEN: &str = "golden/export.json";

/// 匯出全部命中的 JSON 文字（實際寫入檔案的內容）。
fn export_text(w: &common::matching::World) -> String {
    let ids = finding_ids(&w.conn, &FindingFilter::default()).unwrap();
    let doc = build(&w.conn, &ids, &fixed_meta()).unwrap();
    to_json(&doc).unwrap()
}

fn export_all(w: &common::matching::World) -> Value {
    serde_json::from_str(&export_text(w)).unwrap()
}

/// 某元件 × CVE 的 vulnerability。
fn vuln<'a>(doc: &'a Value, asset: &str, cve: &str) -> &'a Value {
    let refs = |v: &&Value| {
        v["affects"][0]["ref"]
            .as_str()
            .is_some_and(|r| r.starts_with(&format!("cvewatch:{asset}:")))
    };
    doc["vulnerabilities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["id"] == cve)
        .find(refs)
        .unwrap_or_else(|| panic!("{asset} × {cve}"))
}

// ── Requirement: Export findings as CycloneDX VEX ─────────────

#[test]
fn export_passes_cyclonedx_schema_and_matches_golden_file() {
    let w = vex_world();
    let text = export_text(&w);
    let doc: Value = serde_json::from_str(&text).unwrap();
    let errors = schema_errors(&doc);
    assert!(errors.is_empty(), "schema 錯誤：{errors:#?}");
    let golden = vex_fixture(GOLDEN);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden.parent().unwrap()).unwrap();
        std::fs::write(&golden, &text).unwrap();
    }
    let expected = std::fs::read_to_string(&golden).expect("golden file（UPDATE_GOLDEN=1 產生）");
    assert_eq!(text, expected.replace("\r\n", "\n"));
}

#[test]
fn schema_validator_rejects_invalid_documents() {
    let w = vex_world();
    let mut doc = export_all(&w);
    doc["vulnerabilities"][0]["analysis"]["state"] = "maybe".into();
    assert!(!schema_errors(&doc).is_empty(), "驗證器必須真的檢查內容");
}

#[test]
fn exporting_one_asset_only_contains_its_components_and_findings() {
    let w = vex_world();
    let filter = FindingFilter {
        asset_tag: Some("fw-01".into()),
        ..FindingFilter::default()
    };
    let ids = finding_ids(&w.conn, &filter).unwrap();
    let doc: Value = serde_json::to_value(build(&w.conn, &ids, &fixed_meta()).unwrap()).unwrap();
    assert!(schema_errors(&doc).is_empty());
    let components = doc["components"].as_array().unwrap();
    assert_eq!(components.len(), 1);
    assert_eq!(components[0]["bom-ref"], "cvewatch:FW-01:paloalto-panos");
    assert_eq!(doc["vulnerabilities"].as_array().unwrap().len(), 4);
    assert!(doc["vulnerabilities"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["affects"][0]["ref"] == "cvewatch:FW-01:paloalto-panos"));
}

#[test]
fn status_filter_limits_findings() {
    let w = vex_world();
    let filter = FindingFilter {
        statuses: vec![Status::Ignored],
        ..FindingFilter::default()
    };
    assert_eq!(finding_ids(&w.conn, &filter).unwrap().len(), 3);
}

#[test]
fn product_component_uses_cpe_and_package_uses_purl() {
    let w = vex_world();
    let doc = export_all(&w);
    let components = doc["components"].as_array().unwrap();
    let fw = components
        .iter()
        .find(|c| c["bom-ref"] == "cvewatch:FW-01:paloalto-panos")
        .unwrap();
    assert_eq!(
        fw["cpe"],
        "cpe:2.3:o:paloaltonetworks:pan-os:11.1.4-h7:*:*:*:*:*:*:*"
    );
    assert_eq!(fw["type"], "operating-system");
    assert!(fw.get("purl").is_none());
    assert_eq!(fw["properties"][0]["value"], FW);
    let web = components
        .iter()
        .find(|c| c["bom-ref"] == "cvewatch:WEB-01:pkg:deb/debian/openssl")
        .unwrap();
    assert_eq!(web["type"], "library");
    assert_eq!(web["name"], "openssl");
    assert!(web["purl"]
        .as_str()
        .unwrap()
        .starts_with("pkg:deb/debian/openssl"));
    assert!(web.get("cpe").is_none());
    assert_eq!(web["properties"][0]["value"], WEB);
}

// ── Requirement: Status mapping on export ─────────────────────

#[test]
fn statuses_map_to_analysis() {
    let w = vex_world();
    let doc = export_all(&w);
    let open = &vuln(&doc, FW, OPEN_CVE)["analysis"];
    assert_eq!(open["state"], "in_triage");
    assert!(open.get("detail").is_none());
    let resolved = &vuln(&doc, FW, RESOLVED_CVE)["analysis"];
    assert_eq!(
        (&resolved["state"], &resolved["response"][0]),
        (&"resolved".into(), &"update".into())
    );
    let risk = &vuln(&doc, FW, RISK_CVE)["analysis"];
    assert_eq!(risk["state"], "exploitable");
    assert_eq!(risk["response"], serde_json::json!(["will_not_fix"]));
    let fp = &vuln(&doc, WEB, PKG_CVE)["analysis"];
    assert_eq!(fp["state"], "false_positive");
    assert!(fp.get("detail").is_none(), "空白備註不輸出 detail");
}

#[test]
fn not_affected_exports_justification_and_detail() {
    let w = vex_world();
    let doc = export_all(&w);
    let a = &vuln(&doc, FW, NOT_AFFECTED_CVE)["analysis"];
    assert_eq!(a["state"], "not_affected");
    assert_eq!(a["justification"], "requires_configuration");
    assert_eq!(a["detail"], NOTE_NOT_AFFECTED);
    assert_eq!(a["firstIssued"], "2026-11-01T00:00:00Z");
    assert_eq!(a["lastUpdated"], "2026-11-02T00:00:00Z");
}

#[test]
fn vulnerability_carries_ratings_source_and_properties() {
    let w = vex_world();
    let doc = export_all(&w);
    let v = vuln(&doc, FW, OPEN_CVE);
    assert_eq!(v["source"]["name"], "CVE");
    assert_eq!(v["ratings"][0]["method"], "CVSSv31");
    assert_eq!(v["ratings"][0]["source"]["name"], "NVD");
    let props: Vec<_> = v["properties"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(props, ["cvewatch:priority", "cvewatch:confidence"]);
}

#[test]
fn export_contains_no_notification_settings() {
    let w = vex_world();
    let text = export_all(&w).to_string().to_lowercase();
    for word in ["smtp", "redmine", "password", "api_key"] {
        assert!(!text.contains(word), "{word}");
    }
}

// ── Requirement: Export file protection ──────────────────────

#[test]
fn encrypted_export_writes_only_age_file() {
    let w = vex_world();
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("all.json");
    let ids = finding_ids(&w.conn, &FindingFilter::default()).unwrap();
    let outcome = export_to_file(&w.conn, &ids, &fixed_meta(), &plain, Some("口令 123")).unwrap();
    assert_eq!(outcome.path, dir.path().join("all.json.age"));
    assert!(outcome.encrypted);
    assert!(!plain.exists(), "不產生明文檔");
    let bytes = std::fs::read(&outcome.path).unwrap();
    assert!(is_encrypted(&bytes));
    let json: Value = serde_json::from_slice(&decrypt(&bytes, "口令 123").unwrap()).unwrap();
    assert_eq!(json["vulnerabilities"].as_array().unwrap().len(), 5);
}

#[test]
fn plain_export_does_not_overwrite_existing_file() {
    let w = vex_world();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("all.json");
    std::fs::write(&path, "keep").unwrap();
    let err = export_to_file(&w.conn, &[], &fixed_meta(), &path, None).unwrap_err();
    assert!(err.to_string().contains("已存在"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
}
