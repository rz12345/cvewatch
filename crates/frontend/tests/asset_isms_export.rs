//! 23 任務 5.1：`Workspace` 的 ISMS 清冊匯出（預設檔名、完成訊息、略過的資產）。
#![allow(dead_code)]

#[path = "../../core/tests/common/mod.rs"]
mod common;

use common::matching::{at, world, World};
use cvewatch_core::assets::systems::create_system;
use cvewatch_frontend::data::Settings;
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
            reminder_days: cvewatch_core::lifecycle::DEFAULT_REMINDER_DAYS,
        },
        config: cvewatch_core::config::Config::default(),
        config_path: rec.config_path(),
        services: test_support::services(&rec, at(NOW)),
    };
    (Workspace::new(ctx).expect("workspace"), rec)
}

fn with_isms(w: &World, tag: &str, isms_id: &str) {
    w.conn
        .execute(
            "UPDATE asset SET isms_id = ?2, system_code = 'NET' WHERE asset_tag = ?1",
            params![tag, isms_id],
        )
        .unwrap();
}

/// FW-EDGE（NET-HW-001）與 SW-CORE（NET-HW-002）；`srv` 時另有沒有 ISMS 編號的 SRV-01。
fn isms_world(srv: bool) -> World {
    let w = world();
    create_system(&w.conn, "NET", "網路服務").unwrap();
    w.asset("FW-EDGE", "internet", &[("paloalto-panos", "11.1.4-h7")]);
    w.asset("SW-CORE", "internal", &[("hpe-aoscx", "10.13.1")]);
    with_isms(&w, "FW-EDGE", "NET-HW-001");
    with_isms(&w, "SW-CORE", "NET-HW-002");
    if srv {
        w.asset("SRV-01", "internal", &[("vmware-esxi", "8.0")]);
    }
    w
}

/// Scenario: Exporting the ISMS workbook from the window（frontend 部分）
#[test]
fn isms_export_default_and_info_report() {
    let (ws, rec) = open(isms_world(false));
    let (dir, name) = ws.isms_export_default();
    assert_eq!(dir, rec.documents_dir());
    assert_eq!(name, "cvewatch-isms-20261107.xlsx");
    let path = dir.join(&name);
    let r = ws.export_isms(&path);
    assert_eq!(r.level, Level::Info, "{}", r.message);
    assert!(r.message.contains("已匯出 2 個資產"), "{}", r.message);
    assert!(r.message.contains(&path.display().to_string()));
    assert!(r.message.contains("此檔未加密"), "{}", r.message);
    assert!(path.exists());

    let again = ws.export_isms(&path);
    assert_eq!(again.level, Level::Error);
    assert!(again.message.contains("目的檔已存在"), "{}", again.message);
}

/// Scenario: Skipped assets in the window（frontend 部分）
#[test]
fn skipped_assets_are_a_warning() {
    let (ws, rec) = open(isms_world(true));
    let path = rec.documents_dir().join("isms.xlsx");
    let r = ws.export_isms(&path);
    assert_eq!(r.level, Level::Warning, "{}", r.message);
    assert!(r.message.contains("已匯出 2 個資產"), "{}", r.message);
    assert!(
        r.message.contains("略過 1 個沒有 ISMS 編號的資產：SRV-01"),
        "{}",
        r.message
    );
    assert!(r.message.contains("此檔未加密"), "{}", r.message);
}
