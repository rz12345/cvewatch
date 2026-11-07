//! 3.8：以 tests/fixtures/inventory_*.xlsx 驗證冪等、中文欄名、缺欄中止、prune，以及 dry-run、未辨識與版本警告。

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::Utc;
use cvewatch_core::assets::columns::ColumnMap;
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::plan::ImportPlan;
use cvewatch_core::assets::sheet::read_inventory;
use cvewatch_core::assets::template::write_template;
use cvewatch_core::assets::validate::parse_inventory;
use cvewatch_core::assets::{AssetRepo, AssetStatus, ComponentRepo, Exposure};
use cvewatch_core::catalog::Catalog;
use cvewatch_core::db::open_working_db;
use cvewatch_core::CoreError;
use rusqlite::Connection;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn catalog() -> Catalog {
    Catalog::seed().unwrap()
}

fn run(
    conn: &mut Connection,
    file: &str,
    columns: &ColumnMap,
    options: ImportOptions,
) -> Result<ImportPlan, CoreError> {
    let book = read_inventory(&fixture(file), None)?;
    let inventory = parse_inventory(&book, columns)?;
    import_inventory(conn, &catalog(), &inventory, options, Utc::now())
}

fn import(conn: &mut Connection, file: &str) -> ImportPlan {
    run(conn, file, &ColumnMap::default(), ImportOptions::default()).unwrap()
}

fn version_of(conn: &Connection, tag: &str, key: &str) -> Option<String> {
    let asset = AssetRepo::new(conn).find_by_tag(tag).unwrap()?;
    ComponentRepo::new(conn)
        .for_asset(asset.id)
        .unwrap()
        .into_iter()
        .find(|c| c.label() == key)
        .map(|c| c.version_raw)
}

fn asset_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM asset", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn first_import_creates_assets_components_and_reports() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let plan = import(&mut conn, "inventory_basic.xlsx");

    assert_eq!(plan.added.len(), 6);
    assert_eq!(plan.updated.len(), 0);
    // R6515-01：ESXi、iDRAC9、BIOS 三個元件
    let r6515 = AssetRepo::new(&conn)
        .find_by_tag("R6515-01")
        .unwrap()
        .unwrap();
    let keys: Vec<String> = ComponentRepo::new(&conn)
        .for_asset(r6515.id)
        .unwrap()
        .iter()
        .map(|c| c.label().to_string())
        .collect();
    assert_eq!(keys.len(), 3, "{keys:?}");
    // 「Palo Alto PA-460」辨識為 PAN-OS
    assert_eq!(
        version_of(&conn, "FW-02", "paloalto-panos").as_deref(),
        Some("11.1 latest")
    );

    // 未辨識：NAS-03「Synolgy DSM」→ 建議 synology-dsm，資產建立但無元件
    assert_eq!(plan.unresolved.len(), 1);
    let item = &plan.unresolved[0];
    assert_eq!(item.asset_tag, "NAS-03");
    assert_eq!(
        item.suggestions.first().map(String::as_str),
        Some("synology-dsm")
    );
    let nas03 = AssetRepo::new(&conn)
        .find_by_tag("NAS-03")
        .unwrap()
        .unwrap();
    assert!(ComponentRepo::new(&conn)
        .for_asset(nas03.id)
        .unwrap()
        .is_empty());
    let listed = AssetRepo::new(&conn).unresolved().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].product_raw, "Synolgy DSM");

    // 版本無法解析只是警告
    assert_eq!(plan.warnings.len(), 1);
    assert_eq!(plan.warnings[0].asset_tag, "FW-02");

    // 預設值
    let nas02 = AssetRepo::new(&conn)
        .find_by_tag("NAS-02")
        .unwrap()
        .unwrap();
    assert_eq!(nas02.record.exposure, Exposure::Internal);
    assert_eq!(nas02.record.criticality, 2);
}

#[test]
fn reimporting_same_file_is_idempotent() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    import(&mut conn, "inventory_basic.xlsx");
    let second = import(&mut conn, "inventory_basic.xlsx");
    assert_eq!(second.added.len(), 0);
    assert_eq!(second.updated.len(), 0);
    assert_eq!(second.unchanged, 6);
    let history: i64 = conn
        .query_row("SELECT count(*) FROM component_history", [], |r| r.get(0))
        .unwrap();
    assert_eq!(history, 0);
}

