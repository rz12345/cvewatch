//! 18 任務 4.3、4.5：從 ISMS 清冊匯入（新增、預覽、重匯、沿用既有資產、退役資產、
//! 資產說明名稱衝突、不在 ISMS 清冊中的資產）。

mod common;

use std::path::PathBuf;

use chrono::Utc;
use common::matching::{world_with, World};
use cvewatch_core::assets::isms::{import_isms, IsmsImport};
use cvewatch_core::assets::{AssetRepo, AssetStatus, Cia, ComponentRepo, Exposure};
use cvewatch_core::matching::run::rematch_state;
use cvewatch_core::CoreError;
use rusqlite::params;

const NEW_TAGS: [&str; 6] = [
    "NET-HW-001",
    "NET-HW-002",
    "NET-HW-003",
    "NET-HW-004",
    "NET-SW-001",
    "WEB-SW-002",
];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn isms_world() -> World {
    let aliases = std::fs::read_to_string(fixture("isms_catalog.d/isms-test.toml")).unwrap();
    world_with(Some(&aliases))
}

fn run(w: &mut World, file: &str, dry_run: bool) -> Result<IsmsImport, CoreError> {
    import_isms(
        &mut w.conn,
        &w.classes,
        &w.catalog,
        &fixture(file),
        dry_run,
        Utc::now(),
    )
}

fn import(w: &mut World, dry_run: bool) -> IsmsImport {
    run(w, "isms_workbook.xlsx", dry_run).unwrap()
}

