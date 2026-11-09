//! 20 任務 3.3、3.4、3.6：清冊的識別欄位、`ips` 工作表與 CSV 的 `--ips` 檔、財產編號的
//! 重複與互換、缺欄保留，以及匯出後重匯不變（spec：Inventory ips sheet、Unique property
//! number、Inventory without identity columns、Inventory export）。

mod common;

use std::path::{Path, PathBuf};

use chrono::Utc;
use cvewatch_core::assets::columns::{ColumnMap, SheetKind, StandardField};
use cvewatch_core::assets::export::{export_inventory, inventory_rows};
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::plan::{ImportPlan, IPS_KEPT_NOTICE};
use cvewatch_core::assets::sheet::read_inventory;
use cvewatch_core::assets::template::{write_sheets, SheetData};
use cvewatch_core::assets::validate::parse_inventory;
use cvewatch_core::assets::{AssetClasses, AssetIp, AssetIpRepo, AssetRecord, AssetRepo};
use cvewatch_core::catalog::Catalog;
use cvewatch_core::db::open_working_db;
use cvewatch_core::CoreError;
use rusqlite::Connection;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn import_book(
    conn: &mut Connection,
    path: &Path,
    ips_csv: Option<&Path>,
    dry_run: bool,
) -> Result<ImportPlan, CoreError> {
    let book = read_inventory(path, None, ips_csv)?;
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

fn run(conn: &mut Connection, path: &Path, dry_run: bool) -> Result<ImportPlan, CoreError> {
    import_book(conn, path, None, dry_run)
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

fn asset_id(conn: &Connection, tag: &str) -> i64 {
    AssetRepo::new(conn).find_by_tag(tag).unwrap().unwrap().id
}

fn ips(conn: &Connection, tag: &str) -> Vec<String> {
    AssetIpRepo::new(conn)
        .for_asset(asset_id(conn, tag))
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn set_ips(conn: &Connection, tag: &str, addresses: &[&str]) {
    let list: Vec<AssetIp> = addresses
        .iter()
        .map(|a| AssetIp {
            address: a.parse().unwrap(),
            purpose: None,
        })
        .collect();
    AssetIpRepo::new(conn)
        .replace(asset_id(conn, tag), &list)
        .unwrap();
}

/// 欄位在 `assets` 工作表的位置。
fn col(field: StandardField) -> usize {
    SheetKind::Assets
        .columns()
        .iter()
        .position(|f| *f == field)
        .unwrap()
}

/// 標準欄名的一列 assets（只填必要欄位與 `fill` 指定的欄位）。
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

fn workbook(
    dir: &Path,
    name: &str,
    assets: Vec<Vec<String>>,
    ips: Option<&[[&str; 3]]>,
) -> PathBuf {
    let path = dir.join(name);
    let mut sheets = vec![SheetData {
        name: "assets",
        headers: headers(SheetKind::Assets),
        rows: assets,
    }];
    if let Some(rows) = ips {
        sheets.push(SheetData {
            name: "ips",
            headers: headers(SheetKind::Ips),
            rows: rows.iter().map(|r| strings(r)).collect(),
        });
    }
    write_sheets(&path, &sheets).unwrap();
    path
}

/// Scenario: Recording identity fields / Importing addresses
#[test]
fn importing_identity_fields_and_addresses() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    run(&mut conn, &fixture("inventory_basic.xlsx"), false).unwrap();
    assert!(ips(&conn, "SRV-01").is_empty());

    let preview = run(&mut conn, &fixture("inventory_identity.xlsx"), true).unwrap();
    let srv = preview
        .updated
        .iter()
        .find(|u| u.asset_tag == "SRV-01")
        .expect("SRV-01 列為更新");
    assert!(srv.field_names().contains(&"ip"), "{srv:?}");
    assert_eq!(
        srv.ip_change.as_ref().unwrap().describe(),
        "- → 192.0.2.10（管理）、2001:db8::10"
    );
    assert!(ips(&conn, "SRV-01").is_empty(), "--dry-run 不寫入");

    run(&mut conn, &fixture("inventory_identity.xlsx"), false).unwrap();
    assert_eq!(ips(&conn, "SRV-01"), ["192.0.2.10（管理）", "2001:db8::10"]);
    let fw = record(&conn, "FW-EDGE");
    assert_eq!(fw.brand.as_deref(), Some("Palo Alto Networks"));
    assert_eq!(fw.model.as_deref(), Some("XYZ-100"));
    assert_eq!(fw.serial_number.as_deref(), Some("SN0001"));
    assert_eq!(fw.hostname.as_deref(), Some("fw-edge"));
    assert_eq!(fw.management_url.as_deref(), Some("https://192.0.2.1/"));
    assert_eq!(ips(&conn, "FW-EDGE"), ["192.0.2.1（管理）"]);

    // IP 沒有變更時不列為更新
    let again = run(&mut conn, &fixture("inventory_identity.xlsx"), true).unwrap();
    assert!(again.updated.is_empty(), "{:?}", again.updated);
}

/// Scenario: Asset left out of the ips sheet
#[test]
fn asset_left_out_of_the_ips_sheet() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    run(&mut conn, &fixture("inventory_basic.xlsx"), false).unwrap();
    set_ips(&conn, "NAS-01", &["192.0.2.20"]);
    set_ips(&conn, "NAS-02", &["192.0.2.21"]);
    let plan = run(&mut conn, &fixture("inventory_identity.xlsx"), false).unwrap();
    assert!(
        ips(&conn, "NAS-01").is_empty(),
        "清冊中的資產以 ips 工作表為準"
    );
    assert_eq!(ips(&conn, "NAS-02"), ["192.0.2.21"], "清冊以外的資產不變");
    let nas = plan
        .updated
        .iter()
        .find(|u| u.asset_tag == "NAS-01")
        .unwrap();
    assert_eq!(nas.ip_change.as_ref().unwrap().describe(), "192.0.2.20 → -");
}

/// Scenario: Re-importing an inventory from before this version / Inventory without an ips sheet
#[test]
fn older_inventory_keeps_identity_and_addresses() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    run(&mut conn, &fixture("inventory_basic.xlsx"), false).unwrap();
    conn.execute(
        "UPDATE asset SET brand = 'Aruba', property_number = 'P-0001' WHERE asset_tag = 'FW-01'",
        [],
    )
    .unwrap();
    set_ips(&conn, "SRV-01", &["192.0.2.10"]);
    let plan = run(&mut conn, &fixture("inventory_basic.xlsx"), false).unwrap();
    let notices: Vec<String> = plan.notices.iter().map(|n| n.to_string()).collect();
    assert_eq!(
        notices,
        [
            "清冊沒有 brand、model、serial_number、property_number、hostname、management_url 欄，\
             保留資料庫中既有資產的值",
            IPS_KEPT_NOTICE,
        ]
    );
    let fw = record(&conn, "FW-01");
    assert_eq!(fw.brand.as_deref(), Some("Aruba"));
    assert_eq!(fw.property_number.as_deref(), Some("P-0001"));
    assert_eq!(ips(&conn, "SRV-01"), ["192.0.2.10"]);
    assert!(plan.updated.is_empty(), "{:?}", plan.updated);
}

