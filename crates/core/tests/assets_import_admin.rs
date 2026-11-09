//! 16 任務 3.2～3.4：清冊的 `systems` 工作表、ISMS 管理欄位、缺欄保留既有值、`owner` 舊欄名，
//! 以及範本與匯出的 `systems` 工作表（spec：Inventory systems sheet、Inventory without new
//! columns、Legacy owner column、Blank template export、Inventory export）。

mod common;

use std::path::{Path, PathBuf};

use chrono::Utc;
use cvewatch_core::assets::columns::ColumnMap;
use cvewatch_core::assets::export::{export_inventory, inventory_rows};
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::plan::ImportPlan;
use cvewatch_core::assets::sheet::read_inventory;
use cvewatch_core::assets::systems::{create_system, SystemDirectory};
use cvewatch_core::assets::template::{write_sheets, write_template, SheetData};
use cvewatch_core::assets::validate::{parse_inventory, OWNER_NOTICE};
use cvewatch_core::assets::{AssetClasses, AssetRecord, AssetRepo};
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

/// 寫出含 assets（與選填 systems）工作表的清冊。
fn workbook(dir: &Path, name: &str, assets: &[&[&str]], systems: Option<&[[&str; 2]]>) -> PathBuf {
    let path = dir.join(name);
    let mut sheets = vec![SheetData {
        name: "assets",
        headers: strings(assets[0]),
        rows: assets[1..].iter().map(|r| strings(r)).collect(),
    }];
    if let Some(rows) = systems {
        sheets.push(SheetData {
            name: "systems",
            headers: strings(&["code", "name"]),
            rows: rows.iter().map(|r| strings(r)).collect(),
        });
    }
    write_sheets(&path, &sheets).unwrap();
    path
}

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
        .expect(tag)
        .record
}

const HEAD: [&str; 6] = [
    "asset_tag",
    "name",
    "category",
    "product",
    "version",
    "system",
];

/// Scenario: Importing a new system with its assets
#[test]
fn importing_a_new_system_with_its_assets() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let path = workbook(
        env.dir.path(),
        "a.xlsx",
        &[
            &HEAD,
            &["FW-01", "fw01", "HW03", "PAN-OS", "11.1.4-h7", "NET"],
            &["SW-CORE", "core", "HW02", "PAN-OS", "11.1.4-h7", "網路服務"],
        ],
        Some(&[["net", "網路服務"]]),
    );
    let plan = run(&mut conn, &path, false).unwrap();
    assert_eq!(
        plan.systems
            .iter()
            .map(|c| c.describe())
            .collect::<Vec<_>>(),
        vec!["新增資產說明 NET 網路服務"]
    );
    let dir = SystemDirectory::load(&conn).unwrap();
    assert_eq!(dir.label("NET"), "NET 網路服務");
    for tag in ["FW-01", "SW-CORE"] {
        assert_eq!(
            record(&conn, tag).system_code.as_deref(),
            Some("NET"),
            "{tag}"
        );
    }
}

/// Scenario: Renaming a system by import（含 dry-run 預覽）與名稱互換
#[test]
fn renaming_a_system_by_import() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    create_system(&conn, "NET", "網路服務").unwrap();
    create_system(&conn, "SYS", "核心系統").unwrap();
    let assets: &[&[&str]] = &[
        &HEAD,
        &["FW-01", "fw01", "HW03", "PAN-OS", "11.1.4-h7", "NET"],
    ];
    let path = workbook(
        env.dir.path(),
        "a.xlsx",
        assets,
        Some(&[["NET", "校園網路服務"]]),
    );
    let preview = run(&mut conn, &path, true).unwrap();
    assert_eq!(
        preview.systems[0].describe(),
        "資產說明 NET 名稱：網路服務 → 校園網路服務"
    );
    assert_eq!(
        SystemDirectory::load(&conn).unwrap().label("NET"),
        "NET 網路服務",
        "dry-run 不寫入"
    );
    run(&mut conn, &path, false).unwrap();
    assert_eq!(
        SystemDirectory::load(&conn).unwrap().label("NET"),
        "NET 校園網路服務"
    );
    // 兩個資產說明互換名稱：寫入不違反名稱的唯一索引
    let swap = workbook(
        env.dir.path(),
        "b.xlsx",
        assets,
        Some(&[["NET", "核心系統"], ["SYS", "校園網路服務"]]),
    );
    run(&mut conn, &swap, false).unwrap();
    let dir = SystemDirectory::load(&conn).unwrap();
    assert_eq!(dir.label("NET"), "NET 核心系統");
    assert_eq!(dir.label("SYS"), "SYS 校園網路服務");
}

