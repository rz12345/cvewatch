//! 10 任務 2.1～2.7：畫面編修（新增、欄位、元件、未辨識產品、退役、恢復、永久刪除）與編修後的比對。

mod common;
#[path = "common/notify.rs"]
mod support;

use std::sync::Arc;

use common::matching::{at, panos, world, World, T0};
use cvewatch_core::assets::columns::SheetKind;
use cvewatch_core::assets::edit::{self, NewComponent};
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::systems;
use cvewatch_core::assets::validate::{
    IncomingAsset, IncomingComponent, Inventory, RecordInput, RowRef,
};
use cvewatch_core::assets::{AssetRecord, AssetRepo, AssetStatus, Cia, ComponentRepo, Exposure};
use cvewatch_core::config::HttpConfig;
use cvewatch_core::matching::run::{rematch_state, run_match};
use cvewatch_core::notify::content::Zone;
use cvewatch_core::notify::dispatch::{Endpoint, Notifier};
use cvewatch_core::notify::redmine::{RedmineClient, RedmineSettings};
use cvewatch_core::notify::settings::{NotifyConfig, RedmineConfig};
use cvewatch_core::CoreError;
use rusqlite::params;
use serde_json::json;
use zeroize::Zeroizing;

const CVE: &str = "CVE-2099-10001";
const NOW: &str = "2026-11-07T00:00:00+00:00";

fn now() -> chrono::DateTime<chrono::Utc> {
    at(NOW)
}

fn firewall<'a>() -> RecordInput<'a> {
    RecordInput {
        asset_tag: "FW-02",
        name: "fw02",
        category: "HW03",
        ..RecordInput::default()
    }
}

fn comp(key: &str, version: &str) -> NewComponent {
    NewComponent {
        product_key: key.into(),
        version_raw: version.into(),
    }
}

fn message(r: Result<impl std::fmt::Debug, CoreError>) -> String {
    match r {
        Err(CoreError::Invalid(m)) => m,
        other => panic!("應以 Invalid 拒絕：{other:?}"),
    }
}

