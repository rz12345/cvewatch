//! 18 任務 1.4～1.5：版本待補的清冊匯入、版本歷史與匯出往返。

mod common;

use std::path::Path;

use chrono::Utc;
use common::matching::{world, World};
use cvewatch_core::assets::columns::{ColumnMap, SheetKind};
use cvewatch_core::assets::export::{export_inventory, inventory_rows, InventoryRows};
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::plan::ImportPlan;
use cvewatch_core::assets::sheet::read_inventory;
use cvewatch_core::assets::template::{write_sheets, SheetData};
use cvewatch_core::assets::validate::parse_inventory;
use cvewatch_core::assets::{AssetClasses, AssetRepo, AssetStatus, Component, ComponentRepo};

fn import(w: &mut World, path: &Path, dry_run: bool) -> ImportPlan {
    let book = read_inventory(path, None).unwrap();
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

fn headers(kind: SheetKind) -> Vec<String> {
    kind.columns()
        .iter()
        .map(|f| f.as_str().to_string())
        .collect()
}

/// 以匯出的資料列寫成清冊檔（測試可先修改資料列）。
fn write_rows(path: &Path, rows: InventoryRows) {
    write_sheets(
        path,
        &[
            SheetData {
                name: SheetKind::Assets.sheet_name(),
                headers: headers(SheetKind::Assets),
                rows: rows.assets,
            },
            SheetData {
                name: SheetKind::Components.sheet_name(),
                headers: headers(SheetKind::Components),
                rows: rows.components,
            },
            SheetData {
                name: SheetKind::Systems.sheet_name(),
                headers: headers(SheetKind::Systems),
                rows: rows.systems,
            },
        ],
    )
    .unwrap();
}

/// 第一個資產列的版本改為 `version` 後寫成清冊檔。
fn with_version(w: &World, path: &Path, version: &str) {
    let mut rows = inventory_rows(&w.conn, &w.catalog).unwrap();
    rows.assets[0][4] = version.to_string();
    write_rows(path, rows);
}

fn component(w: &World, tag: &str) -> Component {
    let asset = AssetRepo::new(&w.conn).find_by_tag(tag).unwrap().unwrap();
    ComponentRepo::new(&w.conn)
        .for_asset(asset.id)
        .unwrap()
        .remove(0)
}

fn history(w: &World, component_id: i64) -> Vec<String> {
    ComponentRepo::new(&w.conn)
        .history(component_id)
        .unwrap()
        .into_iter()
        .map(|h| h.version_raw)
        .collect()
}

#[test]
fn new_component_without_version_is_pending() {
    let w = world();
    w.asset("NAS-03", "internal", &[("synology-dsm", "7.2")]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inv.xlsx");
    with_version(&w, &path, "");
    // 換成全新的資料庫，NAS-03 為新資產
    let mut fresh = world();
    let plan = import(&mut fresh, &path, false);
    assert_eq!(plan.added, vec!["NAS-03".to_string()]);
    assert_eq!(plan.pending_components(), 1);
    assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    let asset = AssetRepo::new(&fresh.conn)
        .find_by_tag("NAS-03")
        .unwrap()
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Active);
    let c = component(&fresh, "NAS-03");
    assert!(c.is_pending());
    assert_eq!(c.version_norm, None);
}

#[test]
fn known_version_cleared_in_the_spreadsheet() {
    let mut w = world();
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.6")]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inv.xlsx");
    with_version(&w, &path, "");

    let preview = import(&mut w, &path, true);
    let fw = &preview.updated[0];
    assert_eq!(fw.asset_tag, "FW-01");
    assert_eq!(fw.version_changes[0].describe(), "11.1.6 → 待補");
    assert!(preview.affects_matching());
    assert_eq!(preview.pending_components(), 1);
    assert!(!component(&w, "FW-01").is_pending(), "dry-run 不寫入");

    import(&mut w, &path, false);
    let c = component(&w, "FW-01");
    assert!(c.is_pending());
    assert_eq!(history(&w, c.id), vec!["11.1.6".to_string()]);
}

#[test]
fn pending_version_filled_leaves_no_history() {
    let mut w = world();
    w.asset("NET-HW-003", "internal", &[("synology-dsm", "")]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inv.xlsx");
    with_version(&w, &path, "7.2.2-72806");

    let plan = import(&mut w, &path, false);
    assert_eq!(
        plan.updated[0].version_changes[0].describe(),
        "待補 → 7.2.2-72806"
    );
    assert_eq!(plan.pending_components(), 0);
    let c = component(&w, "NET-HW-003");
    assert_eq!(c.version_raw, "7.2.2-72806");
    assert!(c.version_norm.is_some());
    assert!(history(&w, c.id).is_empty());
}

#[test]
fn pending_version_is_exported_blank() {
    let mut w = world();
    w.asset("NET-HW-003", "internal", &[("synology-dsm", "")]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inv.xlsx");
    export_inventory(&w.conn, &w.catalog, &path).unwrap();
    let rows = inventory_rows(&w.conn, &w.catalog).unwrap();
    assert_eq!(rows.assets[0][4], "");

    let again = import(&mut w, &path, true);
    assert!(again.added.is_empty(), "{:?}", again.added);
    assert!(again.updated.is_empty(), "{:?}", again.updated);
    assert_eq!(again.pending_components(), 1);
    assert!(component(&w, "NET-HW-003").is_pending());
}

#[test]
fn pending_unresolved_product_is_written() {
    let mut w = world();
    w.asset("NET-HW-004", "internal", &[("synology-dsm", "7.2")]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inv.xlsx");
    let mut rows = inventory_rows(&w.conn, &w.catalog).unwrap();
    rows.components.push(vec![
        "NET-HW-004".to_string(),
        "Zyxel ABCD".to_string(),
        String::new(),
    ]);
    write_rows(&path, rows);
    import(&mut w, &path, false);
    let unresolved = AssetRepo::new(&w.conn).unresolved().unwrap();
    assert_eq!(unresolved.len(), 1);
    assert!(unresolved[0].is_pending());
}