/// Scenario: Duplicate property number in the sheet
#[test]
fn duplicate_property_number_in_the_sheet() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let p = |n| [(StandardField::PropertyNumber, n)];
    let path = workbook(
        env.dir.path(),
        "dup.xlsx",
        vec![
            asset_row("NAS-01", "DSM", "7.2", &[]),
            asset_row("NAS-02", "DSM", "7.2", &p("P-0001")),
            asset_row("NAS-03", "DSM", "7.2", &[]),
            asset_row("NAS-04", "DSM", "7.2", &p("p-0001")),
        ],
        None,
    );
    let errors = rejected(run(&mut conn, &path, false));
    assert_eq!(errors, ["第 5 列：財產編號「p-0001」與第 3 列重複"]);
    let n: i64 = conn
        .query_row("SELECT count(*) FROM asset", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "資料庫不變");
}

/// Scenario: Property number used by an asset outside the inventory / URL in the hostname field
#[test]
fn property_number_of_another_asset_and_bad_hostname() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    run(&mut conn, &fixture("inventory_basic.xlsx"), false).unwrap();
    conn.execute(
        "UPDATE asset SET property_number = 'P-0009', status = 'retired' WHERE asset_tag = 'NAS-02'",
        [],
    )
    .unwrap();
    let path = workbook(
        env.dir.path(),
        "conflict.xlsx",
        vec![
            asset_row(
                "NAS-01",
                "DSM",
                "7.2",
                &[(StandardField::PropertyNumber, "p-0009")],
            ),
            asset_row("NAS-05", "DSM", "7.2", &[]),
        ],
        None,
    );
    let errors = rejected(run(&mut conn, &path, false));
    assert_eq!(
        errors,
        ["第 2 列：財產編號 p-0009 已被 NAS-02 使用（該資產已退役）"]
    );
    assert_eq!(record(&conn, "NAS-01").property_number, None, "資料庫不變");
    let path = workbook(
        env.dir.path(),
        "hostname.xlsx",
        vec![
            asset_row("NAS-01", "DSM", "7.2", &[]),
            asset_row("NAS-05", "DSM", "7.2", &[]),
            asset_row(
                "NAS-06",
                "DSM",
                "7.2",
                &[(StandardField::Hostname, "https://fw-edge")],
            ),
        ],
        None,
    );
    let errors = rejected(run(&mut conn, &path, false));
    assert_eq!(
        errors,
        ["第 4 列：hostname「https://fw-edge」只能包含英數字、-、. 與 _"]
    );
}