fn count(w: &World, sql: &str) -> i64 {
    w.conn.query_row(sql, [], |r| r.get(0)).expect(sql)
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

fn component_id(w: &World, tag: &str, key: &str) -> i64 {
    let asset = AssetRepo::new(&w.conn).find_by_tag(tag).unwrap().unwrap();
    ComponentRepo::new(&w.conn)
        .for_asset(asset.id)
        .unwrap()
        .into_iter()
        .find(|c| c.label() == key)
        .map(|c| c.id)
        .expect("component")
}

fn record(w: &World, tag: &str) -> AssetRecord {
    AssetRepo::new(&w.conn)
        .find_by_tag(tag)
        .unwrap()
        .unwrap()
        .record
}

/// 新增一筆未辨識產品，回傳其 id。
fn unresolved(w: &World, tag: &str, product: &str, version: &str) -> i64 {
    w.conn
        .execute(
            "INSERT INTO asset_unresolved (asset_id, product_raw, version_raw, seen_at)
             VALUES ((SELECT id FROM asset WHERE asset_tag = ?1), ?2, ?3, ?4)",
            params![tag, product, version, T0],
        )
        .unwrap();
    w.conn.last_insert_rowid()
}

fn unresolved_of(w: &World, tag: &str) -> Vec<String> {
    let mut stmt = w
        .conn
        .prepare(
            "SELECT u.product_raw FROM asset_unresolved u JOIN asset a ON a.id = u.asset_id
             WHERE a.asset_tag = ?1",
        )
        .unwrap();
    stmt.query_map([tag], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// 只影響 PAN-OS `11.1.4-h7` 的 CVE。
fn exact_cve(w: &World) {
    w.cve(
        CVE,
        panos(json!([{"version": "11.1.4-h7", "status": "affected"}])),
        json!([]),
    );
}

fn stale_needed(w: &World) -> bool {
    rematch_state(&w.conn, &w.catalog, &w.settings)
        .unwrap()
        .needed()
}

// ── Requirement: Creating an asset ──────────────────────────

#[test]
fn adding_a_new_firewall_uses_import_defaults() {
    let mut w = world();
    let out = edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &firewall(),
        &[comp("paloalto-panos", "11.2.3")],
        now(),
    )
    .unwrap();
    assert!(out.warnings.is_empty());
    let asset = AssetRepo::new(&w.conn)
        .find_by_tag("FW-02")
        .unwrap()
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Active);
    assert_eq!(asset.record.exposure, Exposure::Internal);
    assert_eq!(asset.record.cia, Cia::new(2, 3, 4), "HW03 範本值");
    assert_eq!(asset.record.cia.value(), 4);
    assert_eq!(asset.record.category.as_str(), "HW03");
    assert!(!asset.record.personal_data, "HW03 預設不含個人資料");
    assert_eq!(asset.record.protection_level(), None, "防護等級未評定");
    assert_eq!(asset.record.system_code, None);
    assert_eq!(
        versions(&w, "FW-02"),
        vec![("paloalto-panos".to_string(), "11.2.3".to_string())]
    );
}

#[test]
fn moving_an_asset_to_another_system_needs_a_matching_isms_id() {
    let mut w = world();
    systems::create_system(&w.conn, "SYS", "核心系統").unwrap();
    systems::create_system(&w.conn, "NET", "網路服務").unwrap();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.4")]);
    let input = |system, isms_id| RecordInput {
        name: "fw01",
        category: "HW03",
        system,
        isms_id,
        custodian: "王小明",
        ..RecordInput::default()
    };
    edit::update_asset(
        &mut w.conn,
        &w.classes,
        "FW-01",
        &input("", "SYS-HW-003"),
        now(),
    )
    .unwrap();
    let r = record(&w, "FW-01");
    assert_eq!(r.system_code.as_deref(), Some("SYS"), "由 ISMS 編號帶出");
    assert_eq!(r.custodian.as_deref(), Some("王小明"));
    let m = message(edit::update_asset(
        &mut w.conn,
        &w.classes,
        "FW-01",
        &input("NET", "SYS-HW-003"),
        now(),
    ));
    assert_eq!(
        m, "ISMS 編號「SYS-HW-003」的系統代碼 SYS 與資產說明 NET 不符",
        "{m}"
    );
    assert_eq!(
        record(&w, "FW-01").system_code.as_deref(),
        Some("SYS"),
        "資產不變"
    );
    edit::update_asset(
        &mut w.conn,
        &w.classes,
        "FW-01",
        &input("網路服務", ""),
        now(),
    )
    .unwrap();
    assert_eq!(record(&w, "FW-01").system_code.as_deref(), Some("NET"));
    let m = message(systems::delete_system(
        &w.conn,
        &cvewatch_core::assets::SystemCode::parse("NET").unwrap(),
    ));
    assert!(m.contains("仍有 1 個資產歸屬"), "{m}");
}

#[test]
fn tag_used_by_a_retired_asset_is_refused() {
    let mut w = world();
    w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.retire("NAS-02");
    let input = RecordInput {
        asset_tag: "NAS-02",
        ..firewall()
    };
    let m = message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &input,
        &[comp("synology-dsm", "7.2.2-72806")],
        now(),
    ));
    assert!(m.contains("已退役") && m.contains("恢復"), "{m}");
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")]);
    let input = RecordInput {
        asset_tag: "FW-01",
        ..firewall()
    };
    let m = message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &input,
        &[comp("paloalto-panos", "11.1.2")],
        now(),
    ));
    assert!(m.contains("已存在") && !m.contains("退役"), "{m}");
}

#[test]
fn no_component_given_leaves_database_unchanged() {
    let mut w = world();
    let m = message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &firewall(),
        &[],
        now(),
    ));
    assert!(m.contains("至少需要一個元件"), "{m}");
    assert_eq!(count(&w, "SELECT count(*) FROM asset"), 0);
    // 欄位錯誤與重複產品也不寫入任何資料
    let bad = RecordInput {
        category: "router",
        ..firewall()
    };
    message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &bad,
        &[comp("paloalto-panos", "11.2.3")],
        now(),
    ));
    message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &firewall(),
        &[
            comp("paloalto-panos", "11.2.3"),
            comp("paloalto-panos", "11.2.4"),
        ],
        now(),
    ));
    message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &firewall(),
        &[comp("no-such-product", "1.0")],
        now(),
    ));
    assert_eq!(count(&w, "SELECT count(*) FROM asset"), 0);
    assert_eq!(count(&w, "SELECT count(*) FROM component"), 0);
}

// ── Requirement: Editing asset fields ───────────────────────

#[test]
fn raising_exposure_updates_the_asset() {
    let mut w = world();
    w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.1-69057")]);
    let input = RecordInput {
        asset_tag: "ignored",
        name: "nas01",
        category: "HW05",
        exposure: "internet",
        custodian: "系統組",
        ..RecordInput::default()
    };
    edit::update_asset(&mut w.conn, &w.classes, "NAS-01", &input, now()).unwrap();
    let r = record(&w, "NAS-01");
    assert_eq!(
        r.asset_tag, "NAS-01",
        "修改欄位不使用 input.asset_tag（更名另由 rename_asset／EditOp::Rename）"
    );
    assert_eq!(r.exposure, Exposure::Internet);
    assert_eq!(r.custodian.as_deref(), Some("系統組"));
}

