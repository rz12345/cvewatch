//! `export.rs` 的單元測試（23 任務 2.1、2.3）。

use chrono::{NaiveDate, Utc};
use rusqlite::Connection;

use super::*;
use crate::assets::classes::{Cia, ClassCode};
use crate::assets::export::product_text;
use crate::assets::isms::describe::tests::catalog;
use crate::assets::model::Exposure;
use crate::assets::systems::{create_system, AssetSystem, SystemCode};
use crate::db::migrate::migrate;

fn record(tag: &str, isms_id: Option<&str>) -> AssetRecord {
    AssetRecord {
        asset_tag: tag.into(),
        name: tag.to_lowercase(),
        category: ClassCode::parse("HW02").unwrap(),
        location: None,
        exposure: Exposure::Internal,
        cia: Cia::new(2, 3, 4),
        isms_id: isms_id.map(str::to_string),
        system_code: isms_id.and_then(|id| id.split('-').next().map(str::to_uppercase)),
        responsible_unit: None,
        custodian: None,
        user_unit: None,
        compliance: None,
        personal_data: false,
        brand: None,
        model: None,
        serial_number: None,
        property_number: None,
        hostname: None,
        management_url: None,
        warranty_end: None,
        support_end: None,
        license_end: None,
        hw_eol_override: None,
        notes: None,
    }
}

fn systems() -> SystemDirectory {
    SystemDirectory::new(
        [("NET", "網路服務"), ("SYS", "系統服務")]
            .into_iter()
            .map(|(code, name)| AssetSystem {
                code: SystemCode::parse(code).unwrap(),
                name: name.into(),
            })
            .collect(),
    )
}

fn export(records: &[(AssetRecord, Vec<InventoryItem>)]) -> IsmsExport {
    let classes = AssetClasses::builtin();
    let catalog = catalog();
    let systems = systems();
    let assets: Vec<ExportAsset<'_>> = records
        .iter()
        .map(|(record, items)| ExportAsset { record, items })
        .collect();
    build_export(
        &assets,
        ExportContext {
            classes: &classes,
            catalog: &catalog,
            systems: &systems,
        },
    )
}

fn aoscx() -> Vec<InventoryItem> {
    let catalog = catalog();
    vec![InventoryItem {
        key: Some("hpe-aoscx".into()),
        product: product_text(&catalog, "hpe-aoscx"),
        version: "1.2.3".into(),
    }]
}

/// Scenario: Mapping an exported row
#[test]
fn mapping_an_exported_row() {
    let fw = AssetRecord {
        custodian: Some("王小明".into()),
        brand: Some("Aruba".into()),
        model: Some("XYZ-100".into()),
        ..record("FW-EDGE", Some("NET-HW-001"))
    };
    let out = export(&[(fw, aoscx())]);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(
        out.rows,
        vec![IsmsExportRow {
            system_id: "NET-".into(),
            serial: "HW-001".into(),
            category: "HW02".into(),
            system_name: "網路服務".into(),
            class_name: "通訊設備".into(),
            description: "廠牌型號：【Aruba XYZ-100】\nAOS-CX（1.2.3）".into(),
            responsible_unit: String::new(),
            custodian: "王小明".into(),
            user_unit: String::new(),
            confidentiality: 2,
            integrity: 3,
            availability: 4,
            value: 4,
            compliance: None,
            protection_level: None,
            personal_data: "N".into(),
        }]
    );
}

/// Scenario: Compliance raises the exported protection level
#[test]
fn compliance_raises_the_exported_protection_level() {
    let r = AssetRecord {
        cia: Cia::new(2, 3, 3),
        compliance: Some(4),
        personal_data: true,
        ..record("FW-EDGE", Some("NET-HW-001"))
    };
    let row = &export(&[(r, aoscx())]).rows[0];
    assert_eq!(
        (row.value, row.compliance, row.protection_level),
        (3, Some(4), Some(4))
    );
    assert_eq!(row.personal_data, "Y");
}

/// Scenario: Rows sorted by ISMS ID
#[test]
fn rows_sorted_by_isms_id() {
    let out = export(&[
        (record("A-01", Some("SYS-HW-002")), aoscx()),
        (record("B-01", Some("net-hw-001")), aoscx()),
    ]);
    let ids: Vec<(&str, &str)> = out
        .rows
        .iter()
        .map(|r| (r.system_id.as_str(), r.serial.as_str()))
        .collect();
    assert_eq!(ids, [("net-", "hw-001"), ("SYS-", "HW-002")]);
}

/// Scenario: Asset without an ISMS ID
#[test]
fn asset_without_an_isms_id() {
    let out = export(&[
        (record("SRV-01", None), aoscx()),
        (record("FW-EDGE", Some("NET-HW-001")), aoscx()),
    ]);
    assert_eq!(out.rows.len(), 1);
    assert_eq!(out.rows[0].serial, "HW-001");
    assert_eq!(out.skipped, ["SRV-01"]);
}

