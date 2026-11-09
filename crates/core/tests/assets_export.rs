//! 10 任務 3.1～3.2：範本與清冊匯出不覆蓋既有檔案；匯出內容（未辨識產品保留、退役與套件排除、
//! 沒有元件的警告）與重匯往返。

mod common;

use std::path::{Path, PathBuf};

use chrono::Utc;
use common::matching::{world, World, T0};
use cvewatch_core::assets::columns::ColumnMap;
use cvewatch_core::assets::export::{export_inventory, inventory_rows};
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::plan::ImportPlan;
use cvewatch_core::assets::sheet::read_inventory;
use cvewatch_core::assets::template::write_template;
use cvewatch_core::assets::validate::parse_inventory;
use cvewatch_core::assets::{AssetClasses, AssetRepo};
use rusqlite::params;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn import(w: &mut World, path: &Path, dry_run: bool) -> ImportPlan {
    let book = read_inventory(path, None, None).unwrap();
    let inventory =
        parse_inventory(&book, &ColumnMap::default(), &AssetClasses::builtin()).unwrap();
    import_inventory(
        &mut w.conn,
        &w.catalog,
        &inventory,
        ImportOptions {
            dry_run,
            prune: false,
        },
        Utc::now(),
    )
    .unwrap()
}

fn unresolved(w: &World, tag: &str, product: &str, version: &str) {
    w.conn
        .execute(
            "INSERT INTO asset_unresolved (asset_id, product_raw, version_raw, seen_at)
             VALUES ((SELECT id FROM asset WHERE asset_tag = ?1), ?2, ?3, ?4)",
            params![tag, product, version, T0],
        )
        .unwrap();
}

#[test]
fn existing_target_is_not_overwritten() {
    let w = world();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inv.xlsx");
    std::fs::write(&path, b"original bytes").unwrap();
    let template = write_template(&path, &AssetClasses::builtin())
        .unwrap_err()
        .to_string();
    let export = export_inventory(&w.conn, &w.catalog, &path)
        .unwrap_err()
        .to_string();
    let expected = format!("目的檔已存在，不覆蓋：{}", path.display());
    assert_eq!(template, expected);
    assert_eq!(export, expected);
    assert_eq!(std::fs::read(&path).unwrap(), b"original bytes");
}

#[test]
fn unresolved_product_is_kept() {
    let mut w = world();
    w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.4")]);
    unresolved(&w, "PVE-01", "Proxmox Backup", "3.2");
    let rows = inventory_rows(&w.conn, &w.catalog).unwrap();
    assert_eq!(rows.assets.len(), 1);
    assert_eq!(rows.assets[0][3..5], ["Proxmox VE", "8.2.4"]);
    assert_eq!(
        rows.components,
        vec![vec![
            "PVE-01".to_string(),
            "Proxmox Backup".into(),
            "3.2".into()
        ]]
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inv.xlsx");
    export_inventory(&w.conn, &w.catalog, &path).unwrap();
    import(&mut w, &path, false);
    let listed = AssetRepo::new(&w.conn).unresolved().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (
            listed[0].product_raw.as_str(),
            listed[0].version_raw.as_str()
        ),
        ("Proxmox Backup", "3.2")
    );
}

#[test]
fn retired_assets_and_packages_are_left_out() {
    let w = world();
    w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.retire("NAS-02");
    w.asset("WEB-01", "dmz", &[("debian", "12.7")]);
    w.package("WEB-01", "pkg:deb/debian/openssl", "3.0.11-1", T0);
    let rows = inventory_rows(&w.conn, &w.catalog).unwrap();
    let tags: Vec<&str> = rows.assets.iter().map(|r| r[0].as_str()).collect();
    assert_eq!(tags, vec!["WEB-01"]);
    assert_eq!(rows.assets[0][3..5], ["Debian", "12.7"]);
    assert!(rows.components.is_empty(), "{:?}", rows.components);
    assert!(rows.warnings.is_empty());
}

#[test]
fn asset_without_components_exports_with_a_warning() {
    let w = world();
    w.asset("OLD-01", "internal", &[]);
    let dir = tempfile::tempdir().unwrap();
    let summary = export_inventory(&w.conn, &w.catalog, &dir.path().join("inv.xlsx")).unwrap();
    assert_eq!(summary.assets, 1);
    assert_eq!(summary.warnings.len(), 1);
    assert!(
        summary.warnings[0].contains("OLD-01")
            && summary.warnings[0].contains("重匯前需補上產品與版本"),
        "{:?}",
        summary.warnings
    );
}

#[test]
fn round_trip_without_changes() {
    let mut w = world();
    let first = import(&mut w, &fixture("inventory_basic.xlsx"), false);
    assert!(!first.added.is_empty());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inv.xlsx");
    let summary = export_inventory(&w.conn, &w.catalog, &path).unwrap();
    assert_eq!(summary.assets, first.added.len());
    let again = import(&mut w, &path, true);
    assert!(again.added.is_empty(), "{:?}", again.added);
    assert!(again.updated.is_empty(), "{:?}", again.updated);
    assert_eq!(again.version_change_count(), 0);
    assert!(again.disappeared.is_empty(), "{:?}", again.disappeared);
    assert_eq!(again.unchanged, first.added.len());
    assert!(again.notices.is_empty(), "{:?}", again.notices);
}

/// category 寫子類別代碼、C/I/A 寫實際值（不留空）、ISMS 編號原樣寫出。
#[test]
fn export_writes_class_code_and_actual_ratings() {
    let mut w = world();
    import(&mut w, &fixture("inventory_basic.xlsx"), false);
    let rows = inventory_rows(&w.conn, &w.catalog).unwrap();
    let fw = rows.assets.iter().find(|r| r[0] == "FW-01").unwrap();
    // asset_tag, name, category, product, version, exposure, C, I, A, isms_id, ...
    assert_eq!(fw[2], "HW03");
    assert_eq!(fw[6..10], ["2", "3", "4", "SYS-HW-003"]);
    let nas = rows.assets.iter().find(|r| r[0] == "NAS-01").unwrap();
    assert_eq!(nas[2], "HW05", "以名稱匯入的類別匯出為代碼");
}

#[test]
fn export_uses_the_current_catalog_names() {
    let mut w = world();
    w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.4")]);
    // 資料庫中的目錄名稱尚未同步（例如剛修改 catalog.d）
    w.conn
        .execute(
            "UPDATE catalog_product SET display_name = '舊名稱' WHERE key = 'proxmox-ve'",
            [],
        )
        .unwrap();
    let rows = inventory_rows(&w.conn, &w.catalog).unwrap();
    assert_eq!(rows.assets[0][3], "Proxmox VE");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inv.xlsx");
    export_inventory(&w.conn, &w.catalog, &path).unwrap();
    let again = import(&mut w, &path, true);
    assert!(
        again.updated.is_empty() && again.unresolved.is_empty(),
        "{again:?}"
    );
}