#[test]
fn invalid_rating_keeps_the_asset() {
    let mut w = world();
    w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.1-69057")]);
    let before = record(&w, "NAS-01");
    let input = RecordInput {
        name: "nas01",
        category: "HW05",
        confidentiality: "5",
        ..RecordInput::default()
    };
    let m = message(edit::update_asset(
        &mut w.conn,
        &w.classes,
        "NAS-01",
        &input,
        now(),
    ));
    assert!(m.contains("機密性") && m.contains("1–4"), "{m}");
    assert_eq!(record(&w, "NAS-01"), before);
}

/// Scenario: Changing class keeps ratings — 編修時 C/I/A 以已設定的值送出，改類別不改 C/I/A。
#[test]
fn changing_class_keeps_ratings() {
    let mut w = world();
    w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.4")]);
    let set = |category| RecordInput {
        name: "pve01",
        category,
        confidentiality: "3",
        integrity: "3",
        availability: "4",
        ..RecordInput::default()
    };
    edit::update_asset(&mut w.conn, &w.classes, "PVE-01", &set("HW01"), now()).unwrap();
    edit::update_asset(&mut w.conn, &w.classes, "PVE-01", &set("HW05"), now()).unwrap();
    let r = record(&w, "PVE-01");
    assert_eq!(r.category.as_str(), "HW05");
    assert_eq!(r.cia, Cia::new(3, 3, 4));
}

/// Scenario: Valid ISMS ID / Major class mismatch（編修）。
#[test]
fn isms_id_is_validated_on_edit() {
    let mut w = world();
    systems::create_system(&w.conn, "SYS", "核心系統").unwrap();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.4")]);
    let input = |isms_id| RecordInput {
        name: "fw01",
        category: "HW03",
        isms_id,
        ..RecordInput::default()
    };
    let m = message(edit::update_asset(
        &mut w.conn,
        &w.classes,
        "FW-01",
        &input("SYS-SW-003"),
        now(),
    ));
    assert!(m.contains("大類 SW 與資產類別 HW03 不符"), "{m}");
    edit::update_asset(
        &mut w.conn,
        &w.classes,
        "FW-01",
        &input("SYS-HW-003"),
        now(),
    )
    .unwrap();
    let r = record(&w, "FW-01");
    assert_eq!(r.isms_id.as_deref(), Some("SYS-HW-003"));
    assert_eq!(r.asset_tag, "FW-01");
    // 同一資產重新儲存（含只改大小寫）不視為重複
    edit::update_asset(
        &mut w.conn,
        &w.classes,
        "FW-01",
        &input("sys-hw-003"),
        now(),
    )
    .unwrap();
    // 其他資產使用同一編號時拒絕
    let m = message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &RecordInput {
            isms_id: "SYS-HW-003",
            ..firewall()
        },
        &[comp("paloalto-panos", "11.2.3")],
        now(),
    ));
    assert!(m.contains("已被 FW-01 使用"), "{m}");
}

#[test]
fn retired_asset_fields_cannot_be_edited() {
    let mut w = world();
    w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.retire("NAS-02");
    let input = RecordInput {
        name: "x",
        category: "HW05",
        ..RecordInput::default()
    };
    let m = message(edit::update_asset(
        &mut w.conn,
        &w.classes,
        "NAS-02",
        &input,
        now(),
    ));
    assert!(m.contains("已退役"), "{m}");
    let m = message(edit::add_component(
        &mut w.conn,
        &w.catalog,
        "NAS-02",
        &comp("dell-idrac9", "7.10.30.00"),
        now(),
    ));
    assert!(m.contains("已退役"), "{m}");
}

// ── Requirement: Editing components ─────────────────────────

#[test]
fn upgrading_firmware_records_history() {
    let mut w = world();
    let id = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4-h7")])[0];
    edit::set_component_version(&mut w.conn, &w.catalog, id, "11.1.6", now()).unwrap();
    assert_eq!(
        versions(&w, "FW-01"),
        vec![("paloalto-panos".to_string(), "11.1.6".to_string())]
    );
    let history = ComponentRepo::new(&w.conn).history(id).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].version_raw, "11.1.4-h7");
    assert_eq!(history[0].changed_at, NOW);
    let norm: Option<String> = w
        .conn
        .query_row(
            "SELECT version_norm FROM component WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(norm.is_some(), "與匯入相同計算 version_norm");
}

