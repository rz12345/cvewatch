//! 22 任務 3.3、3.4：清冊的生命週期日期欄與 `eol` 工作表——匯入覆寫、缺欄與缺工作表時保留、
//! 工作表中沒有的覆寫移除、無效日期與沒有元件的產品中止，以及匯出後重匯不變、範本可匯入
//! （spec：Lifecycle date fields、Lifecycle columns and EoL sheet）。

mod common;

use std::path::{Path, PathBuf};

use chrono::{NaiveDate, Utc};
use cvewatch_core::assets::columns::{ColumnMap, SheetKind, StandardField};
use cvewatch_core::assets::export::export_inventory;
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::plan::{EolTarget, ImportPlan};
use cvewatch_core::assets::sheet::read_inventory;
use cvewatch_core::assets::template::{write_sheets, write_template, SheetData};
use cvewatch_core::assets::validate::parse_inventory;
use cvewatch_core::assets::{AssetClasses, AssetRecord, AssetRepo, ComponentRepo, EolOverride};
use cvewatch_core::catalog::Catalog;
use cvewatch_core::db::open_working_db;
use cvewatch_core::CoreError;
use rusqlite::Connection;

fn run(conn: &mut Connection, path: &Path, dry_run: bool) -> Result<ImportPlan, CoreError> {
    let book = read_inventory(path, None, None)?;
    let inventory = parse_inventory(&book, &ColumnMap::default(), &AssetClasses::builtin())?;
    import_inventory(
        conn,
        &Catalog::seed().unwrap(),
        &inventory,
        ImportOptions {
            dry_run,
            prune: false,
        },
        Utc::now(),
    )
}

fn rejected(r: Result<ImportPlan, CoreError>) -> Vec<String> {
    match r {
        Err(CoreError::ImportRejected(errors)) => errors,
        other => panic!("應中止：{other:?}"),
    }
}

fn record(conn: &Connection, tag: &str) -> AssetRecord {
    AssetRepo::new(conn)
        .find_by_tag(tag)
        .unwrap()
        .unwrap_or_else(|| panic!("{tag}"))
        .record
}

