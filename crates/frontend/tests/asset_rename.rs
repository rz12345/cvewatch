//! 12 任務 3.2～3.3：`Workspace::save_asset_form` 的更名（只更名不需比對、與其他修改同一交易、
//! 重複編號標在資產編號欄位且不寫入）。
#![allow(dead_code)]

#[path = "../../core/tests/common/mod.rs"]
mod common;

use common::matching::{at, world, World};
use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::matching::run::match_is_stale;
use cvewatch_frontend::asset_form::AssetForm;
use cvewatch_frontend::data::Settings;
use cvewatch_frontend::test_support::{self, Recorder};
use cvewatch_frontend::{Context, Level, Workspace};

const NOW: &str = "2026-11-08T00:00:00+00:00";

fn open(w: World) -> (Workspace, Recorder) {
    let rec = Recorder::default();
    std::fs::create_dir_all(rec.documents_dir()).unwrap();
    let ctx = Context {
        conn: w.conn,
        settings: Settings {
            catalog: w.catalog,
            scoring: w.settings.scoring.clone(),
            stale_days: w.settings.stale_days,
        },
        config: cvewatch_core::config::Config::default(),
        config_path: rec.config_path(),
        services: test_support::services(&rec, at(NOW)),
    };
    (Workspace::new(ctx).expect("workspace"), rec)
}

/// FW-01 與已退役的 NAS-02；「比對待補」已清除。
fn assets_world() -> World {
    let mut w = world();
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4-h7")]);
    w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.retire("NAS-02");
    w.run_at(NOW);
    w
}

fn edit_form(ws: &Workspace, tag: &str) -> AssetForm {
    AssetForm::from_entry(ws.data.asset(tag).expect("asset"))
}

fn tag_in_db(ws: &Workspace, tag: &str) -> bool {
    ws.conn
        .query_row(
            "SELECT count(*) FROM asset WHERE asset_tag = ?1",
            [tag],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
        == 1
}

#[test]
fn rename_only_does_not_need_matching() {
    let (mut ws, _rec) = open(assets_world());
    let mut form = edit_form(&ws, "FW-01");
    form.asset_tag = " FW-11 ".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    assert_eq!(r.report.message, "已儲存 FW-11（更名 FW-01 → FW-11）");
    assert!(!r.needs_match);
    assert!(!match_is_stale(&ws.conn).unwrap());
    assert!(ws.data.asset("FW-11").is_some(), "重新載入後以新編號列出");
    assert!(ws.data.asset("FW-01").is_none());
}

#[test]
fn rename_with_field_change_saves_both_and_needs_matching() {
    let (mut ws, _rec) = open(assets_world());
    let mut form = edit_form(&ws, "FW-01");
    form.asset_tag = "FW-11".into();
    form.name = "fw11".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    assert_eq!(
        r.report.message,
        "已儲存 FW-11（更名 FW-01 → FW-11、修改資產欄位）"
    );
    assert!(r.needs_match);
    assert_eq!(ws.data.asset("FW-11").unwrap().asset.record.name, "fw11");
}

#[test]
fn field_change_alone_is_not_a_rename() {
    let (mut ws, _rec) = open(assets_world());
    let mut form = edit_form(&ws, "FW-01");
    form.name = "fw01".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.message, "已儲存 FW-01（修改資產欄位）");
}

#[test]
fn renaming_to_a_retired_assets_tag_is_a_field_error() {
    let (mut ws, _rec) = open(assets_world());
    let mut form = edit_form(&ws, "FW-01");
    form.asset_tag = "nas-02".into();
    form.name = "fw11".into();
    let errors = ws.validate_asset_form(&form).unwrap_err();
    assert_eq!(
        errors.field(StandardField::AssetTag),
        Some("已被使用（已退役的 NAS-02）"),
        "錯誤標在資產編號欄位"
    );
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Error);
    assert!(!r.needs_match);
    assert_eq!(r.report.message, "資產編號：已被使用（已退役的 NAS-02）");
    assert!(tag_in_db(&ws, "FW-01"));
    assert_eq!(ws.data.asset("FW-01").unwrap().asset.record.name, "FW-01");
}

#[test]
fn renaming_to_an_active_assets_tag_is_a_field_error() {
    let w = assets_world();
    w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.6")]);
    let (ws, _rec) = open(w);
    let mut form = edit_form(&ws, "FW-01");
    form.asset_tag = "fw-02".into();
    let errors = ws.validate_asset_form(&form).unwrap_err();
    assert_eq!(
        errors.field(StandardField::AssetTag),
        Some("已被使用（FW-02）")
    );
}

#[test]
fn changing_only_the_letter_case_is_not_a_conflict() {
    let (mut ws, _rec) = open(assets_world());
    let mut form = edit_form(&ws, "FW-01");
    form.asset_tag = "fw-01".into();
    assert_eq!(ws.validate_asset_form(&form), Ok(()));
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    assert!(tag_in_db(&ws, "fw-01"));
}