#[test]
fn adding_a_duplicate_product_is_refused() {
    let mut w = world();
    w.asset(
        "SRV-01",
        "mgmt",
        &[("vmware-esxi", "8.0 U3c"), ("dell-idrac9", "7.00.00.00")],
    );
    let m = message(edit::add_component(
        &mut w.conn,
        &w.catalog,
        "SRV-01",
        &comp("dell-idrac9", "7.10.30.00"),
        now(),
    ));
    assert!(m.contains("已有產品") && m.contains("修改版本"), "{m}");
    edit::add_component(
        &mut w.conn,
        &w.catalog,
        "SRV-01",
        &comp("dell-poweredge-bios", "2.16.0"),
        now(),
    )
    .unwrap();
    assert_eq!(versions(&w, "SRV-01").len(), 3);
}

#[test]
fn removing_the_last_component_is_refused() {
    let mut w = world();
    let id = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4-h7")])[0];
    let m = message(edit::remove_component(&mut w.conn, id, now()));
    assert!(
        m.contains("至少需要一個元件") && m.contains("刪除資產"),
        "{m}"
    );
    // 還有其他元件時可移除（軟刪除）
    edit::add_component(
        &mut w.conn,
        &w.catalog,
        "FW-01",
        &comp("dell-idrac9", "7.10.30.00"),
        now(),
    )
    .unwrap();
    edit::remove_component(&mut w.conn, id, now()).unwrap();
    assert_eq!(
        versions(&w, "FW-01"),
        vec![("dell-idrac9".to_string(), "7.10.30.00".to_string())]
    );
    // 移除後再加回同產品：恢復原元件（命中與歷史延續）
    edit::add_component(
        &mut w.conn,
        &w.catalog,
        "FW-01",
        &comp("paloalto-panos", "11.1.6"),
        now(),
    )
    .unwrap();
    assert_eq!(component_id(&w, "FW-01", "paloalto-panos"), id);
}

#[test]
fn version_text_the_scheme_cannot_read_is_saved_with_a_warning() {
    let mut w = world();
    let id = w.asset("SRV-01", "mgmt", &[("vmware-esxi", "8.0 U3c")])[0];
    let out =
        edit::set_component_version(&mut w.conn, &w.catalog, id, "8.0 latest", now()).unwrap();
    assert_eq!(out.warnings.len(), 1);
    assert_eq!(
        out.warnings[0].describe(),
        "SRV-01 vmware-esxi「8.0 latest」不符合 esxi 格式"
    );
    assert_eq!(
        versions(&w, "SRV-01"),
        vec![("vmware-esxi".to_string(), "8.0 latest".to_string())]
    );
}

#[test]
fn package_components_cannot_be_edited() {
    let mut w = world();
    w.asset("WEB-01", "dmz", &[("debian", "12.7")]);
    let pkg = w.package("WEB-01", "pkg:deb/debian/openssl", "3.0.11-1", T0);
    let m = message(edit::remove_component(&mut w.conn, pkg, now()));
    assert!(m.contains("套件清單"), "{m}");
    let m = message(edit::set_component_version(
        &mut w.conn,
        &w.catalog,
        pkg,
        "3.0.15-1",
        now(),
    ));
    assert!(m.contains("套件清單"), "{m}");
    assert_eq!(versions(&w, "WEB-01").len(), 2);
    // 套件元件不算清冊元件：移除唯一的清冊元件仍被拒絕
    let debian = component_id(&w, "WEB-01", "debian");
    message(edit::remove_component(&mut w.conn, debian, now()));
}

#[test]
fn empty_version_is_refused() {
    let mut w = world();
    let id = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4-h7")])[0];
    let m = message(edit::set_component_version(
        &mut w.conn,
        &w.catalog,
        id,
        "  ",
        now(),
    ));
    assert!(m.contains("version"), "{m}");
}

// ── Requirement: Resolving unresolved products ──────────────

#[test]
fn mapping_to_a_catalog_product_keeps_the_version() {
    let mut w = world();
    w.asset("PVE-01", "internal", &[("dell-idrac9", "7.10.30.00")]);
    let id = unresolved(&w, "PVE-01", "PVE 叢集", "8.2.4");
    edit::resolve_unresolved(&mut w.conn, &w.catalog, "PVE-01", id, "proxmox-ve", now()).unwrap();
    assert!(versions(&w, "PVE-01").contains(&("proxmox-ve".to_string(), "8.2.4".to_string())));
    assert!(unresolved_of(&w, "PVE-01").is_empty());
}

