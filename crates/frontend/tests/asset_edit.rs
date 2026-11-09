//! 10 任務 4.3～4.4：`Workspace` 的資產編修、退役／恢復／永久刪除、清冊匯出與範本。
#![allow(dead_code)]

#[path = "../../core/tests/common/mod.rs"]
mod common;

use common::matching::{at, world, World};
use cvewatch_frontend::asset_form::{AssetForm, UnresolvedAction, UnresolvedDraft};
use cvewatch_frontend::data::{AssetListFilter, Settings};
use cvewatch_frontend::test_support::{self, Recorder};
use cvewatch_frontend::{Context, Level, Workspace};
use rusqlite::params;

const NOW: &str = "2026-11-07T00:00:00+00:00";

fn open(w: World) -> (Workspace, Recorder) {
    let rec = Recorder::default();
    std::fs::create_dir_all(rec.documents_dir()).unwrap();
    let ctx = Context {
        conn: w.conn,
        settings: Settings {
            catalog: w.catalog,
            classes: w.classes,
            scoring: w.settings.scoring.clone(),
            stale_days: w.settings.stale_days,
        },
        config: cvewatch_core::config::Config::default(),
        config_path: rec.config_path(),
        services: test_support::services(&rec, at(NOW)),
    };
    (Workspace::new(ctx).expect("workspace"), rec)
}

/// FW-01（PAN-OS 11.1.4-h7）與已退役的 NAS-02。
fn assets_world() -> World {
    let w = world();
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4-h7")]);
    w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.retire("NAS-02");
    w
}

fn edit_form(ws: &Workspace, tag: &str) -> AssetForm {
    AssetForm::from_entry(ws.data.asset(tag).expect("asset"))
}

fn version(ws: &Workspace, tag: &str) -> String {
    ws.data.asset(tag).expect("asset").components[0]
        .version_raw
        .clone()
}

#[test]
fn asset_version_edit_needs_match_and_reloads() {
    let (mut ws, _rec) = open(assets_world());
    let mut form = edit_form(&ws, "FW-01");
    form.components[0].version = "11.1.6".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    assert!(r.needs_match);
    let m = &r.report.message;
    assert!(m.contains("PAN-OS 版本改為 11.1.6"), "{m}");
    assert!(!m.contains("paloalto-panos"), "產品以顯示名稱呈現：{m}");
    assert_eq!(version(&ws, "FW-01"), "11.1.6");
}

#[test]
fn asset_unchanged_form_writes_nothing() {
    let (mut ws, _rec) = open(assets_world());
    let form = edit_form(&ws, "FW-01");
    let r = ws.save_asset_form(&form, &[]);
    assert!(!r.needs_match);
    assert!(
        r.report.message.contains("沒有變更"),
        "{}",
        r.report.message
    );
}

#[test]
fn asset_failed_step_writes_nothing() {
    let (mut ws, _rec) = open(assets_world());
    let mut form = edit_form(&ws, "FW-01");
    form.name = "邊界防火牆".into();
    // 本地驗證不查目錄：加入元件的步驟由 core 拒絕，同一交易的欄位修改也不寫入
    form.add_product("no-such-product");
    form.components[1].version = "1.0".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Error);
    assert!(!r.needs_match);
    let m = &r.report.message;
    assert!(
        m.contains("未寫入任何變更") && m.contains("產品目錄中沒有"),
        "{m}"
    );
    assert_eq!(ws.data.asset("FW-01").unwrap().asset.record.name, "FW-01");
    let name: String = ws
        .conn
        .query_row(
            "SELECT name FROM asset WHERE asset_tag = 'FW-01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "FW-01");
}

#[test]
fn asset_replacing_a_component_with_the_same_product() {
    let (mut ws, _rec) = open(assets_world());
    let mut form = edit_form(&ws, "FW-01");
    form.components[0].removed = true;
    form.add_product("paloalto-panos");
    form.components[1].version = "11.1.6".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    assert_eq!(version(&ws, "FW-01"), "11.1.6");
}

#[test]
fn asset_first_step_failure_is_unchanged() {
    let (mut ws, _rec) = open(assets_world());
    let mut form = AssetForm::new();
    form.asset_tag = "NAS-02".into();
    form.name = "nas02".into();
    form.category = "HW05".into();
    form.add_product("synology-dsm");
    form.components[0].version = "7.2.2-72806".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Error);
    assert!(!r.needs_match);
    assert!(r.report.message.contains("已退役"), "{}", r.report.message);
}

