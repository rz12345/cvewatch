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
use cvewatch_core::assets::validate::{CRITICALITY_NOTICE, ISMS_KEPT_NOTICE};
use cvewatch_core::assets::{AssetClasses, AssetRepo, AssetStatus, Cia, ComponentRepo, Exposure};
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

fn classes() -> AssetClasses {
    AssetClasses::builtin()
}

fn run(
    conn: &mut Connection,
    file: &str,
    columns: &ColumnMap,
    options: ImportOptions,
) -> Result<ImportPlan, CoreError> {
    run_with(conn, file, columns, &classes(), options)
}

fn run_with(
    conn: &mut Connection,
    file: &str,
    columns: &ColumnMap,
    classes: &AssetClasses,
    options: ImportOptions,
) -> Result<ImportPlan, CoreError> {
    let book = read_inventory(&fixture(file), None)?;
    let inventory = parse_inventory(&book, columns, classes)?;
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
    // SRV-01：ESXi、iDRAC9、BIOS 三個元件
    let srv = AssetRepo::new(&conn)
        .find_by_tag("SRV-01")
        .unwrap()
        .unwrap();
    let keys: Vec<String> = ComponentRepo::new(&conn)
        .for_asset(srv.id)
        .unwrap()
        .iter()
        .map(|c| c.label().to_string())
        .collect();
    assert_eq!(keys.len(), 3, "{keys:?}");
    // 「Palo Alto PAN-OS」辨識為 PAN-OS
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
    assert_eq!(nas02.record.cia, Cia::new(2, 3, 4), "HW05 範本值");
    // 子類別名稱「儲存設備」、小寫代碼 hw05；有填的 C/I/A 覆寫範本值
    let tags = |tag: &str| AssetRepo::new(&conn).find_by_tag(tag).unwrap().unwrap();
    assert_eq!(tags("NAS-01").record.category.as_str(), "HW05");
    assert_eq!(tags("NAS-03").record.category.as_str(), "HW05");
    assert_eq!(tags("NAS-03").record.cia, Cia::new(2, 3, 2));
    assert_eq!(tags("SRV-01").record.cia, Cia::new(3, 3, 4));
    let fw01 = tags("FW-01");
    assert_eq!(fw01.record.isms_id.as_deref(), Some("SYS-HW-003"));
    assert!(
        plan.notices.is_empty(),
        "新格式清冊沒有 notice：{:?}",
        plan.notices
    );
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
        ("機密性", "confidentiality"),
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
    assert_eq!(fw.record.category.as_str(), "HW03", "類別以子類別名稱填寫");
    assert_eq!(fw.record.cia, Cia::new(3, 3, 4));

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
    assert!(msg.contains("第 12 列：缺少 product"), "{msg}");
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
    let inventory = parse_inventory(&book, &ColumnMap::default(), &classes()).unwrap();
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
        version_of(&conn, "SRV-01", "dell-idrac9").as_deref(),
        Some("7.10.30.00")
    );
}

#[test]
fn template_roundtrips_through_importer() {
    let env = common::initialized();
    let path = env.dir.path().join("inv.xlsx");
    write_template(&path, &classes()).unwrap();
    assert!(write_template(&path, &classes()).is_err(), "不覆蓋既有檔案");
    let book = read_inventory(&path, None).unwrap();
    assert_eq!(book.assets.headers[0], "asset_tag");
    assert!(!book.assets.headers.iter().any(|h| h == "criticality"));
    assert_eq!(book.components.as_ref().unwrap().headers[0], "asset_tag");
    let inventory = parse_inventory(&book, &ColumnMap::default(), &classes()).unwrap();
    assert!(inventory.notices.is_empty(), "{:?}", inventory.notices);
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
        systems: None,
    };
    let err = parse_inventory(&book, &ColumnMap::default(), &classes()).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("第 2 列：category「printer」不是允許的值"),
        "{msg}"
    );
}