#[test]
fn deleting_the_only_unresolved_product_without_components_is_refused() {
    let mut w = world();
    w.asset("NAS-03", "internal", &[]);
    let typo = unresolved(&w, "NAS-03", "Synolgy DSM", "7.2");
    let m = message(edit::delete_unresolved(&mut w.conn, "NAS-03", typo, now()));
    assert!(m.contains("至少需要一個元件"), "{m}");
    unresolved(&w, "NAS-03", "QNAP", "5.1");
    edit::delete_unresolved(&mut w.conn, "NAS-03", typo, now()).unwrap();
    assert_eq!(unresolved_of(&w, "NAS-03"), vec!["QNAP".to_string()]);
    // 只剩未辨識產品時，它也算「至少一個」：清冊元件可移除
    let mut w = world();
    let id = w.asset("NAS-04", "internal", &[("synology-dsm", "7.2.1-69057")])[0];
    unresolved(&w, "NAS-04", "QNAP", "5.1");
    edit::remove_component(&mut w.conn, id, now()).unwrap();
}

#[test]
fn duplicate_unresolved_text_is_handled_by_id() {
    let mut w = world();
    w.asset("PVE-01", "internal", &[("dell-idrac9", "7.10.30.00")]);
    let old = unresolved(&w, "PVE-01", "PVE 叢集", "7.4");
    let new = unresolved(&w, "PVE-01", "PVE 叢集", "8.2");
    edit::resolve_unresolved(&mut w.conn, &w.catalog, "PVE-01", new, "proxmox-ve", now()).unwrap();
    assert!(versions(&w, "PVE-01").contains(&("proxmox-ve".to_string(), "8.2".to_string())));
    let left: i64 = w
        .conn
        .query_row("SELECT id FROM asset_unresolved", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, old, "只處理指定的那一筆");
    // 其他資產的未辨識產品 id 不可用
    w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.1-69057")]);
    let m = message(edit::delete_unresolved(&mut w.conn, "NAS-01", old, now()));
    assert!(m.contains("沒有這筆未辨識產品"), "{m}");
}

// ── 表單儲存：同一交易 ─────────────────────────────────────

#[test]
fn apply_edits_is_all_or_nothing() {
    let mut w = world();
    let id = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4-h7")])[0];
    let before = record(&w, "FW-01");
    let fields = RecordInput {
        name: "邊界防火牆",
        category: "HW03",
        ..RecordInput::default()
    };
    let ops = [
        edit::EditOp::Fields(Box::new(fields)),
        edit::EditOp::SetVersion {
            component_id: id,
            version_raw: "11.1.6".into(),
        },
        edit::EditOp::AddComponent(comp("no-such-product", "1.0")),
    ];
    let m = message(edit::apply_edits(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        "FW-01",
        &ops,
        now(),
    ));
    assert!(m.contains("產品目錄中沒有"), "{m}");
    assert_eq!(record(&w, "FW-01"), before, "第一步也不寫入");
    assert_eq!(versions(&w, "FW-01")[0].1, "11.1.4-h7");
    assert!(ComponentRepo::new(&w.conn).history(id).unwrap().is_empty());
}

#[test]
fn apply_edits_can_replace_a_component_with_the_same_product() {
    let mut w = world();
    let id = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4-h7")])[0];
    let ops = [
        edit::EditOp::RemoveComponent(id),
        edit::EditOp::AddComponent(comp("paloalto-panos", "11.1.6")),
    ];
    edit::apply_edits(&mut w.conn, &w.catalog, &w.classes, "FW-01", &ops, now()).unwrap();
    assert_eq!(
        versions(&w, "FW-01"),
        vec![("paloalto-panos".to_string(), "11.1.6".to_string())]
    );
    assert_eq!(
        component_id(&w, "FW-01", "paloalto-panos"),
        id,
        "恢復原元件"
    );
    // 移除後以未辨識產品改為同一產品
    let pve = w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.2")])[0];
    let raw = unresolved(&w, "PVE-01", "PVE 叢集", "8.2.4");
    let ops = [
        edit::EditOp::RemoveComponent(pve),
        edit::EditOp::ResolveUnresolved {
            unresolved_id: raw,
            product_key: "proxmox-ve".into(),
        },
    ];
    edit::apply_edits(&mut w.conn, &w.catalog, &w.classes, "PVE-01", &ops, now()).unwrap();
    assert_eq!(
        versions(&w, "PVE-01"),
        vec![("proxmox-ve".to_string(), "8.2.4".to_string())]
    );
    // 全部移除時在最後拒絕
    let ops = [edit::EditOp::RemoveComponent(pve)];
    let m = message(edit::apply_edits(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        "PVE-01",
        &ops,
        now(),
    ));
    assert!(m.contains("至少需要一個元件"), "{m}");
}