/// Scenario: Swapping property numbers by import
#[test]
fn swapping_property_numbers_by_import() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    run(&mut conn, &fixture("inventory_identity.xlsx"), false).unwrap();
    assert_eq!(
        record(&conn, "FW-EDGE").property_number.as_deref(),
        Some("P-0001")
    );
    let catalog = Catalog::seed().unwrap();
    let mut rows = inventory_rows(&conn, &catalog).unwrap();
    let at = col(StandardField::PropertyNumber);
    for row in &mut rows.assets {
        row[at] = match row[at].as_str() {
            "P-0001" => "P-0002".into(),
            "P-0002" => "P-0001".into(),
            other => other.into(),
        };
    }
    let path = env.dir.path().join("swap.xlsx");
    write_sheets(
        &path,
        &[SheetData {
            name: "assets",
            headers: headers(SheetKind::Assets),
            rows: rows.assets,
        }],
    )
    .unwrap();
    run(&mut conn, &path, false).unwrap();
    assert_eq!(
        record(&conn, "FW-EDGE").property_number.as_deref(),
        Some("P-0002")
    );
    assert_eq!(
        record(&conn, "SRV-01").property_number.as_deref(),
        Some("P-0001")
    );
}

/// Scenario: CSV inventory with an IP file
#[test]
fn csv_inventory_with_an_ip_file() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    import_book(
        &mut conn,
        &fixture("inventory_basic.csv"),
        Some(&fixture("inventory_ips.csv")),
        false,
    )
    .unwrap();
    assert_eq!(ips(&conn, "SRV-01"), ["192.0.2.10"]);
    let err = read_inventory(
        &fixture("inventory_basic.xlsx"),
        None,
        Some(&fixture("inventory_ips.csv")),
    )
    .unwrap_err();
    assert!(err.to_string().contains("--ips 只用於 CSV"), "{err}");
}

/// Scenario: Asset not in the assets sheet（資料庫不變）
#[test]
fn ips_row_for_an_unknown_asset_aborts() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let path = workbook(
        env.dir.path(),
        "unknown.xlsx",
        vec![asset_row("SRV-01", "DSM", "7.2", &[])],
        Some(&[
            ["SRV-01", "192.0.2.10", ""],
            ["SRV-01", "192.0.2.11", ""],
            ["SRV-99", "192.0.2.12", ""],
        ]),
    );
    let errors = rejected(run(&mut conn, &path, false));
    assert_eq!(
        errors,
        ["ips 工作表第 4 列：asset_tag「SRV-99」不在 assets 工作表中"]
    );
    let n: i64 = conn
        .query_row("SELECT count(*) FROM asset", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

/// Scenario: Addresses are exported / Round trip without changes
#[test]
fn addresses_are_exported_and_round_trip() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    run(&mut conn, &fixture("inventory_identity.xlsx"), false).unwrap();
    let catalog = Catalog::seed().unwrap();
    let rows = inventory_rows(&conn, &catalog).unwrap();
    assert_eq!(
        rows.ips,
        vec![
            strings(&["FW-EDGE", "192.0.2.1", "管理"]),
            strings(&["SRV-01", "192.0.2.10", "管理"]),
            strings(&["SRV-01", "2001:db8::10", ""]),
        ],
        "依資產編號與儲存順序；NAS-01 沒有 IP 不列出"
    );
    let fw = rows.assets.iter().find(|r| r[0] == "FW-EDGE").unwrap();
    assert_eq!(fw[col(StandardField::ManagementUrl)], "https://192.0.2.1/");

    let path = env.dir.path().join("out.xlsx");
    export_inventory(&conn, &catalog, &path).unwrap();
    let again = run(&mut conn, &path, true).unwrap();
    assert!(
        again.added.is_empty() && again.updated.is_empty(),
        "{again:?}"
    );
    assert!(again.notices.is_empty(), "{:?}", again.notices);
}