/// 04 code review 第十二輪 #5：只有影響比對的變更才需要重新比對。
#[test]
fn plan_reports_whether_matching_is_affected() {
    use cvewatch_core::assets::columns::StandardField;
    use cvewatch_core::assets::plan::{AssetUpdate, FieldChange, ImportPlan};
    let with = |update: AssetUpdate| ImportPlan {
        updated: vec![update],
        ..ImportPlan::default()
    };
    assert!(!ImportPlan::default().affects_matching());
    let owner_only = AssetUpdate {
        asset_tag: "FW-01".into(),
        changed_fields: vec![
            FieldChange::new(StandardField::Custodian, "王小明", "李小華"),
            FieldChange::new(StandardField::Notes, "-", "備註"),
            FieldChange::new(StandardField::Confidentiality, "2", "3"),
            FieldChange::new(StandardField::IsmsId, "-", "SYS-HW-003"),
        ],
        ..AssetUpdate::default()
    };
    assert!(!with(owner_only).affects_matching());
    let exposure = AssetUpdate {
        asset_tag: "FW-01".into(),
        changed_fields: vec![FieldChange::new(
            StandardField::Exposure,
            "internal",
            "internet",
        )],
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

/// Scenario: Old inventory with English category / Old inventory with criticality
#[test]
fn legacy_inventory_is_converted_with_notices() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let plan = import(&mut conn, "inventory_legacy.xlsx");
    assert_eq!(plan.added.len(), 6);
    let notices: Vec<String> = plan.notices.iter().map(|n| n.to_string()).collect();
    assert_eq!(notices[0], CRITICALITY_NOTICE, "整批一次：{notices:?}");
    assert_eq!(
        notices
            .iter()
            .filter(|n| n.as_str() == CRITICALITY_NOTICE)
            .count(),
        1
    );
    assert!(
        notices.contains(&"第 2 列 FW-01：security 已轉為 HW03 資安設備".to_string()),
        "{notices:?}"
    );
    assert!(
        notices.contains(&"第 3 列 SRV-01：virtualization 已轉為 HW01 伺服器".to_string()),
        "{notices:?}"
    );
    let fw = AssetRepo::new(&conn).find_by_tag("FW-01").unwrap().unwrap();
    assert_eq!(fw.record.category.as_str(), "HW03");
    assert_eq!(
        fw.record.cia,
        Cia::new(2, 3, 4),
        "criticality 3 被忽略，依範本值"
    );
}

/// 匯出前先匯入新格式清冊，再以舊清冊重匯：C/I/A 回到範本值，dry-run 明細列出差異。
#[test]
fn legacy_reimport_lists_rating_changes() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    import(&mut conn, "inventory_basic.xlsx");
    let options = ImportOptions {
        dry_run: true,
        prune: false,
    };
    let plan = run(
        &mut conn,
        "inventory_legacy.xlsx",
        &ColumnMap::default(),
        options,
    )
    .unwrap();
    let esx = plan
        .updated
        .iter()
        .find(|u| u.asset_tag == "SRV-01")
        .expect("SRV-01 的機密性由 3 回到範本值 2");
    assert!(esx.field_names().contains(&"confidentiality"), "{esx:?}");
    assert!(
        !plan
            .updated
            .iter()
            .any(|u| u.field_names().contains(&"isms_id")),
        "舊清冊沒有 isms_id 欄，不清除既有編號：{:?}",
        plan.updated
    );
    assert!(
        plan.notices.iter().any(|n| n.message == ISMS_KEPT_NOTICE),
        "{:?}",
        plan.notices
    );
}

/// 13 code review：舊清冊（沒有 isms_id 欄）重匯時保留既有資產的 ISMS 編號；
/// 有 isms_id 欄但留空時才表示沒有 ISMS 編號。
#[test]
fn legacy_reimport_keeps_isms_ids() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    import(&mut conn, "inventory_basic.xlsx");
    let isms = |conn: &Connection| {
        AssetRepo::new(conn)
            .find_by_tag("FW-01")
            .unwrap()
            .unwrap()
            .record
            .isms_id
    };
    let plan = import(&mut conn, "inventory_legacy.xlsx");
    assert_eq!(isms(&conn).as_deref(), Some("SYS-HW-003"));
    assert!(plan.notices.iter().any(|n| n.message == ISMS_KEPT_NOTICE));
    // 新格式清冊的 isms_id 欄留空：清除
    let path = env.dir.path().join("blank.csv");
    std::fs::write(
        &path,
        "asset_tag,name,category,product,version,isms_id
FW-01,fw01,HW03,PAN-OS,11.1.4-h7,
",
    )
    .unwrap();
    let book = read_inventory(&path, None).unwrap();
    let inventory = parse_inventory(&book, &ColumnMap::default(), &classes()).unwrap();
    assert!(!inventory.keeps(cvewatch_core::assets::columns::StandardField::IsmsId));
    import_inventory(
        &mut conn,
        &catalog(),
        &inventory,
        ImportOptions::default(),
        Utc::now(),
    )
    .unwrap();
    assert_eq!(isms(&conn), None);
}

/// Scenario: Raising a template value — 設定檔調高 HW05 的機密性範本值，只影響之後
/// 留空匯入的資產；資料庫中既有資產的 C/I/A 不會自動改變。
#[test]
fn raising_a_template_value_affects_only_new_imports() {
    use cvewatch_core::assets::classes::AssetClassConfig;
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    import(&mut conn, "inventory_basic.xlsx");
    let raised = AssetClasses::from_config(&BTreeMap::from([(
        "HW05".to_string(),
        AssetClassConfig {
            confidentiality: Some(3),
            ..AssetClassConfig::default()
        },
    )]))
    .unwrap();
    let nas02 = |conn: &Connection| {
        AssetRepo::new(conn)
            .find_by_tag("NAS-02")
            .unwrap()
            .unwrap()
            .record
            .cia
    };
    assert_eq!(nas02(&conn), Cia::new(2, 3, 4), "設定變更不改既有資產");
    run_with(
        &mut conn,
        "inventory_basic.xlsx",
        &ColumnMap::default(),
        &raised,
        ImportOptions::default(),
    )
    .unwrap();
    assert_eq!(nas02(&conn), Cia::new(3, 3, 4), "留空重匯時採用新的範本值");
}