#[test]
fn tags_differing_only_in_case_are_refused() {
    let mut w = world();
    w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.2")]);
    let input = RecordInput {
        asset_tag: "fw-02",
        ..firewall()
    };
    let m = message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &input,
        &[comp("paloalto-panos", "11.2.3")],
        now(),
    ));
    assert!(m.contains("FW-02 已存在"), "{m}");
}

// ── Requirement: Retiring and restoring an asset ────────────

/// NAS-02（DSM，一筆 open 命中）與比對過的 CVE。
fn nas_world() -> (World, i64) {
    let mut w = world();
    let id = w.asset(
        "NAS-02",
        "internal",
        &[("synology-dsm", "7.2.1-69057 Update 5")],
    )[0];
    w.cve(
        CVE,
        json!([common::matching::affected(
            "Synology",
            "DiskStation Manager (DSM)",
            None,
            json!([{"version": "7.2", "lessThan": "7.2.2-72806", "status": "affected"}])
        )]),
        json!([]),
    );
    w.run();
    assert!(w.finding(id, CVE).is_some(), "前提：有命中");
    (w, id)
}

#[test]
fn deleting_an_asset_on_screen_retires_and_keeps_history() {
    let (mut w, _) = nas_world();
    let findings = count(&w, "SELECT count(*) FROM finding");
    let events = count(&w, "SELECT count(*) FROM finding_event");
    edit::retire_asset(&mut w.conn, "NAS-02", now()).unwrap();
    let asset = AssetRepo::new(&w.conn)
        .find_by_tag("NAS-02")
        .unwrap()
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Retired);
    assert_eq!(asset.retired_at.as_deref(), Some(NOW));
    assert_eq!(count(&w, "SELECT count(*) FROM finding"), findings);
    assert_eq!(count(&w, "SELECT count(*) FROM finding_event"), events);
    let m = message(edit::retire_asset(&mut w.conn, "NAS-02", now()));
    assert!(m.contains("已經是退役"), "{m}");
}

#[test]
fn restoring_a_retired_asset_lists_its_findings_again() {
    let (mut w, id) = nas_world();
    edit::retire_asset(&mut w.conn, "NAS-02", now()).unwrap();
    w.run_at(NOW);
    edit::restore_asset(&mut w.conn, "NAS-02", at("2026-11-07T01:00:00+00:00")).unwrap();
    let asset = AssetRepo::new(&w.conn)
        .find_by_tag("NAS-02")
        .unwrap()
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Active);
    assert_eq!(asset.retired_at, None);
    assert_eq!(asset.last_seen, "2026-11-07T01:00:00+00:00");
    w.run_at("2026-11-07T02:00:00+00:00");
    let listed = cvewatch_core::matching::query::list(
        &w.conn,
        &cvewatch_core::matching::query::FindingFilter::default(),
    )
    .unwrap();
    assert!(
        listed
            .iter()
            .any(|f| f.asset_tag == "NAS-02" && f.cve_id == CVE),
        "恢復後重新列出命中"
    );
    assert!(w.finding(id, CVE).is_some());
    let m = message(edit::restore_asset(&mut w.conn, "NAS-02", now()));
    assert!(m.contains("未退役"), "{m}");
}

// ── Requirement: Permanently deleting an asset ──────────────

fn rows_of_nas(w: &World) -> Vec<i64> {
    [
        "SELECT count(*) FROM component c JOIN asset a ON a.id = c.asset_id WHERE a.asset_tag = 'NAS-02'",
        "SELECT count(*) FROM component_history",
        "SELECT count(*) FROM finding",
        "SELECT count(*) FROM finding_event",
        "SELECT count(*) FROM asset_unresolved",
        "SELECT count(*) FROM asset WHERE asset_tag = 'NAS-02'",
    ]
    .iter()
    .map(|sql| count(w, sql))
    .collect()
}