#[test]
fn dry_run_shows_version_change_without_writing() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    import(&mut conn, "inventory_basic.xlsx");
    let options = ImportOptions {
        dry_run: true,
        prune: false,
    };
    let plan = run(
        &mut conn,
        "inventory_v2.xlsx",
        &ColumnMap::default(),
        options,
    )
    .unwrap();

    let fw = plan
        .updated
        .iter()
        .find(|u| u.asset_tag == "FW-01")
        .unwrap();
    assert_eq!(fw.version_changes[0].from, "11.1.4-h7");
    assert_eq!(fw.version_changes[0].to, "11.1.6");
    assert_eq!(plan.version_change_count(), 2);
    assert_eq!(plan.disappeared, vec!["NAS-02".to_string()]);
    assert_eq!(
        version_of(&conn, "FW-01", "paloalto-panos").as_deref(),
        Some("11.1.4-h7")
    );
}

#[test]
fn version_change_writes_history_and_disappeared_asset_stays_active() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    import(&mut conn, "inventory_basic.xlsx");
    let plan = import(&mut conn, "inventory_v2.xlsx");
    assert_eq!(plan.disappeared, vec!["NAS-02".to_string()]);

    assert_eq!(
        version_of(&conn, "NAS-01", "synology-dsm").as_deref(),
        Some("7.2.2-72806")
    );
    let nas01 = AssetRepo::new(&conn)
        .find_by_tag("NAS-01")
        .unwrap()
        .unwrap();
    let components = ComponentRepo::new(&conn);
    let dsm = &components.for_asset(nas01.id).unwrap()[0];
    let history = components.history(dsm.id).unwrap();
    assert_eq!(history[0].version_raw, "7.2.1-69057");

    let nas02 = AssetRepo::new(&conn)
        .find_by_tag("NAS-02")
        .unwrap()
        .unwrap();
    assert_eq!(nas02.status, AssetStatus::Active, "未加 --prune 不退役");
}

#[test]
fn prune_retires_disappeared_assets() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    import(&mut conn, "inventory_basic.xlsx");
    let options = ImportOptions {
        dry_run: false,
        prune: true,
    };
    run(
        &mut conn,
        "inventory_v2.xlsx",
        &ColumnMap::default(),
        options,
    )
    .unwrap();
    let nas02 = AssetRepo::new(&conn)
        .find_by_tag("NAS-02")
        .unwrap()
        .unwrap();
    assert_eq!(nas02.status, AssetStatus::Retired);
    assert!(nas02.retired_at.is_some());
    let active: Vec<String> = cvewatch_core::matching::load::MatchData::load(&conn)
        .unwrap()
        .components
        .into_iter()
        .map(|c| c.asset_tag)
        .collect();
    assert!(!active.contains(&"NAS-02".to_string()));

    // 重新出現在清冊時恢復為 active
    let back = import(&mut conn, "inventory_basic.xlsx");
    assert!(back
        .updated
        .iter()
        .any(|u| u.asset_tag == "NAS-02" && u.reactivated));
}

#[test]
fn chinese_headers_with_column_mapping() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let mapping: BTreeMap<String, String> = [
        ("財產編號", "asset_tag"),
        ("設備名稱", "name"),
        ("類別", "category"),
        ("產品", "product"),
        ("版本", "version"),
        ("曝露度", "exposure"),
        ("重要性", "criticality"),
        ("放置地點", "location"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    let columns = ColumnMap::from_config(&mapping).unwrap();
    let plan = run(
        &mut conn,
        "inventory_chinese.xlsx",
        &columns,
        ImportOptions::default(),
    )
    .unwrap();
    assert_eq!(plan.added, vec!["FW-01".to_string()]);
    let fw = AssetRepo::new(&conn).find_by_tag("FW-01").unwrap().unwrap();
    assert_eq!(fw.record.location.as_deref(), Some("機房 A"));

    // 沒有對應時視為缺欄
    let err = run(
        &mut conn,
        "inventory_chinese.xlsx",
        &ColumnMap::default(),
        ImportOptions::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("asset_tag"), "{err}");
}

#[test]
fn missing_column_aborts() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let err = run(
        &mut conn,
        "inventory_missing_column.xlsx",
        &ColumnMap::default(),
        ImportOptions::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("version"), "{err}");
    assert_eq!(asset_count(&conn), 0);
}