#[test]
fn too_long_cell_is_truncated_with_a_warning() {
    let r = AssetRecord {
        custodian: Some("王".repeat(EXCEL_CELL_MAX_CHARS + 5)),
        ..record("FW-EDGE", Some("NET-HW-001"))
    };
    let out = export(&[(r, aoscx())]);
    assert_eq!(out.rows[0].custodian.chars().count(), EXCEL_CELL_MAX_CHARS);
    assert_eq!(
        out.warnings,
        ["FW-EDGE 的保管人超過 Excel 儲存格上限 32767 字，已截斷；以 ISMS 匯入讀回時會與原本不同"]
    );
    let bytes = super::super::export_write::write_workbook(&out.rows, day());
    assert!(bytes.is_ok(), "截斷後可以寫出");
}

#[test]
fn skipped_line_lists_every_asset() {
    let summary = IsmsExportSummary {
        exported: 1,
        skipped: vec!["SRV-01".into(), "SRV-02".into()],
        warnings: Vec::new(),
    };
    assert_eq!(
        summary.skipped_line().as_deref(),
        Some("略過 2 個沒有 ISMS 編號的資產：SRV-01、SRV-02")
    );
    assert_eq!(IsmsExportSummary::default().skipped_line(), None);
}

#[test]
fn undefined_class_leaves_the_name_blank() {
    let r = AssetRecord {
        category: ClassCode::parse("HW99").unwrap(),
        ..record("FW-EDGE", Some("NET-HW-001"))
    };
    let out = export(&[(r, aoscx())]);
    assert_eq!(out.rows[0].class_name, "");
    assert_eq!(out.rows[0].category, "HW99");
    assert_eq!(
        out.warnings,
        ["FW-EDGE 的資產類別 HW99 已不在設定中，資產名稱留空；以 ISMS 匯入讀回時會被略過"]
    );
}

#[test]
fn system_name_from_the_isms_id() {
    let r = AssetRecord {
        system_code: None,
        ..record("FW-EDGE", Some("NET-HW-001"))
    };
    assert_eq!(export(&[(r, aoscx())]).rows[0].system_name, "網路服務");
    let r = record("X-01", Some("ZZZ-HW-001"));
    let out = export(&[(r, aoscx())]);
    assert_eq!(out.rows[0].system_name, "");
    assert_eq!(
        out.warnings,
        ["X-01 的 ISMS 編號「ZZZ-HW-001」沒有對應的資產說明，資產說明留空"]
    );
}

fn db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    migrate(&mut conn).unwrap();
    create_system(&conn, "NET", "網路服務").unwrap();
    conn
}

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
}

/// Scenario: Retired asset is not exported
#[test]
fn retired_asset_is_not_exported() {
    let conn = db();
    let repo = AssetRepo::new(&conn);
    let id = repo
        .insert(&record("OLD-01", Some("NET-HW-005")), Utc::now())
        .unwrap();
    repo.retire(id, Utc::now()).unwrap();
    repo.insert(&record("FW-EDGE", Some("NET-HW-001")), Utc::now())
        .unwrap();
    repo.insert(&record("SRV-01", None), Utc::now()).unwrap();
    let out = isms_export_rows(&conn, &AssetClasses::builtin(), &catalog()).unwrap();
    let serials: Vec<&str> = out.rows.iter().map(|r| r.serial.as_str()).collect();
    assert_eq!(serials, ["HW-001"]);
    assert_eq!(out.skipped, ["SRV-01"], "retired 資產不列在略過清單");
}

/// Scenario: ISMS export target exists
#[test]
fn isms_export_target_exists() {
    let conn = db();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("isms-out.xlsx");
    std::fs::write(&path, b"original").unwrap();
    let e = export_isms(&conn, &AssetClasses::builtin(), &catalog(), &path, day())
        .unwrap_err()
        .to_string();
    assert!(e.contains("目的檔已存在"), "{e}");
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
}

#[test]
fn no_assets_still_writes_the_header() {
    let conn = db();
    AssetRepo::new(&conn)
        .insert(&record("SRV-01", None), Utc::now())
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("isms-out.xlsx");
    let summary = export_isms(&conn, &AssetClasses::builtin(), &catalog(), &path, day()).unwrap();
    assert_eq!(summary.exported, 0);
    assert_eq!(summary.skipped, ["SRV-01"]);
    let book = crate::assets::isms::workbook_read::read_isms_workbook(&path).unwrap();
    assert_eq!(book.header_row, 4);
    assert!(book.rows.is_empty());
}