#[test]
fn asset_local_validation_blocks_writes() {
    let (mut ws, _rec) = open(assets_world());
    let mut form = AssetForm::new();
    form.name = "x".into();
    form.category = "HW03".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Error);
    assert!(!r.needs_match);
    let m = &r.report.message;
    assert!(m.contains("資產編號：必填"), "{m}");
    assert!(!m.contains("asset_tag"), "{m}");
    assert!(r.report.message.contains("至少需要一個元件"));
}

#[test]
fn asset_new_form_creates_asset_with_warning_for_unreadable_version() {
    let (mut ws, _rec) = open(assets_world());
    let mut form = AssetForm::new();
    form.asset_tag = "ESX-01".into();
    form.name = "esx01".into();
    form.category = "HW01".into();
    form.add_product("vmware-esxi");
    form.components[0].version = "8.0 latest".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Warning, "{}", r.report.message);
    assert!(r.report.message.contains("不符合 esxi 格式"));
    assert!(r.needs_match);
    assert_eq!(version(&ws, "ESX-01"), "8.0 latest");
}

#[test]
fn asset_unresolved_mapping_and_removal_order() {
    let w = assets_world();
    w.conn
        .execute(
            "INSERT INTO asset_unresolved (asset_id, product_raw, version_raw, seen_at)
             VALUES ((SELECT id FROM asset WHERE asset_tag = 'FW-01'), 'PVE 叢集', '8.2.4', ?1)",
            params![NOW],
        )
        .unwrap();
    let (mut ws, _rec) = open(w);
    let mut form = edit_form(&ws, "FW-01");
    let id = ws.data.asset("FW-01").unwrap().unresolved[0].id;
    assert_eq!(
        form.unresolved,
        vec![UnresolvedDraft {
            id,
            product_raw: "PVE 叢集".into(),
            version_raw: "8.2.4".into(),
            action: UnresolvedAction::Keep,
        }]
    );
    // 換掉唯一的元件：先加入（改為目錄產品）再移除，不被「至少一個元件」擋下
    form.unresolved[0].action = UnresolvedAction::MapTo("proxmox-ve".into());
    form.components[0].removed = true;
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    let entry = ws.data.asset("FW-01").unwrap();
    assert!(entry.unresolved.is_empty());
    let keys: Vec<&str> = entry.components.iter().map(|c| c.label()).collect();
    assert_eq!(keys, vec!["proxmox-ve"]);
}

#[test]
fn asset_list_includes_retired_only_when_asked() {
    let (ws, _rec) = open(assets_world());
    let tags = |include_retired: bool| -> Vec<String> {
        ws.data
            .asset_list(&AssetListFilter {
                include_retired,
                class: None,
                ..AssetListFilter::default()
            })
            .iter()
            .map(|a| a.asset.record.asset_tag.clone())
            .collect()
    };
    assert_eq!(tags(false), vec!["FW-01"]);
    assert_eq!(tags(true), vec!["FW-01", "NAS-02"]);
    assert!(ws.data.asset("NAS-02").is_none(), "使用中清單不含已退役");
    assert!(ws.data.any_asset("NAS-02").is_some());
}

#[test]
fn asset_list_filters_by_class_or_major_class() {
    use cvewatch_core::assets::ClassFilter;
    let w = assets_world();
    w.conn
        .execute(
            "UPDATE asset SET category = 'SW02' WHERE asset_tag = 'FW-01'",
            [],
        )
        .unwrap();
    w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.conn
        .execute(
            "UPDATE asset SET category = 'HW05' WHERE asset_tag = 'NAS-01'",
            [],
        )
        .unwrap();
    let (ws, _rec) = open(w);
    let tags = |class: &str| -> Vec<String> {
        ws.data
            .asset_list(&AssetListFilter {
                include_retired: false,
                class: ClassFilter::parse(class),
                ..AssetListFilter::default()
            })
            .iter()
            .map(|a| a.asset.record.asset_tag.clone())
            .collect()
    };
    assert_eq!(tags("SW"), vec!["FW-01"]);
    assert_eq!(tags("hw"), vec!["NAS-01"]);
    assert_eq!(tags("HW05"), vec!["NAS-01"]);
    assert!(tags("HW01").is_empty());
}

