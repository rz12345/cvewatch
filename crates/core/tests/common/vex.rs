//! VEX 測試共用：含各種處理狀態的資料庫、CycloneDX 1.6 schema 驗證器與 fixture 路徑。

use std::path::{Path, PathBuf};

use cvewatch_core::matching::triage::{self, ChangeRequest};
use cvewatch_core::vex::export::ExportMeta;
use serde_json::{json, Value};

use super::matching::{at, panos, world, World};

pub const FW: &str = "FW-01";
pub const WEB: &str = "WEB-01";
pub const OPEN_CVE: &str = "CVE-2026-1001";
pub const RESOLVED_CVE: &str = "CVE-2026-1002";
pub const NOT_AFFECTED_CVE: &str = "CVE-2026-1003";
pub const RISK_CVE: &str = "CVE-2026-1004";
pub const PKG_CVE: &str = "CVE-2023-5678";
pub const PANOS_VERSION: &str = "11.1.4-h7";
pub const OPENSSL_PURL: &str = "pkg:deb/debian/openssl";
pub const OPENSSL_VERSION: &str = "3.0.11-1";
/// 比對時間
pub const MATCHED: &str = "2026-11-01T00:00:00+00:00";
/// 使用者變更狀態的時間
pub const TRIAGED: &str = "2026-11-02T00:00:00+00:00";
/// 匯出時間
pub const EXPORTED: &str = "2026-11-03T00:00:00+00:00";
pub const NOTE_RESOLVED: &str = "已升級至 11.1.5";
pub const NOTE_NOT_AFFECTED: &str = "未啟用 GlobalProtect";
pub const NOTE_RISK: &str = "內網設備，接受風險";

/// FW-01（PAN-OS 11.1.4-h7）四筆命中：未處理、已處理、不受影響、接受風險；
/// WEB-01（openssl 套件）一筆比對錯誤。
pub fn vex_world() -> World {
    let mut w = world();
    w.asset(FW, "internet", &[("paloalto-panos", PANOS_VERSION)]);
    for (i, cve) in [OPEN_CVE, RESOLVED_CVE, NOT_AFFECTED_CVE, RISK_CVE]
        .iter()
        .enumerate()
    {
        w.cve(
            cve,
            panos(json!([{"version": "11.1.0", "lessThan": "11.1.5", "status": "affected"}])),
            json!([]),
        );
        w.cvss(cve, 9.0 - i as f64);
    }
    w.asset(WEB, "dmz", &[]);
    let pkg = w.package(WEB, OPENSSL_PURL, OPENSSL_VERSION, MATCHED);
    w.osv_hit(pkg, "DEBIAN-CVE-2023-5678", &[PKG_CVE], MATCHED);
    w.osv_synced(MATCHED);
    w.run_at(MATCHED);
    set(
        &w,
        FW,
        RESOLVED_CVE,
        req("resolved", None, None, NOTE_RESOLVED),
        TRIAGED,
    );
    set(
        &w,
        FW,
        NOT_AFFECTED_CVE,
        req(
            "ignored",
            Some("not_affected"),
            Some("requires_configuration"),
            NOTE_NOT_AFFECTED,
        ),
        TRIAGED,
    );
    set(
        &w,
        FW,
        RISK_CVE,
        req("ignored", Some("risk_accepted"), None, NOTE_RISK),
        TRIAGED,
    );
    set(
        &w,
        WEB,
        PKG_CVE,
        req("ignored", Some("false_positive"), None, ""),
        TRIAGED,
    );
    w
}

pub fn req(
    status: &str,
    reason: Option<&str>,
    justification: Option<&str>,
    note: &str,
) -> ChangeRequest {
    ChangeRequest {
        status: status.into(),
        reason: reason.map(str::to_string),
        justification: justification.map(str::to_string),
        note: note.into(),
    }
}

/// 命中 id（資產 × CVE）。
pub fn finding_id(w: &World, asset: &str, cve: &str) -> i64 {
    w.conn
        .query_row(
            "SELECT f.id FROM finding f JOIN component c ON c.id = f.component_id
             JOIN asset a ON a.id = c.asset_id WHERE a.asset_tag = ?1 AND f.cve_id = ?2",
            rusqlite::params![asset, cve],
            |r| r.get(0),
        )
        .expect("finding")
}

/// 以使用者身分變更狀態。
pub fn set(w: &World, asset: &str, cve: &str, request: ChangeRequest, when: &str) {
    let change = triage::validate(&request).expect("有效的變更");
    triage::set_status(&w.conn, finding_id(w, asset, cve), &change, at(when)).expect("set");
}

/// (狀態, 原因, 理由)
pub fn verdict(w: &World, asset: &str, cve: &str) -> (String, Option<String>, Option<String>) {
    w.conn
        .query_row(
            "SELECT status, ignore_reason, justification FROM finding WHERE id = ?1",
            [finding_id(w, asset, cve)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("verdict")
}

/// 固定序號與時間（golden file）。
pub fn fixed_meta() -> ExportMeta {
    ExportMeta {
        serial_number: "urn:uuid:00000000-0000-4000-8000-000000000007".into(),
        timestamp: at(EXPORTED),
        tool_version: "0.0.0-test".into(),
    }
}

/// `tests/fixtures/vex/<rel>`
pub fn vex_fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/vex")
        .join(rel)
}

fn schema_json(name: &str) -> Value {
    let path = vex_fixture(&format!("schema/{name}"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("schema json")
}

/// 以 CycloneDX 1.6 JSON schema 驗證；回傳所有錯誤訊息。
pub fn schema_errors(instance: &Value) -> Vec<String> {
    let registry = jsonschema::Registry::new()
        .add(
            "http://cyclonedx.org/schema/spdx.schema.json",
            schema_json("spdx.schema.json"),
        )
        .expect("spdx")
        .add(
            "http://cyclonedx.org/schema/jsf-0.82.schema.json",
            schema_json("jsf-0.82.schema.json"),
        )
        .expect("jsf")
        .prepare()
        .expect("registry");
    let schema = schema_json("bom-1.6.schema.json");
    let validator = jsonschema::options()
        .with_registry(&registry)
        .build(&schema)
        .expect("build validator");
    validator
        .iter_errors(instance)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect()
}
