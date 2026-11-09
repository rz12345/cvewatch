//! 16 任務 5.1～5.2：資產說明的分組清單、新增／改名／刪除，以及資產表單的 ISMS 管理欄位；
//! 19 任務 3.1～3.2：修改資產說明代碼的預覽與寫入。
#![allow(dead_code)]

#[path = "../../core/tests/common/mod.rs"]
mod common;
mod shared;

use common::matching::{at, world, World};
use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::systems::SystemField;
use cvewatch_core::assets::systems_edit::SystemEdit;
use cvewatch_core::assets::SystemCode;
use cvewatch_frontend::asset_form::{AssetForm, ISMS_SYSTEM_MISMATCH};
use cvewatch_frontend::data::{AssetListFilter, Settings};
use cvewatch_frontend::groups::UNASSIGNED_GROUP;
use cvewatch_frontend::test_support::{self, Recorder};
use cvewatch_frontend::workspace::systems::{field_error, field_errors, SystemSaveError};
use cvewatch_frontend::{Context, Level, Workspace};

const NOW: &str = "2026-11-07T00:00:00+00:00";

fn open(w: World) -> Workspace {
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
    Workspace::new(ctx).expect("workspace")
}

fn assign(w: &World, tag: &str, code: &str) {
    w.conn
        .execute(
            "UPDATE asset SET system_code = ?2 WHERE asset_tag = ?1",
            rusqlite::params![tag, code],
        )
        .unwrap();
}