/// Scenario: Changing class keeps ratings — PVE-01 為 HW01、C/I/A 3/3/4，改為 HW05 後 C/I/A 不變。
#[test]
fn changing_class_keeps_ratings() {
    let w = world();
    w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.4")]);
    w.conn
        .execute(
            "UPDATE asset SET category = 'HW01', confidentiality = 3, integrity = 3,
                              availability = 4
             WHERE asset_tag = 'PVE-01'",
            [],
        )
        .unwrap();
    let (mut ws, _rec) = open(w);
    let mut form = edit_form(&ws, "PVE-01");
    assert_eq!(
        (
            form.confidentiality.as_str(),
            form.integrity.as_str(),
            form.availability.as_str()
        ),
        ("3", "3", "4"),
        "編輯表單帶入已設定的值"
    );
    form.category = "HW05".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    let record = &ws.data.asset("PVE-01").unwrap().asset.record;
    assert_eq!(record.category.as_str(), "HW05");
    assert_eq!(record.cia, cvewatch_core::assets::Cia::new(3, 3, 4));
}

/// ISMS 編號已被已退役資產使用時，錯誤標在「ISMS 編號」欄位（寫入前的本地檢查）。
#[test]
fn isms_id_used_by_a_retired_asset_is_reported_on_its_field() {
    use cvewatch_core::assets::columns::StandardField;
    let w = assets_world();
    cvewatch_core::assets::systems::create_system(&w.conn, "SYS", "核心系統").unwrap();
    w.conn
        .execute(
            "UPDATE asset SET isms_id = 'SYS-HW-005', system_code = 'SYS' WHERE asset_tag = 'NAS-02'",
            [],
        )
        .unwrap();
    let (ws, _rec) = open(w);
    let mut form = edit_form(&ws, "FW-01");
    form.isms_id = "sys-hw-005".into();
    let e = ws.validate_asset_form(&form).unwrap_err();
    assert_eq!(
        e.field(StandardField::IsmsId),
        Some("已被使用（已退役的 NAS-02）")
    );
    form.isms_id = "SYS-HW-006".into();
    assert!(ws.validate_asset_form(&form).is_ok());
}

#[test]
fn asset_retire_restore_and_purge() {
    let (mut ws, _rec) = open(assets_world());
    let r = ws.retire_asset("FW-01", &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    assert!(r.needs_match);
    assert!(ws.data.asset("FW-01").is_none());
    let r = ws.retire_asset("FW-01", &[]);
    assert_eq!(r.report.level, Level::Error);
    assert!(
        r.report.message.starts_with("將 FW-01 標為退役失敗"),
        "{}",
        r.report.message
    );

    let r = ws.restore_asset("FW-01", &[]);
    assert!(r.needs_match);
    assert!(ws.data.asset("FW-01").is_some());

    assert!(ws.purge_preview("FW-01").is_err(), "使用中不可永久刪除");
    let preview = ws.purge_preview("NAS-02").unwrap();
    assert_eq!(preview.components, 1);
    let r = ws.purge_asset("NAS-02", &[]);
    assert!(
        r.report.message.contains("已永久刪除 NAS-02"),
        "{}",
        r.report.message
    );
    assert!(ws.data.any_asset("NAS-02").is_none());
}

#[test]
fn asset_export_and_template_reports() {
    let (ws, rec) = open(assets_world());
    let (dir, name) = ws.inventory_export_default();
    assert_eq!(dir, rec.documents_dir());
    assert_eq!(name, "cvewatch-inventory-20261107.xlsx");
    let path = dir.join(&name);
    let r = ws.export_inventory(&path);
    assert_eq!(r.level, Level::Info, "{}", r.message);
    assert!(r.message.contains("已匯出 1 個資產") && r.message.contains("此檔未加密"));
    let again = ws.export_inventory(&path);
    assert_eq!(again.level, Level::Error);
    assert!(again.message.contains("目的檔已存在"), "{}", again.message);

    let (dir, name) = ws.template_default();
    assert_eq!(name, "cvewatch-inventory-template.xlsx");
    let r = ws.write_template(&dir.join(&name));
    assert!(r.message.contains("已產生範本"), "{}", r.message);
    assert_eq!(ws.write_template(&dir.join(&name)).level, Level::Error);
}