/// Scenario: Duplicate ISMS ID — retired 的 NAS-02 已使用 SYS-HW-005。
#[test]
fn duplicate_isms_id_with_a_retired_asset_is_rejected() {
    use cvewatch_core::assets::edit;
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    import(&mut conn, "inventory_basic.xlsx");
    conn.execute(
        "UPDATE asset SET isms_id = 'SYS-HW-005' WHERE asset_tag = 'NAS-02'",
        [],
    )
    .unwrap();
    edit::retire_asset(&mut conn, "NAS-02", Utc::now()).unwrap();
    let input = cvewatch_core::assets::validate::RecordInput {
        name: "nas01",
        category: "HW05",
        isms_id: "sys-hw-005",
        ..Default::default()
    };
    let err = edit::update_asset(&mut conn, &classes(), "NAS-01", &input, Utc::now())
        .unwrap_err()
        .to_string();
    assert!(err.contains("已被 NAS-02 使用（該資產已退役）"), "{err}");
    let nas01 = AssetRepo::new(&conn)
        .find_by_tag("NAS-01")
        .unwrap()
        .unwrap();
    assert_eq!(nas01.record.isms_id, None, "資產不變");

    // 清冊匯入同樣拒絕，資料庫不變
    let dir = env.dir.path().join("dup.csv");
    std::fs::write(
        &dir,
        "asset_tag,name,category,product,version,isms_id\n\
         NAS-01,nas01,HW05,Synology DSM,7.2.1-69057,sys-hw-005\n",
    )
    .unwrap();
    let book = read_inventory(&dir, None).unwrap();
    let inventory = parse_inventory(&book, &ColumnMap::default(), &classes()).unwrap();
    let err = import_inventory(
        &mut conn,
        &catalog(),
        &inventory,
        ImportOptions::default(),
        Utc::now(),
    )
    .unwrap_err()
    .to_string();
    assert!(
        err.contains("第 2 列：ISMS 編號 sys-hw-005 已被 NAS-02 使用（該資產已退役）"),
        "{err}"
    );
}

/// 兩個資產互換 ISMS 編號：重複檢查以匯入後的狀態判斷，寫入不違反唯一索引。
#[test]
fn swapping_isms_ids_in_one_import() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    cvewatch_core::assets::systems::create_system(&conn, "SYS", "核心系統").unwrap();
    let write = |name: &str, a: &str, b: &str| {
        let path = env.dir.path().join(name);
        std::fs::write(
            &path,
            format!(
                "asset_tag,name,category,product,version,isms_id\n\
                 FW-01,fw01,HW03,PAN-OS,11.1.4-h7,{a}\n\
                 FW-02,fw02,HW03,PAN-OS,11.1.4-h7,{b}\n"
            ),
        )
        .unwrap();
        path
    };
    let first = write("a.csv", "SYS-HW-001", "SYS-HW-002");
    let second = write("b.csv", "SYS-HW-002", "SYS-HW-001");
    for path in [first, second] {
        let book = read_inventory(&path, None).unwrap();
        let inventory = parse_inventory(&book, &ColumnMap::default(), &classes()).unwrap();
        import_inventory(
            &mut conn,
            &catalog(),
            &inventory,
            ImportOptions::default(),
            Utc::now(),
        )
        .unwrap();
    }
    let fw01 = AssetRepo::new(&conn).find_by_tag("FW-01").unwrap().unwrap();
    assert_eq!(fw01.record.isms_id.as_deref(), Some("SYS-HW-002"));
}

/// Scenario: Generating a template / Class sheet follows configuration
#[test]
fn template_has_class_sheet_following_configuration() {
    use calamine::{open_workbook_auto, Reader};
    use cvewatch_core::assets::classes::AssetClassConfig;
    let env = common::initialized();
    let path = env.dir.path().join("t.xlsx");
    let classes = AssetClasses::from_config(&BTreeMap::from([(
        "HW12".to_string(),
        AssetClassConfig {
            name: Some("實驗儀器".into()),
            confidentiality: Some(2),
            integrity: Some(3),
            availability: Some(3),
            ..AssetClassConfig::default()
        },
    )]))
    .unwrap();
    write_template(&path, &classes).unwrap();
    let mut book = open_workbook_auto(&path).unwrap();
    assert_eq!(
        book.sheet_names(),
        vec!["assets", "components", "systems", "classes"]
    );
    let range = book.worksheet_range("classes").unwrap();
    let rows: Vec<Vec<String>> = range
        .rows()
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .collect();
    assert_eq!(rows[0][0], "code");
    let hw12 = rows.iter().find(|r| r[0] == "HW12").expect("HW12");
    assert_eq!(hw12[1], "實驗儀器");
    assert_eq!(&hw12[3..6], ["2", "3", "3"]);
    let sw01 = rows.iter().find(|r| r[0] == "SW01").unwrap();
    assert_eq!(&sw01[3..6], ["", "", ""], "無範本值時留空");
    assert_eq!(rows[0][6], "personal_data");
    assert_eq!(sw01[6], "N", "內建子類別預設不含個人資料");
    assert_eq!(rows.len(), 1 + 19);
}