/// Scenario: Duplicate name in the systems sheet / 與資料庫中其他代碼的名稱衝突
#[test]
fn systems_sheet_errors_abort_the_import() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    create_system(&conn, "SYS", "核心系統").unwrap();
    let assets: &[&[&str]] = &[&HEAD, &["FW-01", "fw01", "HW03", "PAN-OS", "11.1.4-h7", ""]];
    let dup = workbook(
        env.dir.path(),
        "a.xlsx",
        assets,
        Some(&[["NET", "網路服務"], ["LAB", "實驗室"], ["WEB", "網路服務"]]),
    );
    assert_eq!(
        rejected(run(&mut conn, &dup, false)),
        vec!["systems 工作表第 4 列：名稱「網路服務」與第 2 列重複"]
    );
    let taken = workbook(
        env.dir.path(),
        "b.xlsx",
        assets,
        Some(&[["NET", "核心系統"]]),
    );
    assert_eq!(
        rejected(run(&mut conn, &taken, false)),
        vec!["systems 工作表第 2 列：名稱「核心系統」已被資產說明 SYS 使用"]
    );
    assert!(AssetRepo::new(&conn)
        .find_by_tag("FW-01")
        .unwrap()
        .is_none());
    assert_eq!(SystemDirectory::load(&conn).unwrap().all().len(), 1);
}

/// Scenario: Unknown system、ISMS ID from another system、管理欄位的列錯誤。
/// 欄位格式錯誤在讀取清冊時回報；資產說明須對照資料庫，在規劃階段回報（16 design 的 apply 決定）。
#[test]
fn row_errors_for_systems_and_admin_fields() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    create_system(&conn, "NET", "網路服務").unwrap();
    let head: &[&str] = &[
        "asset_tag",
        "name",
        "category",
        "product",
        "version",
        "system",
        "isms_id",
        "compliance",
        "personal_data",
    ];
    let fields = workbook(
        env.dir.path(),
        "a.xlsx",
        &[
            head,
            &["FW-03", "fw03", "HW03", "PAN-OS", "1", "", "", "5", ""],
            &["FW-04", "fw04", "HW03", "PAN-OS", "1", "", "", "", "maybe"],
        ],
        None,
    );
    assert_eq!(
        rejected(run(&mut conn, &fields, false)),
        vec![
            "第 2 列：法規遵循性「5」必須是 1–4",
            "第 3 列：是否含個人資料「maybe」只能填 Y、N、是或否",
        ]
    );
    let systems = workbook(
        env.dir.path(),
        "b.xlsx",
        &[
            head,
            &["FW-01", "fw01", "HW03", "PAN-OS", "1", "LAB", "", "", ""],
            &[
                "FW-02",
                "fw02",
                "HW03",
                "PAN-OS",
                "1",
                "NET",
                "SYS-HW-001",
                "",
                "",
            ],
        ],
        None,
    );
    assert_eq!(
        rejected(run(&mut conn, &systems, false)),
        vec![
            "第 2 列：資產說明「LAB」尚未定義",
            "第 3 列：ISMS 編號「SYS-HW-001」的系統代碼 SYS 與資產說明 NET 不符",
        ]
    );
    assert!(AssetRepo::new(&conn)
        .find_by_tag("FW-01")
        .unwrap()
        .is_none());
}

/// Scenario: Recording the custodian and units / Default from the class / 往返不變
#[test]
fn admin_fields_round_trip_through_export() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let first = run(&mut conn, &fixture("inventory_basic.xlsx"), false).unwrap();
    assert_eq!(
        first
            .systems
            .iter()
            .map(|c| c.describe())
            .collect::<Vec<_>>(),
        vec!["新增資產說明 SYS 核心系統"]
    );
    let fw = record(&conn, "FW-01");
    assert_eq!(fw.system_code.as_deref(), Some("SYS"), "由 ISMS 編號帶出");
    assert_eq!(fw.responsible_unit.as_deref(), Some("網管組"));
    assert_eq!(fw.custodian.as_deref(), Some("王小明"));
    assert!(!fw.personal_data);
    let srv = record(&conn, "SRV-01");
    assert_eq!(srv.system_code.as_deref(), Some("SYS"));
    assert_eq!(srv.compliance, Some(3));
    assert_eq!(srv.protection_level(), Some(4), "MAX(2/3/4 範本值與 3)");
    create_system(&conn, "LAB", "實驗室").unwrap();

    let rows = inventory_rows(&conn, &Catalog::seed().unwrap()).unwrap();
    assert_eq!(
        rows.systems,
        vec![strings(&["LAB", "實驗室"]), strings(&["SYS", "核心系統"])],
        "所有資產說明（含沒有資產者）"
    );
    let fw_row = rows.assets.iter().find(|r| r[0] == "FW-01").unwrap();
    assert_eq!(
        fw_row[10..16],
        strings(&["SYS", "網管組", "王小明", "網管組", "", "N"])
    );

    let path = env.dir.path().join("out.xlsx");
    export_inventory(&conn, &Catalog::seed().unwrap(), &path).unwrap();
    let again = run(&mut conn, &path, true).unwrap();
    assert!(
        again.added.is_empty() && again.updated.is_empty(),
        "{again:?}"
    );
    assert!(again.systems.is_empty(), "{:?}", again.systems);
    assert!(again.notices.is_empty(), "{:?}", again.notices);
}