#[test]
fn purging_a_retired_asset_removes_everything_of_it() {
    let (mut w, id) = nas_world();
    edit::set_component_version(&mut w.conn, &w.catalog, id, "7.2.1-69057 Update 6", now())
        .unwrap();
    unresolved(&w, "NAS-02", "QNAP", "5.1");
    w.conn
        .execute(
            "INSERT INTO pending (cve_id, reason, entered_at, last_checked)
             VALUES ('CVE-2099-20001', 'keyword_only', ?1, ?1)",
            [T0],
        )
        .unwrap();
    w.conn
        .execute(
            "INSERT INTO redmine_issue (cve_id, issue_id, created_at) VALUES (?1, 42, ?2)",
            params![CVE, T0],
        )
        .unwrap();
    edit::retire_asset(&mut w.conn, "NAS-02", now()).unwrap();
    assert!(
        rows_of_nas(&w).iter().all(|n| *n > 0),
        "{:?}",
        rows_of_nas(&w)
    );
    let keep = [
        count(&w, "SELECT count(*) FROM cve"),
        count(&w, "SELECT count(*) FROM pending"),
        count(&w, "SELECT count(*) FROM redmine_issue"),
    ];

    let preview = edit::purge_preview(&w.conn, "NAS-02").unwrap();
    assert_eq!((preview.components, preview.findings), (1, 1));
    let done = edit::purge_asset(&mut w.conn, "NAS-02", now()).unwrap();
    assert_eq!(done, preview);
    assert_eq!(rows_of_nas(&w), vec![0; 6]);
    assert_eq!(
        [
            count(&w, "SELECT count(*) FROM cve"),
            count(&w, "SELECT count(*) FROM pending"),
            count(&w, "SELECT count(*) FROM redmine_issue"),
        ],
        keep
    );
}

#[test]
fn active_asset_cannot_be_purged() {
    let mut w = world();
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4-h7")]);
    let m = message(edit::purge_preview(&w.conn, "FW-01"));
    assert!(m.contains("先標為退役"), "{m}");
    let m = message(edit::purge_asset(&mut w.conn, "FW-01", now()));
    assert!(m.contains("先標為退役"), "{m}");
    assert_eq!(count(&w, "SELECT count(*) FROM asset"), 1);
}

#[test]
fn reusing_a_purged_tag_imports_as_new() {
    let (mut w, _) = nas_world();
    edit::retire_asset(&mut w.conn, "NAS-02", now()).unwrap();
    edit::purge_asset(&mut w.conn, "NAS-02", now()).unwrap();
    let row = RowRef {
        sheet: SheetKind::Assets,
        number: 2,
    };
    let inventory = Inventory {
        assets: vec![IncomingAsset {
            row,
            record: cvewatch_core::assets::validate::validate_import_record(
                &RecordInput {
                    asset_tag: "NAS-02",
                    name: "nas02",
                    category: "HW05",
                    ..RecordInput::default()
                },
                &cvewatch_core::assets::AssetClasses::builtin(),
            )
            .unwrap()
            .record,
            system_raw: String::new(),
            components: vec![IncomingComponent {
                row,
                product_raw: "Synology DSM".into(),
                version_raw: "7.2.2-72806".into(),
            }],
        }],
        ..Inventory::default()
    };
    let plan = import_inventory(
        &mut w.conn,
        &w.catalog,
        &inventory,
        ImportOptions::default(),
        now(),
    )
    .unwrap();
    assert_eq!(plan.added, vec!["NAS-02".to_string()]);
}

// ── 2.6：排入佇列的通知不依賴已永久刪除的命中 ───────────────

#[test]
fn queued_redmine_issue_is_sent_after_the_asset_is_purged() {
    let pki = support::pki();
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, &pki.ca_pem).unwrap();
    let created = json!({"issue": {"id": 42}}).to_string();
    let (port, seen) = support::https_server(Arc::clone(&pki.server), vec![(201, created)]);
    let config = NotifyConfig {
        ca_file: Some(ca),
        redmine: RedmineConfig {
            enabled: true,
            base_url: format!("https://localhost:{port}"),
            project_id: "12".into(),
            tracker_id: 3,
            priority_map: Default::default(),
            assigned_to_id: 0,
            add_note_on_triage: true,
        },
        ..NotifyConfig::default()
    };
    let api = RedmineClient::new(&RedmineSettings {
        redmine: &config.redmine,
        api_key: Some(Zeroizing::new("k3y-for-tests".into())),
        ca_file: config.ca_file(),
        allow_insecure: config.allow_insecure_transport,
        http: &HttpConfig::default(),
    })
    .unwrap();
    let n = Notifier {
        config,
        stale_days: 14,
        zone: Zone::Local,
        mail: Endpoint::Disabled,
        redmine: Endpoint::Ready(Box::new(api)),
    };

    let mut w = world();
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.2")]);
    w.cve(
        CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.kev(CVE, "2026-10-20");
    w.cvss(CVE, 9.8);
    w.run();
    let queued = n.queue_urgent(&mut w.conn, now()).unwrap();
    assert_eq!(queued.issues, 1);
    edit::retire_asset(&mut w.conn, "FW-01", now()).unwrap();
    edit::purge_asset(&mut w.conn, "FW-01", now()).unwrap();
    assert_eq!(count(&w, "SELECT count(*) FROM outbox"), 1, "佇列項目保留");

    let stats = n.flush(&w.conn, now()).unwrap();
    assert_eq!(stats.sent, 1, "{:?}", stats.errors);
    assert!(stats.errors.is_empty());
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert_eq!(count(&w, "SELECT count(*) FROM outbox"), 0);
}

