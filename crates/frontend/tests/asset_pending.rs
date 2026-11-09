//! 18 任務 6.1～6.2：版本待補的顯示、資產篩選、Dashboard 計數與編輯表單（未修改的待補元件可儲存、
//! 已知版本清空與新元件未填版本為「必填」）。
#![allow(dead_code)]

#[path = "../../core/tests/common/mod.rs"]
mod common;

use common::matching::{at, world, World};
use cvewatch_frontend::asset_form::{AssetForm, VERSION_REQUIRED};
use cvewatch_frontend::data::{AssetListFilter, Settings};
use cvewatch_frontend::display::{
    component_line, pending_versions_text, version_text, PENDING_VERSION,
};
use cvewatch_frontend::test_support::{self, Recorder};
use cvewatch_frontend::{Context, Level, Workspace};

const NOW: &str = "2026-11-09T00:00:00+00:00";

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
            reminder_days: cvewatch_core::lifecycle::DEFAULT_REMINDER_DAYS,
        },
        config: cvewatch_core::config::Config::default(),
        config_path: rec.config_path(),
        services: test_support::services(&rec, at(NOW)),
    };
    Workspace::new(ctx).expect("workspace")
}

/// NET-HW-003（DSM 版本待補）、FW-01（PAN-OS 11.1.6）。
fn pending_world() -> World {
    let w = world();
    w.asset("NET-HW-003", "internal", &[("synology-dsm", "")]);
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.6")]);
    w
}

fn edit_form(ws: &Workspace, tag: &str) -> AssetForm {
    AssetForm::from_entry(ws.data.asset(tag).expect("asset"))
}

#[test]
fn pending_component_reads_pending_version() {
    let ws = open(pending_world());
    let catalog = &ws.settings.catalog;
    let nas = ws.data.asset("NET-HW-003").unwrap();
    assert!(nas.has_pending());
    assert_eq!(
        component_line(catalog, &nas.components[0]),
        "DiskStation Manager 版本待補（未納入比對）"
    );
    let fw = ws.data.asset("FW-01").unwrap();
    assert!(!fw.has_pending());
    assert_eq!(component_line(catalog, &fw.components[0]), "PAN-OS 11.1.6");
    assert_eq!(version_text(" "), PENDING_VERSION);
    assert_eq!(version_text("3.2"), "3.2");
}

#[test]
fn pending_only_filter_and_dashboard_count() {
    let ws = open(pending_world());
    let tags = |pending_only| -> Vec<String> {
        ws.data
            .asset_list(&AssetListFilter {
                pending_only,
                ..AssetListFilter::default()
            })
            .iter()
            .map(|a| a.asset.record.asset_tag.clone())
            .collect()
    };
    assert_eq!(tags(false), ["FW-01", "NET-HW-003"]);
    assert_eq!(tags(true), ["NET-HW-003"]);
    assert_eq!(
        pending_versions_text(&ws.data.dashboard.pending_versions),
        "版本待補 1 個元件（1 個資產），未納入比對"
    );
    assert_eq!(pending_versions_text(&Default::default()), "版本待補 0");
}

#[test]
fn saving_other_fields_with_a_pending_component() {
    let mut ws = open(pending_world());
    let mut form = edit_form(&ws, "NET-HW-003");
    form.custodian = "李小華".into();
    assert_eq!(ws.validate_asset_form(&form), Ok(()));
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    let nas = ws.data.asset("NET-HW-003").unwrap();
    assert_eq!(nas.asset.record.custodian.as_deref(), Some("李小華"));
    assert!(nas.components[0].is_pending());
}

#[test]
fn filling_a_pending_version_on_screen() {
    let mut ws = open(pending_world());
    let mut form = edit_form(&ws, "NET-HW-003");
    form.components[0].version = "7.2.2-72806".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    assert!(r.needs_match, "填上版本後自動比對");
    let nas = ws.data.asset("NET-HW-003").unwrap();
    assert_eq!(nas.components[0].version_raw, "7.2.2-72806");
}

#[test]
fn clearing_a_known_version_shows_required() {
    let ws = open(pending_world());
    let mut form = edit_form(&ws, "FW-01");
    form.components[0].version = " ".into();
    let e = ws.validate_asset_form(&form).unwrap_err();
    assert_eq!(e.component(0), vec![VERSION_REQUIRED]);

    // 新加入的元件未填版本也是必填
    let mut form = edit_form(&ws, "NET-HW-003");
    form.add_product("dell-idrac9");
    let e = ws.validate_asset_form(&form).unwrap_err();
    assert_eq!(e.component(1), vec![VERSION_REQUIRED]);
    assert!(e.component(0).is_empty(), "未修改的待補元件不擋儲存");
}