fn component_eol(conn: &Connection, tag: &str) -> Vec<Option<EolOverride>> {
    let id = AssetRepo::new(conn).find_by_tag(tag).unwrap().unwrap().id;
    ComponentRepo::new(conn)
        .for_asset(id)
        .unwrap()
        .into_iter()
        .map(|c| c.eol_override)
        .collect()
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn eol(date: NaiveDate, source: &str) -> EolOverride {
    EolOverride {
        date,
        source: source.into(),
    }
}

fn col(field: StandardField) -> usize {
    SheetKind::Assets
        .columns()
        .iter()
        .position(|f| *f == field)
        .unwrap()
}

/// 一列 assets（只填必要欄位與 `fill` 指定的欄位）。
fn asset_row(
    tag: &str,
    product: &str,
    version: &str,
    fill: &[(StandardField, &str)],
) -> Vec<String> {
    let mut row = vec![String::new(); SheetKind::Assets.columns().len()];
    row[col(StandardField::AssetTag)] = tag.into();
    row[col(StandardField::Name)] = tag.to_lowercase();
    row[col(StandardField::Category)] = "HW05".into();
    row[col(StandardField::Product)] = product.into();
    row[col(StandardField::Version)] = version.into();
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

/// assets 工作表（`drop` 中的欄位不寫出）與選填的 eol 工作表。
fn workbook(
    dir: &Path,
    name: &str,
    assets: Vec<Vec<String>>,
    drop: &[StandardField],
    eol_rows: Option<&[[&str; 4]]>,
) -> PathBuf {
    let keep: Vec<usize> = SheetKind::Assets
        .columns()
        .iter()
        .enumerate()
        .filter(|(_, f)| !drop.contains(f))
        .map(|(i, _)| i)
        .collect();
    let pick = |row: &Vec<String>| keep.iter().map(|&i| row[i].clone()).collect::<Vec<_>>();
    let mut sheets = vec![SheetData {
        name: "assets",
        headers: pick(&headers(SheetKind::Assets)),
        rows: assets.iter().map(pick).collect(),
    }];
    if let Some(rows) = eol_rows {
        sheets.push(SheetData {
            name: "eol",
            headers: headers(SheetKind::Eol),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
        });
    }
    let path = dir.join(name);
    write_sheets(&path, &sheets).unwrap();
    path
}

fn base_assets(warranty: &str) -> Vec<Vec<String>> {
    vec![
        asset_row(
            "SRV-01",
            "ESXi",
            "8.0 U3c",
            &[
                (StandardField::WarrantyEnd, warranty),
                (StandardField::SupportEnd, "2027-06-30"),
            ],
        ),
        asset_row("SW-CORE", "AOS-CX", "10.13.1000", &[]),
    ]
}

const OVERRIDES: [[&str; 4]; 2] = [
    ["SW-CORE", "", "2027-06-30", "原廠延長支援函"],
    ["SRV-01", "ESXi", "2028-12-31", "延長支援合約"],
];

#[test]
fn recording_dates_and_importing_overrides() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let path = workbook(
        env.dir.path(),
        "a.xlsx",
        base_assets("2027/12/31"),
        &[],
        Some(&OVERRIDES),
    );
    let plan = run(&mut conn, &path, false).unwrap();
    assert_eq!(plan.added.len(), 2);
    let srv = record(&conn, "SRV-01");
    assert_eq!(srv.warranty_end, Some(ymd(2027, 12, 31)));
    assert_eq!(srv.support_end, Some(ymd(2027, 6, 30)));
    assert_eq!(srv.license_end, None);
    assert_eq!(srv.hw_eol_override, None);
    assert_eq!(
        component_eol(&conn, "SRV-01"),
        [Some(eol(ymd(2028, 12, 31), "延長支援合約"))]
    );
    assert_eq!(
        record(&conn, "SW-CORE").hw_eol_override,
        Some(eol(ymd(2027, 6, 30), "原廠延長支援函"))
    );
    assert!(
        cvewatch_core::matching::run::match_is_stale(&conn).unwrap(),
        "新增資產本來就需要比對"
    );
}

#[test]
fn invalid_date_aborts_without_changes() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let mut assets = base_assets("");
    assets[0][col(StandardField::SupportEnd)] = "2027-02-30".into();
    let path = workbook(env.dir.path(), "bad.xlsx", assets, &[], None);
    let errors = rejected(run(&mut conn, &path, false));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("第 2 列"), "{errors:?}");
    assert!(
        errors[0].contains("support_end 的「2027-02-30」"),
        "{errors:?}"
    );
    assert!(AssetRepo::new(&conn)
        .find_by_tag("SRV-01")
        .unwrap()
        .is_none());
}

#[test]
fn override_for_a_product_the_asset_does_not_have() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let path = workbook(
        env.dir.path(),
        "a.xlsx",
        base_assets(""),
        &[],
        Some(&[["SW-CORE", "Synology DSM", "2027-06-30", "x"]]),
    );
    let errors = rejected(run(&mut conn, &path, false));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with("eol 工作表第 2 列"), "{errors:?}");
    assert!(
        errors[0].contains("沒有 DiskStation Manager 的元件"),
        "{errors:?}"
    );
    assert!(AssetRepo::new(&conn)
        .find_by_tag("SW-CORE")
        .unwrap()
        .is_none());
}

#[test]
fn reimporting_an_inventory_without_lifecycle_data_keeps_it() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let full = workbook(
        env.dir.path(),
        "full.xlsx",
        base_assets("2027-12-31"),
        &[],
        Some(&OVERRIDES),
    );
    run(&mut conn, &full, false).unwrap();
    let old = workbook(
        env.dir.path(),
        "old.xlsx",
        base_assets(""),
        &[
            StandardField::WarrantyEnd,
            StandardField::SupportEnd,
            StandardField::LicenseEnd,
        ],
        None,
    );
    let plan = run(&mut conn, &old, false).unwrap();
    assert!(plan.updated.is_empty(), "{:?}", plan.updated);
    let srv = record(&conn, "SRV-01");
    assert_eq!(srv.warranty_end, Some(ymd(2027, 12, 31)));
    assert_eq!(
        component_eol(&conn, "SRV-01"),
        [Some(eol(ymd(2028, 12, 31), "延長支援合約"))]
    );
    assert!(record(&conn, "SW-CORE").hw_eol_override.is_some());
    let notices: Vec<String> = plan.notices.iter().map(ToString::to_string).collect();
    assert!(
        notices
            .iter()
            .any(|n| n.contains("warranty_end") && n.contains("保留")),
        "{notices:?}"
    );
    assert!(
        notices.iter().any(|n| n.contains("eol 工作表")),
        "{notices:?}"
    );
}

