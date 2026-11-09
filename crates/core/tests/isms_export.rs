//! 23 任務 3.1～3.3：ISMS 匯出後以 ISMS 匯入讀回（空資料庫、同一資料庫），以及匯出檔不含
//! 主機名稱與 IP（spec：Exported file is read as an ISMS workbook、Importing into an empty
//! database、Re-importing into the same database、Host name and IP are not exported）。

mod common;

use std::path::{Path, PathBuf};

use calamine::{open_workbook, Reader, Xlsx};
use chrono::{NaiveDate, Utc};
use common::matching::{world_with, World};
use cvewatch_core::assets::columns::{ColumnMap, SheetKind, StandardField};
use cvewatch_core::assets::export::InventoryItem;
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::isms::{
    build_conversion, export_isms, import_isms, read_isms_workbook, ConversionContext,
    IsmsExportSummary, ISMS_V21_HEADERS,
};
use cvewatch_core::assets::sheet::{cell_text, read_inventory};
use cvewatch_core::assets::template::{write_sheets, SheetData};
use cvewatch_core::assets::validate::parse_inventory;
use cvewatch_core::assets::AssetRepo;
use cvewatch_core::catalog::normalize;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn isms_world() -> World {
    let aliases = std::fs::read_to_string(fixture("isms_catalog.d/isms-test.toml")).unwrap();
    world_with(Some(&aliases))
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn col(field: StandardField) -> usize {
    SheetKind::Assets
        .columns()
        .iter()
        .position(|f| *f == field)
        .unwrap()
}

fn asset_row(tag: &str, category: &str, fill: &[(StandardField, &str)]) -> Vec<String> {
    let mut row = vec![String::new(); SheetKind::Assets.columns().len()];
    row[col(StandardField::AssetTag)] = tag.into();
    row[col(StandardField::Name)] = tag.to_lowercase();
    row[col(StandardField::Category)] = category.into();
    for (f, v) in fill {
        row[col(*f)] = v.to_string();
    }
    row
}

fn headers(kind: SheetKind) -> Vec<String> {
    kind.columns()
        .iter()
        .map(|f| f.as_str().to_string())
        .collect()
}

/// 虛構資產：FW-EDGE（廠牌型號＋AOS-CX）、NAS-01（版本待補的 DSM、未辨識產品、主機名稱與 IP）、
/// SRV-01（沒有 ISMS 編號）、OLD-01（之後退役）。
fn inventory(dir: &Path) -> PathBuf {
    use StandardField::*;
    let assets = vec![
        asset_row(
            "FW-EDGE",
            "HW02",
            &[
                (Product, "AOS-CX"),
                (Version, "1.2.3"),
                (IsmsId, "NET-HW-001"),
                (Custodian, "王小明"),
                (Brand, "Aruba"),
                (Model, "XYZ-100"),
            ],
        ),
        asset_row(
            "NAS-01",
            "HW01",
            &[
                (Product, "DiskStation Manager"),
                (IsmsId, "NET-HW-002"),
                (Hostname, "srv01"),
            ],
        ),
        asset_row("SRV-01", "HW01", &[(Product, "ESXi"), (Version, "8.0")]),
        asset_row(
            "OLD-01",
            "HW01",
            &[(Product, "ESXi"), (Version, "7.0"), (IsmsId, "NET-HW-005")],
        ),
    ];
    let sheets = [
        SheetData {
            name: "assets",
            headers: headers(SheetKind::Assets),
            rows: assets,
        },
        SheetData {
            name: "components",
            headers: headers(SheetKind::Components),
            rows: vec![strings(&["NAS-01", "Zyxel ABCD", "2.0"])],
        },
        SheetData {
            name: "systems",
            headers: headers(SheetKind::Systems),
            rows: vec![strings(&["NET", "網路服務"])],
        },
        SheetData {
            name: "ips",
            headers: headers(SheetKind::Ips),
            rows: vec![strings(&["NAS-01", "192.0.2.10", ""])],
        },
    ];
    let path = dir.join("inventory.xlsx");
    write_sheets(&path, &sheets).unwrap();
    path
}

/// 以一般清冊匯入建立資產並退役 OLD-01。
fn seeded(dir: &Path) -> World {
    let mut w = isms_world();
    let book = read_inventory(&inventory(dir), None, None).unwrap();
    let inv = parse_inventory(&book, &ColumnMap::default(), &w.classes).unwrap();
    let options = ImportOptions {
        dry_run: false,
        prune: false,
    };
    import_inventory(&mut w.conn, &w.catalog, &inv, options, Utc::now()).unwrap();
    let repo = AssetRepo::new(&w.conn);
    let old = repo.find_by_tag("OLD-01").unwrap().unwrap();
    repo.retire(old.id, Utc::now()).unwrap();
    w
}

fn export(w: &World, path: &Path) -> IsmsExportSummary {
    let day = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
    export_isms(&w.conn, &w.classes, &w.catalog, path, day).unwrap()
}

fn keys(items: &[InventoryItem]) -> Vec<String> {
    items.iter().filter_map(|i| i.key.clone()).collect()
}

fn unresolved(items: &[InventoryItem]) -> Vec<String> {
    items
        .iter()
        .filter(|i| i.key.is_none())
        .map(|i| normalize(&i.product))
        .collect()
}

/// Scenario: Exported file is read as an ISMS workbook / Importing into an empty database
#[test]
fn importing_into_an_empty_database() {
    let dir = tempfile::tempdir().unwrap();
    let w = seeded(dir.path());
    let path = dir.path().join("isms-out.xlsx");
    let summary = export(&w, &path);
    assert_eq!(summary.exported, 2);
    assert_eq!(summary.skipped, ["SRV-01"]);
    assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);

    let book = read_isms_workbook(&path).unwrap();
    assert_eq!(book.header_row, 4);
    let rows: Vec<(usize, String)> = book
        .rows
        .iter()
        .map(|r| (r.number, format!("{}{}", r.system_id, r.serial)))
        .collect();
    assert_eq!(
        rows,
        [(5, "NET-HW-001".to_string()), (6, "NET-HW-002".to_string())]
    );

    let empty = ConversionContext::empty(&w.classes, &w.catalog);
    let conversion = build_conversion(&book, &empty);
    let originals = ConversionContext::load(&w.conn, &w.classes, &w.catalog).unwrap();
    assert_eq!(conversion.assets.len(), 2);
    for converted in &conversion.assets {
        let tag = converted.record.asset_tag.as_str();
        let original = originals
            .existing
            .iter()
            .find(|a| a.record.isms_id.as_deref() == Some(tag))
            .unwrap_or_else(|| panic!("{tag}"));
        assert_eq!(keys(&converted.items), keys(&original.items), "{tag}");
        assert_eq!(
            unresolved(&converted.items),
            unresolved(&original.items),
            "{tag}"
        );
        assert!(
            converted.items.iter().all(|i| i.version.is_empty()),
            "版本不讀回：{:?}",
            converted.items
        );
    }
    let fw = &conversion.assets[0];
    assert_eq!(keys(&fw.items), ["hpe-aoscx"]);
    assert_eq!(fw.record.name, "AOS-CX", "名稱取第一個元件行");
    let report = &conversion.report;
    assert!(report.merged.is_empty(), "{:?}", report.merged);
    let lines: Vec<&str> = report
        .unresolved_lines
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(lines, ["Zyxel ABCD"], "沒有來自廠牌型號行的未辨識描述行");
}