/// Scenario: Re-importing an older inventory / Importing an owner column
#[test]
fn older_inventory_keeps_new_fields_and_reads_owner() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    run(&mut conn, &fixture("inventory_basic.xlsx"), false).unwrap();
    let plan = run(&mut conn, &fixture("inventory_owner.xlsx"), false).unwrap();
    let notices: Vec<String> = plan.notices.iter().map(|n| n.to_string()).collect();
    assert_eq!(
        notices,
        vec![
            OWNER_NOTICE.to_string(),
            "清冊沒有 system、responsible_unit、user_unit、compliance、personal_data、brand、\
             model、serial_number、property_number、hostname、management_url、warranty_end、\
             support_end、license_end 欄，保留資料庫中既有資產的值"
                .to_string(),
        ]
    );
    let fw = record(&conn, "FW-01");
    assert_eq!(
        fw.custodian.as_deref(),
        Some("王小明"),
        "owner 欄匯入為保管人"
    );
    assert_eq!(fw.system_code.as_deref(), Some("SYS"), "資產說明不變");
    assert_eq!(
        fw.responsible_unit.as_deref(),
        Some("網管組"),
        "權責單位不變"
    );
    let nas = record(&conn, "NAS-01");
    assert_eq!(nas.custodian.as_deref(), Some("李小華"));
}

/// Scenario: Both owner and custodian columns
#[test]
fn owner_and_custodian_columns_together_are_rejected() {
    let env = common::initialized();
    let path = workbook(
        env.dir.path(),
        "a.xlsx",
        &[
            &[
                "asset_tag",
                "name",
                "category",
                "product",
                "version",
                "owner",
                "custodian",
            ],
            &["FW-01", "fw01", "HW03", "PAN-OS", "1", "a", "b"],
        ],
        None,
    );
    let book = read_inventory(&path, None, None).unwrap();
    let err = parse_inventory(&book, &ColumnMap::default(), &AssetClasses::builtin())
        .unwrap_err()
        .to_string();
    assert!(err.contains("owner 與 custodian 欄只能保留一欄"), "{err}");
}

/// Scenario: Generating a template（四個工作表，範本可直接匯入）
#[test]
fn template_has_systems_sheet_and_imports() {
    use calamine::{open_workbook_auto, Reader};
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let path = env.dir.path().join("t.xlsx");
    write_template(&path, &AssetClasses::builtin()).unwrap();
    let mut book = open_workbook_auto(&path).unwrap();
    let range = book.worksheet_range("systems").unwrap();
    let header: Vec<String> = range
        .rows()
        .next()
        .unwrap()
        .iter()
        .map(|c| c.to_string())
        .collect();
    assert_eq!(header, strings(&["code", "name"]));
    let plan = run(&mut conn, &path, false).unwrap();
    assert_eq!(plan.added.len(), 3);
    assert_eq!(record(&conn, "FW-01").system_code.as_deref(), Some("SYS"));
}

/// code review：清冊沒有 system 欄但 ISMS 編號換了系統代碼時，資產說明由新編號重新帶出
#[test]
fn new_isms_id_rederives_the_system_without_a_system_column() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    run(&mut conn, &fixture("inventory_basic.xlsx"), false).unwrap();
    assert_eq!(record(&conn, "FW-01").system_code.as_deref(), Some("SYS"));
    create_system(&conn, "NET", "網路服務").unwrap();
    let path = workbook(
        env.dir.path(),
        "a.xlsx",
        &[
            &[
                "asset_tag",
                "name",
                "category",
                "product",
                "version",
                "isms_id",
            ],
            &["FW-01", "fw01", "HW03", "PAN-OS", "11.1.4-h7", "NET-HW-003"],
        ],
        None,
    );
    run(&mut conn, &path, false).unwrap();
    let fw = record(&conn, "FW-01");
    assert_eq!(fw.isms_id.as_deref(), Some("NET-HW-003"));
    assert_eq!(fw.system_code.as_deref(), Some("NET"));
    assert_eq!(
        fw.custodian.as_deref(),
        Some("王小明"),
        "其他缺少的欄位保留"
    );
}