#[test]
fn overrides_removed_in_the_sheet() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let full = workbook(
        env.dir.path(),
        "full.xlsx",
        base_assets(""),
        &[],
        Some(&OVERRIDES),
    );
    run(&mut conn, &full, false).unwrap();
    let without_sw = workbook(
        env.dir.path(),
        "next.xlsx",
        base_assets(""),
        &[],
        Some(&OVERRIDES[1..]),
    );
    let preview = run(&mut conn, &without_sw, true).unwrap();
    assert_eq!(preview.updated.len(), 1);
    let update = &preview.updated[0];
    assert_eq!(update.asset_tag, "SW-CORE");
    assert!(update.changed_fields.is_empty());
    assert_eq!(update.eol_changes.len(), 1);
    assert_eq!(update.eol_changes[0].target, EolTarget::Hardware);
    assert!(
        update.eol_changes[0].describe().contains("將移除"),
        "{}",
        update.eol_changes[0].describe()
    );
    assert!(!preview.affects_matching(), "覆寫不影響比對");
    assert!(
        record(&conn, "SW-CORE").hw_eol_override.is_some(),
        "dry-run 不寫入"
    );
    run(&mut conn, &without_sw, false).unwrap();
    assert_eq!(record(&conn, "SW-CORE").hw_eol_override, None);
    assert!(component_eol(&conn, "SRV-01")[0].is_some());
}

#[test]
fn round_trip_with_lifecycle_data() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let mut assets = base_assets("2027-12-31");
    assets[1][col(StandardField::LicenseEnd)] = "2026-12-31".into();
    let path = workbook(env.dir.path(), "a.xlsx", assets, &[], Some(&OVERRIDES));
    run(&mut conn, &path, false).unwrap();
    let out = env.dir.path().join("export.xlsx");
    export_inventory(&conn, &Catalog::seed().unwrap(), &out).unwrap();
    let book = read_inventory(&out, None, None).unwrap();
    let eol_sheet = book.eol.as_ref().expect("匯出含 eol 工作表");
    assert_eq!(eol_sheet.rows.len(), 2);
    let plan = run(&mut conn, &out, true).unwrap();
    assert!(plan.added.is_empty());
    assert!(plan.updated.is_empty(), "{:?}", plan.updated);
}

#[test]
fn template_has_an_eol_sheet_and_imports_cleanly() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let path = env.dir.path().join("template.xlsx");
    write_template(&path, &AssetClasses::builtin()).unwrap();
    let book = read_inventory(&path, None, None).unwrap();
    assert_eq!(
        book.eol.as_ref().unwrap().headers,
        ["asset_tag", "product", "eol", "source"]
    );
    for h in ["warranty_end", "support_end", "license_end"] {
        assert!(book.assets.headers.iter().any(|x| x == h), "{h}");
    }
    let plan = run(&mut conn, &path, true).unwrap();
    assert_eq!(plan.added.len(), 3);
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

/// 任務 3.1、3.6：表頭解析、`EOL` 工作表名稱不分大小寫、Excel 日期儲存格，CSV 清冊沒有 eol。
#[test]
fn fixture_with_date_cell_and_uppercase_eol_sheet() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let path = fixture("inventory_lifecycle.xlsx");
    let book = read_inventory(&path, None, None).unwrap();
    assert_eq!(book.eol.as_ref().unwrap().rows.len(), 2);
    let cols = ColumnMap::default()
        .locate(&book.assets.headers, SheetKind::Assets)
        .unwrap();
    for f in [
        StandardField::WarrantyEnd,
        StandardField::SupportEnd,
        StandardField::LicenseEnd,
    ] {
        assert!(cols.contains_key(&f), "{f:?}");
    }
    run(&mut conn, &path, false).unwrap();
    let srv = record(&conn, "SRV-01");
    assert_eq!(
        srv.warranty_end,
        Some(ymd(2027, 12, 31)),
        "Excel 日期儲存格"
    );
    assert_eq!(srv.support_end, Some(ymd(2027, 6, 30)));
    let sw = record(&conn, "SW-CORE");
    assert_eq!(sw.warranty_end, Some(ymd(2026, 12, 31)));
    assert_eq!(sw.hw_eol_override.unwrap().source, "原廠延長支援函");

    let csv = read_inventory(&fixture("inventory_basic.csv"), None, None).unwrap();
    assert!(csv.eol.is_none());
}