/// NET：FW-01（3 筆未處理命中）、SW-CORE；SYS：只有已退役的 OLD-01；NAS-01 未歸屬。
fn grouped_world() -> World {
    let mut w = world();
    for (code, name) in [("NET", "網路服務"), ("SYS", "核心系統")] {
        cvewatch_core::assets::systems::create_system(&w.conn, code, name).unwrap();
    }
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.0")]);
    w.asset("SW-CORE", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.asset("OLD-01", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.retire("OLD-01");
    assign(&w, "FW-01", "NET");
    assign(&w, "SW-CORE", "NET");
    assign(&w, "OLD-01", "SYS");
    shared::panos_cves(&w, false);
    w.run();
    w
}

fn headings(ws: &Workspace, include_retired: bool) -> Vec<(String, Vec<String>, usize)> {
    ws.data
        .asset_groups(&AssetListFilter {
            include_retired,
            class: None,
            ..AssetListFilter::default()
        })
        .iter()
        .map(|g| {
            (
                g.heading(),
                g.assets
                    .iter()
                    .map(|a| a.asset.record.asset_tag.clone())
                    .collect(),
                g.open_findings,
            )
        })
        .collect()
}

/// Scenario: Assets under their systems / Group with only retired assets
#[test]
fn assets_are_grouped_by_system() {
    let ws = open(grouped_world());
    assert_eq!(
        headings(&ws, false),
        vec![
            (
                "NET 網路服務（2）".to_string(),
                vec!["FW-01".to_string(), "SW-CORE".to_string()],
                3
            ),
            (
                format!("{UNASSIGNED_GROUP}（1）"),
                vec!["NAS-01".to_string()],
                0
            ),
        ]
    );
    let with_retired = headings(&ws, true);
    assert_eq!(with_retired.len(), 3);
    assert_eq!(with_retired[1].0, "SYS 核心系統（1）");
    assert_eq!(with_retired[1].1, vec!["OLD-01".to_string()]);
}

/// Scenario: Adding a system from the window / Duplicate name / System still in use
#[test]
fn systems_are_managed_from_the_workspace() {
    let mut ws = open(grouped_world());
    let r = ws.create_system("lab", "實驗室", &[]).expect("新增");
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    assert_eq!(ws.data.systems.label("LAB"), "LAB 實驗室");
    assert_eq!(ws.system_usage(&SystemCode::parse("LAB").unwrap()), 0);

    let Err(SystemSaveError::Fields(errors)) = ws.create_system("WEB", "網路服務", &[]) else {
        panic!("名稱重複應回報欄位錯誤");
    };
    assert_eq!(
        field_error(&errors, SystemField::Name),
        Some("名稱「網路服務」已被資產說明 NET 使用")
    );
    assert!(ws.data.systems.find("WEB").is_none(), "資料庫不變");

    let net = SystemCode::parse("NET").unwrap();
    assert_eq!(ws.system_usage(&net), 2);
    let r = ws.delete_system(&net, &[]);
    assert_eq!(r.report.level, Level::Error);
    assert!(
        r.report.message.contains("仍有 2 個資產"),
        "{}",
        r.report.message
    );
    let sys = SystemCode::parse("SYS").unwrap();
    assert_eq!(ws.system_usage(&sys), 1, "含已退役資產");

    let edited = ws
        .edit_system(&edit("NET", "NET", "校園網路服務"), None, &[])
        .expect("改名");
    assert_eq!(
        edited.refreshed.report.message,
        "已將資產說明改為 NET 校園網路服務"
    );
    assert!(edited.renamed.is_empty());
    assert_eq!(ws.data.systems.label("NET"), "NET 校園網路服務");
    let lab = SystemCode::parse("LAB").unwrap();
    let r = ws.delete_system(&lab, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    assert!(ws.data.systems.find("LAB").is_none());
}

/// Scenario: ISMS ID from another system（表單層）與管理欄位的儲存
#[test]
fn asset_form_saves_admin_fields() {
    let mut ws = open(grouped_world());
    let mut form = AssetForm::from_entry(ws.data.asset("FW-01").unwrap());
    assert_eq!(form.system, "NET");
    form.isms_id = "SYS-HW-001".into();
    let errors = ws.validate_asset_form(&form).unwrap_err();
    assert_eq!(
        errors.field(StandardField::IsmsId),
        Some(ISMS_SYSTEM_MISMATCH)
    );

    form.isms_id = "NET-HW-001".into();
    form.custodian = "王小明".into();
    form.compliance = "4".into();
    form.personal_data = "Y".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    let saved = &ws.data.asset("FW-01").unwrap().asset.record;
    assert_eq!(saved.custodian.as_deref(), Some("王小明"));
    assert_eq!(saved.protection_level(), Some(4));
    assert!(saved.personal_data);
    assert_eq!(saved.isms_id.as_deref(), Some("NET-HW-001"));
}

fn edit(code: &str, new_code: &str, name: &str) -> SystemEdit {
    SystemEdit {
        code: SystemCode::parse(code).unwrap(),
        new_code: new_code.into(),
        name: name.into(),
    }
}

/// NET：FW-01 改名為 NET-HW-001（ISMS 編號相同）、SW-CORE（ISMS 編號 NET-SW-002）。
fn recode_world() -> World {
    let w = grouped_world();
    w.conn
        .execute(
            "UPDATE asset SET asset_tag = 'NET-HW-001', isms_id = 'NET-HW-001'
             WHERE asset_tag = 'FW-01'",
            [],
        )
        .unwrap();
    w.conn
        .execute(
            "UPDATE asset SET isms_id = 'NET-SW-002' WHERE asset_tag = 'SW-CORE'",
            [],
        )
        .unwrap();
    w
}

/// 19 Scenario: Changing a code cascades to assets（frontend 層：訊息與改名清單）
#[test]
fn changing_a_code_reports_renamed_assets() {
    let mut ws = open(recode_world());
    let plan = ws
        .preview_system_edit(&edit("NET", "nws", "網路服務"))
        .expect("預覽");
    assert_eq!(plan.assets.len(), 2);
    assert!(ws.data.asset("NET-HW-001").is_some(), "預覽不寫入");

    let edited = ws
        .edit_system(&edit("NET", "nws", "網路服務"), None, &[])
        .expect("修改代碼");
    assert_eq!(
        edited.refreshed.report.message,
        "已將資產說明代碼 NET 改為 NWS（2 個資產；ISMS 編號 2 筆、資產編號 1 筆變更）"
    );
    assert_eq!(
        edited.renamed,
        vec![("NET-HW-001".to_string(), "NWS-HW-001".to_string())]
    );
    assert_eq!(ws.data.systems.label("NWS"), "NWS 網路服務");
    let hw = &ws.data.asset("NWS-HW-001").expect("新編號").asset.record;
    assert_eq!(hw.isms_id.as_deref(), Some("NWS-HW-001"));
    assert_eq!(hw.system_code.as_deref(), Some("NWS"));
    assert_eq!(ws.system_usage(&SystemCode::parse("NWS").unwrap()), 2);
}

/// 19 Scenario: New asset tag already used（預覽回報欄位錯誤，資料不變）
#[test]
fn preview_with_conflicts_returns_fields() {
    let w = recode_world();
    w.asset("nws-hw-001", "internal", &[]);
    w.retire("nws-hw-001");
    let mut ws = open(w);
    let Err(SystemSaveError::Fields(errors)) =
        ws.preview_system_edit(&edit("NET", "NWS", "網路服務"))
    else {
        panic!("新編號衝突應回報欄位錯誤");
    };
    assert_eq!(
        field_errors(&errors, SystemField::Code).as_deref(),
        Some("資產編號 NWS-HW-001 已被使用（屬於已退役的資產）")
    );
    let Err(SystemSaveError::Fields(_)) =
        ws.edit_system(&edit("NET", "NWS", "網路服務"), None, &[])
    else {
        panic!("寫入也應拒絕");
    };
    assert_eq!(ws.data.systems.label("NET"), "NET 網路服務", "資料不變");
    assert!(ws.data.asset("NET-HW-001").is_some());

    let Err(SystemSaveError::Fields(errors)) =
        ws.preview_system_edit(&edit("NET", "sys", "網路服務"))
    else {
        panic!("代碼重複應回報欄位錯誤");
    };
    assert_eq!(
        field_error(&errors, SystemField::Code),
        Some("資產說明代碼 SYS 已被使用")
    );
}