// ── Requirement: Edits are matched automatically ────────────

/// 一個編修操作。
type Edit = dyn Fn(&mut World);

#[test]
fn every_edit_marks_matching_as_needed() {
    let mut w = world();
    let fw = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4-h7")])[0];
    let pve = unresolved(&w, "FW-01", "Proxmox 叢集", "8.2.4");
    let qnap = unresolved(&w, "FW-01", "QNAP", "5.1");
    let mut ops: Vec<Box<Edit>> = vec![
        Box::new(|w| {
            edit::create_asset(
                &mut w.conn,
                &w.catalog,
                &w.classes,
                &firewall(),
                &[comp("paloalto-panos", "11.2.3")],
                now(),
            )
            .map(drop)
            .unwrap()
        }),
        Box::new(|w| {
            let input = RecordInput {
                name: "fw01",
                category: "HW03",
                exposure: "dmz",
                ..RecordInput::default()
            };
            edit::update_asset(&mut w.conn, &w.classes, "FW-01", &input, now())
                .map(drop)
                .unwrap()
        }),
        Box::new(|w| {
            edit::add_component(
                &mut w.conn,
                &w.catalog,
                "FW-01",
                &comp("dell-idrac9", "7.10.30.00"),
                now(),
            )
            .map(drop)
            .unwrap()
        }),
        Box::new(move |w| {
            edit::set_component_version(&mut w.conn, &w.catalog, fw, "11.1.6", now())
                .map(drop)
                .unwrap()
        }),
        Box::new(move |w| {
            edit::remove_component(&mut w.conn, fw, now())
                .map(drop)
                .unwrap()
        }),
        Box::new(move |w| {
            edit::resolve_unresolved(&mut w.conn, &w.catalog, "FW-01", pve, "proxmox-ve", now())
                .map(drop)
                .unwrap()
        }),
        Box::new(move |w| {
            edit::delete_unresolved(&mut w.conn, "FW-01", qnap, now())
                .map(drop)
                .unwrap()
        }),
        Box::new(|w| {
            edit::retire_asset(&mut w.conn, "FW-02", now())
                .map(drop)
                .unwrap()
        }),
        Box::new(|w| {
            edit::restore_asset(&mut w.conn, "FW-02", now())
                .map(drop)
                .unwrap()
        }),
        Box::new(|w| {
            edit::retire_asset(&mut w.conn, "FW-02", now()).unwrap();
            edit::purge_asset(&mut w.conn, "FW-02", now())
                .map(drop)
                .unwrap()
        }),
    ];
    for (i, op) in ops.iter_mut().enumerate() {
        run_match(&mut w.conn, &w.catalog, &w.settings, now()).unwrap();
        assert!(!stale_needed(&w), "第 {i} 個操作之前已比對");
        op(&mut w);
        assert!(stale_needed(&w), "第 {i} 個操作後需要比對");
    }
}

#[test]
fn version_fixed_and_finding_resolved() {
    let mut w = world();
    let id = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4-h7")])[0];
    exact_cve(&w);
    w.run();
    assert_eq!(w.finding(id, CVE).expect("命中").status, "open");
    edit::set_component_version(&mut w.conn, &w.catalog, id, "11.1.6", now()).unwrap();
    w.run_at(NOW);
    assert_eq!(w.finding(id, CVE).expect("命中保留").status, "resolved");
}

#[test]
fn failed_edit_does_not_mark_matching() {
    let mut w = world();
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4-h7")]);
    w.run();
    assert!(!stale_needed(&w));
    message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &firewall(),
        &[],
        now(),
    ));
    let input = RecordInput {
        name: "fw01",
        category: "HW03",
        availability: "9",
        ..RecordInput::default()
    };
    message(edit::update_asset(
        &mut w.conn,
        &w.classes,
        "FW-01",
        &input,
        now(),
    ));
    assert!(!stale_needed(&w), "拒絕的編修不寫入任何資料");
}