fn count(w: &World, table: &str) -> i64 {
    w.conn
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// 既有資產加上 ISMS 編號、C/I/A 與保管人。
fn with_isms(w: &World, tag: &str, isms_id: &str, cia: (u8, u8, u8), custodian: &str) {
    w.conn
        .execute(
            "UPDATE asset SET isms_id = ?2, confidentiality = ?3, integrity = ?4,
                              availability = ?5, custodian = ?6
             WHERE asset_tag = ?1",
            params![tag, isms_id, cia.0, cia.1, cia.2, custodian],
        )
        .unwrap();
}

fn versions(w: &World, tag: &str) -> Vec<(String, String)> {
    let asset = AssetRepo::new(&w.conn).find_by_tag(tag).unwrap().unwrap();
    ComponentRepo::new(&w.conn)
        .for_asset(asset.id)
        .unwrap()
        .into_iter()
        .map(|c| (c.label().to_string(), c.version_raw))
        .collect()
}

#[test]
fn importing_an_isms_workbook() {
    let mut w = isms_world();
    let result = import(&mut w, false);
    assert_eq!(result.plan.added, NEW_TAGS);
    assert_eq!(count(&w, "asset"), 6);
    assert_eq!(
        result.report.major_line().as_deref(),
        Some("略過 PE 1、IF 2、SE 1")
    );
    // aoscx、PAN-OS、aoscx、Veeam 為版本待補元件；Zyxel、Synology、官網系統為未辨識產品
    assert_eq!(result.report.pending_components, 4);
    assert_eq!(result.plan.pending_components(), 4);
    let nas = AssetRepo::new(&w.conn)
        .find_by_tag("NET-HW-003")
        .unwrap()
        .unwrap();
    assert_eq!(nas.record.exposure, Exposure::Internal);
    assert_eq!(nas.record.system_code.as_deref(), Some("NET"));
    let unresolved = AssetRepo::new(&w.conn).unresolved().unwrap();
    assert_eq!(unresolved.len(), 3);
    assert!(unresolved.iter().all(|u| u.is_pending()));
    assert!(
        rematch_state(&w.conn, &w.catalog, &w.settings)
            .unwrap()
            .needed(),
        "寫入後自動比對"
    );
}

#[test]
fn previewing_an_isms_import() {
    let mut w = isms_world();
    let preview = import(&mut w, true);
    assert_eq!(count(&w, "asset"), 0, "資料庫不變");
    assert_eq!(count(&w, "asset_system"), 0, "資料庫不變");
    let mut fresh = isms_world();
    let applied = import(&mut fresh, false);
    assert_eq!(preview, applied, "預覽與實際匯入相同");
}

#[test]
fn re_importing_the_same_isms_workbook() {
    let mut w = isms_world();
    import(&mut w, false);
    let again = import(&mut w, false);
    assert!(again.plan.added.is_empty(), "{:?}", again.plan.added);
    assert!(again.plan.updated.is_empty(), "{:?}", again.plan.updated);
    assert!(again.plan.systems.is_empty(), "{:?}", again.plan.systems);
    assert_eq!(again.report.pending_components, 0);
    assert_eq!(again.report.converted_existing, 6);
    assert_eq!(AssetRepo::new(&w.conn).unresolved().unwrap().len(), 3);
}

#[test]
fn isms_id_already_in_cvewatch() {
    let mut w = isms_world();
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.6")]);
    with_isms(&w, "FW-01", "NET-HW-002", (2, 3, 4), "李小華");

    let preview = import(&mut w, true);
    let fw = preview
        .plan
        .updated
        .iter()
        .find(|u| u.asset_tag == "FW-01")
        .expect("FW-01 更新");
    let lines: Vec<String> = fw
        .changed_fields
        .iter()
        .map(|c| c.describe(&w.classes))
        .collect();
    assert!(lines.contains(&"機密性 2 → 3".to_string()), "{lines:?}");
    assert!(
        lines.contains(&"保管人 李小華 → 王小明".to_string()),
        "{lines:?}"
    );
    assert!(fw.version_changes.is_empty() && fw.components_removed.is_empty());

    import(&mut w, false);
    let fw = AssetRepo::new(&w.conn)
        .find_by_tag("FW-01")
        .unwrap()
        .unwrap();
    assert_eq!(fw.record.cia, Cia::new(3, 3, 4));
    assert_eq!(fw.record.exposure, Exposure::Internet);
    assert_eq!(
        versions(&w, "FW-01"),
        [("paloalto-panos".to_string(), "11.1.6".to_string())]
    );
    assert!(AssetRepo::new(&w.conn)
        .find_by_tag("NET-HW-002")
        .unwrap()
        .is_none());
    assert_eq!(count(&w, "asset"), 6);
}

#[test]
fn new_product_on_an_existing_asset() {
    let mut w = isms_world();
    w.asset("FW-01", "internet", &[("dell-idrac9", "7.10.30.00")]);
    with_isms(&w, "FW-01", "NET-HW-002", (3, 3, 4), "王小明");
    let result = import(&mut w, false);
    let fw = result
        .plan
        .updated
        .iter()
        .find(|u| u.asset_tag == "FW-01")
        .unwrap();
    assert_eq!(fw.components_added, ["paloalto-panos"]);
    assert_eq!(
        versions(&w, "FW-01"),
        [
            ("dell-idrac9".to_string(), "7.10.30.00".to_string()),
            ("paloalto-panos".to_string(), String::new()),
        ]
    );
}

#[test]
fn isms_id_of_a_retired_asset() {
    let mut w = isms_world();
    w.asset("NAS-02", "internal", &[("synology-dsm", "7.2")]);
    with_isms(&w, "NAS-02", "NET-HW-003", (2, 3, 4), "王小明");
    w.retire("NAS-02");
    let result = import(&mut w, false);
    assert!(
        result.report.skipped_rows.iter().any(|s| s.to_string()
            == "第 7 列：ISMS 編號 NET-HW-003 屬於已退役資產 NAS-02，須先恢復該資產"),
        "{:?}",
        result.report.skipped_rows
    );
    let nas = AssetRepo::new(&w.conn)
        .find_by_tag("NAS-02")
        .unwrap()
        .unwrap();
    assert_eq!(nas.status, AssetStatus::Retired);
    assert_eq!(result.plan.added.len(), 5);
}

#[test]
fn system_name_used_by_another_code() {
    let mut w = isms_world();
    w.conn
        .execute(
            "INSERT INTO asset_system (code, name) VALUES ('SYS', '網路服務')",
            [],
        )
        .unwrap();
    let CoreError::ImportRejected(errors) = run(&mut w, "isms_workbook.xlsx", false).unwrap_err()
    else {
        panic!("應中止匯入");
    };
    assert!(
        errors
            .iter()
            .any(|e| e.contains("網路服務") && e.contains("SYS")),
        "{errors:?}"
    );
    assert_eq!(count(&w, "asset"), 0, "資料庫不變");
    assert_eq!(count(&w, "asset_system"), 1, "資料庫不變");
}

#[test]
fn assets_missing_from_the_isms_workbook() {
    let mut w = isms_world();
    w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.4")]);
    let result = import(&mut w, false);
    assert_eq!(result.report.not_in_workbook, ["PVE-01"]);
    let pve = AssetRepo::new(&w.conn)
        .find_by_tag("PVE-01")
        .unwrap()
        .unwrap();
    assert_eq!(pve.status, AssetStatus::Active, "不退役");
}

#[test]
fn not_an_isms_workbook() {
    let mut w = isms_world();
    let e = run(&mut w, "isms_not_workbook.xlsx", false)
        .unwrap_err()
        .to_string();
    assert!(e.contains("找不到 ISMS 資訊資產清冊的表頭"), "{e}");
    assert_eq!(count(&w, "asset"), 0);
}