#[test]
fn row_errors_abort_whole_batch_with_row_numbers() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    import(&mut conn, "inventory_basic.xlsx");
    let before = asset_count(&conn);
    let err = run(
        &mut conn,
        "inventory_bad_rows.xlsx",
        &ColumnMap::default(),
        ImportOptions::default(),
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, CoreError::ImportRejected(_)));
    assert!(msg.contains("第 12 列：缺少 version"), "{msg}");
    assert!(
        msg.contains("第 5 列：exposure「public」不是允許的值"),
        "{msg}"
    );
    assert_eq!(asset_count(&conn), before, "資料庫維持匯入前狀態");
}

#[test]
fn csv_with_separate_components_file() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let book = read_inventory(
        &fixture("inventory_basic.csv"),
        Some(&fixture("inventory_components.csv")),
    )
    .unwrap();
    let inventory = parse_inventory(&book, &ColumnMap::default()).unwrap();
    let plan = import_inventory(
        &mut conn,
        &catalog(),
        &inventory,
        ImportOptions::default(),
        Utc::now(),
    )
    .unwrap();
    assert_eq!(plan.added.len(), 2);
    assert_eq!(
        version_of(&conn, "R6515-01", "dell-idrac9").as_deref(),
        Some("7.10.30.00")
    );
}

#[test]
fn template_roundtrips_through_importer() {
    let env = common::initialized();
    let path = env.dir.path().join("inv.xlsx");
    write_template(&path).unwrap();
    assert!(write_template(&path).is_err(), "不覆蓋既有檔案");
    let book = read_inventory(&path, None).unwrap();
    assert_eq!(book.assets.headers[0], "asset_tag");
    assert_eq!(book.components.as_ref().unwrap().headers[0], "asset_tag");
    let inventory = parse_inventory(&book, &ColumnMap::default()).unwrap();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let plan = import_inventory(
        &mut conn,
        &catalog(),
        &inventory,
        ImportOptions::default(),
        Utc::now(),
    )
    .unwrap();
    assert_eq!(plan.added.len(), 3);
    assert!(
        plan.unresolved.is_empty() && plan.warnings.is_empty(),
        "{plan:?}"
    );
}

#[test]
fn invalid_category_aborts_with_row_number() {
    use cvewatch_core::assets::columns::SheetKind;
    use cvewatch_core::assets::sheet::{Sheet, SheetRow, Workbook};
    let cells = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let book = Workbook {
        assets: Sheet {
            kind: SheetKind::Assets,
            headers: cells(&["asset_tag", "name", "category", "product", "version"]),
            rows: vec![SheetRow {
                number: 2,
                cells: cells(&["PRN-01", "prn01", "printer", "Debian", "12.7"]),
            }],
        },
        components: None,
    };
    let err = parse_inventory(&book, &ColumnMap::default()).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("第 2 列：category「printer」不是允許的值"),
        "{msg}"
    );
}

/// 04 code review 第十二輪 #5：只有影響比對的變更才需要重新比對。
#[test]
fn plan_reports_whether_matching_is_affected() {
    use cvewatch_core::assets::plan::{AssetUpdate, ImportPlan};
    let with = |update: AssetUpdate| ImportPlan {
        updated: vec![update],
        ..ImportPlan::default()
    };
    assert!(!ImportPlan::default().affects_matching());
    let owner_only = AssetUpdate {
        asset_tag: "FW-01".into(),
        changed_fields: vec!["owner", "notes", "criticality"],
        ..AssetUpdate::default()
    };
    assert!(!with(owner_only).affects_matching());
    let exposure = AssetUpdate {
        asset_tag: "FW-01".into(),
        changed_fields: vec!["exposure"],
        exposure_changed: true,
        ..AssetUpdate::default()
    };
    assert!(with(exposure).affects_matching());
    let reactivated = AssetUpdate {
        asset_tag: "FW-01".into(),
        reactivated: true,
        ..AssetUpdate::default()
    };
    assert!(with(reactivated).affects_matching());
}
