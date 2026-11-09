//! 23 任務 5.3：資產畫面的「匯出 ISMS 清冊」（檔案對話框替身）：匯出、略過的資產、取消、
//! 找不到 cvewatch.exe 時仍可使用。

mod support;

use cvewatch_core::assets::isms::read_isms_workbook;
use cvewatch_core::assets::systems::create_system;
use cvewatch_frontend::Level;
use cvewatch_gui::screens::asset_list::{ISMS_EXPORT_BUTTON, ISMS_IMPORT_BUTTON};
use cvewatch_gui::GuiApp;
use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use rusqlite::params;
use support::*;

const FILE_NAME: &str = "cvewatch-isms-20261107.xlsx";

fn with_isms(w: &common::matching::World, tag: &str, isms_id: &str) {
    w.conn
        .execute(
            "UPDATE asset SET isms_id = ?2, system_code = 'NET' WHERE asset_tag = ?1",
            params![tag, isms_id],
        )
        .unwrap();
}

/// 種子資料的 3 個資產加上 SW-01、SW-02，全部有 ISMS 編號；`srv` 時另有沒有 ISMS 編號的 SRV-01。
fn isms_world(srv: bool) -> common::matching::World {
    let w = seeded_world();
    create_system(&w.conn, "NET", "網路服務").unwrap();
    w.asset("SW-01", "internal", &[("hpe-aoscx", "10.13.1")]);
    w.asset("SW-02", "internal", &[("hpe-aoscx", "10.13.2")]);
    for (i, tag) in ["FW-EDGE", "FW-CORE", "PVE-01", "SW-01", "SW-02"]
        .iter()
        .enumerate()
    {
        with_isms(&w, tag, &format!("NET-HW-{:03}", i + 1));
    }
    if srv {
        w.asset("SRV-01", "internal", &[("vmware-esxi", "8.0")]);
    }
    w
}

fn level(h: &Harness<'_, GuiApp>) -> Level {
    h.state().status.as_ref().unwrap().level
}

/// Scenario: Exporting the ISMS workbook from the window
#[test]
fn exporting_the_isms_workbook_from_the_window() {
    let (mut h, f) = gui(isms_world(false));
    *f.dialogs.save.borrow_mut() = Some(None);
    ctrl(&mut h, egui::Key::Num3);
    let x = |label: &str| h.get_by_label(label).rect().left();
    assert!(
        x(ISMS_EXPORT_BUTTON) > x(ISMS_IMPORT_BUTTON),
        "位於「從 ISMS 清冊匯入」右側"
    );
    click(&mut h, ISMS_EXPORT_BUTTON);
    let path = f.rec.documents_dir().join(FILE_NAME);
    assert_eq!(
        f.dialogs.save_asked.borrow().last().cloned(),
        Some((f.rec.documents_dir(), FILE_NAME.to_string()))
    );
    assert_eq!(
        f.dialogs.xlsx_titles.borrow().as_slice(),
        ["匯出 ISMS 清冊"]
    );
    let book = read_isms_workbook(&path).expect("ISMS 清冊可讀");
    assert_eq!(book.rows.len(), 5);
    let s = status(&h);
    assert!(s.contains("已匯出 5 個資產"), "{s}");
    assert!(s.contains("此檔未加密"), "{s}");
    assert!(s.contains(&path.display().to_string()), "{s}");
    assert_eq!(level(&h), Level::Info, "{s}");
}

/// Scenario: Skipped assets in the window
#[test]
fn skipped_assets_in_the_window() {
    let (mut h, f) = gui(isms_world(true));
    *f.dialogs.save.borrow_mut() = Some(None);
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, ISMS_EXPORT_BUTTON);
    let s = status(&h);
    assert_eq!(level(&h), Level::Warning, "{s}");
    assert!(s.contains("略過 1 個沒有 ISMS 編號的資產：SRV-01"), "{s}");
}

/// Scenario: Cancelling the ISMS export
#[test]
fn cancelling_the_isms_export() {
    let (mut h, f) = gui(isms_world(false));
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, ISMS_EXPORT_BUTTON);
    assert_eq!(status(&h), "已取消匯出 ISMS 清冊");
    let files: Vec<_> = std::fs::read_dir(f.rec.documents_dir()).unwrap().collect();
    assert!(files.is_empty(), "{files:?}");
}

/// Scenario: ISMS export without the companion
#[test]
fn isms_export_without_the_companion() {
    let (mut h, f) = gui_with(
        isms_world(false),
        Setup {
            companion: false,
            ..Setup::default()
        },
    );
    *f.dialogs.save.borrow_mut() = Some(None);
    ctrl(&mut h, egui::Key::Num3);
    assert!(is_disabled(&h, ISMS_IMPORT_BUTTON));
    assert!(!is_disabled(&h, ISMS_EXPORT_BUTTON));
    click(&mut h, ISMS_EXPORT_BUTTON);
    assert!(f.rec.documents_dir().join(FILE_NAME).exists());
    assert!(status(&h).contains("已匯出 5 個資產"), "{}", status(&h));
}