/// Scenario: Re-importing into the same database
#[test]
fn re_importing_into_the_same_database() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = seeded(dir.path());
    let path = dir.path().join("isms-out.xlsx");
    export(&w, &path);
    let snapshot = AssetRepo::new(&w.conn)
        .list(&cvewatch_core::assets::AssetFilter {
            category: None,
            include_retired: true,
        })
        .unwrap();
    let result = import_isms(&mut w.conn, &w.classes, &w.catalog, &path, true, Utc::now()).unwrap();
    assert!(result.plan.added.is_empty(), "{:?}", result.plan.added);
    assert!(result.plan.updated.is_empty(), "{:?}", result.plan.updated);
    assert_eq!(result.report.pending_components, 0);
    assert_eq!(result.report.not_in_workbook, ["SRV-01"]);
    let after = AssetRepo::new(&w.conn)
        .list(&cvewatch_core::assets::AssetFilter {
            category: None,
            include_retired: true,
        })
        .unwrap();
    assert_eq!(after, snapshot, "資料庫不變");
}

/// Scenario: Host name and IP are not exported
#[test]
fn host_name_and_ip_are_not_exported() {
    let dir = tempfile::tempdir().unwrap();
    let w = seeded(dir.path());
    let path = dir.path().join("isms-out.xlsx");
    export(&w, &path);
    let mut book: Xlsx<_> = open_workbook(&path).unwrap();
    let names = book.sheet_names();
    assert_eq!(names.len(), 1);
    let range = book.worksheet_range(&names[0]).unwrap();
    let cells: Vec<String> = range.cells().map(|(_, _, c)| cell_text(c)).collect();
    assert!(cells.iter().any(|c| c == ISMS_V21_HEADERS[5]), "有表頭");
    assert!(cells.iter().any(|c| c.contains("DiskStation Manager")));
    for secret in ["srv01", "192.0.2.10", "NET-HW-005", "OLD-01", "SRV-01"] {
        assert!(
            cells.iter().all(|c| !c.contains(secret)),
            "{secret} 不應出現在匯出檔"
        );
    }
}
