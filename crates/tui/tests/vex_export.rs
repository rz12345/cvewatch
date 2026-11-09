//! 07 任務 4.2：Findings 畫面 `E` — 路徑對話框（預設文件資料夾）、依目前篩選匯出、狀態列提示。

mod support;

use std::path::PathBuf;

use cvewatch_tui::dialog::Dialog;
use ratatui::crossterm::event::KeyCode;
use serde_json::Value;
use support::common::vex::schema_errors;
use support::{app, keys, press, render, seeded_world};

/// 測試時鐘 2026-11-07 00:00（UTC 顯示）產生的預設檔名。
const DEFAULT_NAME: &str = "cvewatch-vex-20261107-0000.json";

fn open_findings() -> (cvewatch_tui::App, support::Recorder) {
    let (mut app, rec) = app(seeded_world());
    std::fs::create_dir_all(rec.documents_dir()).unwrap();
    keys(&mut app, "2");
    (app, rec)
}

/// 只看某資產（與篩選面板的「資產」列相同）。
fn filter_asset(app: &mut cvewatch_tui::App, tag: &str) {
    app.findings.filter = app.findings.filter.with_asset(Some(tag.to_string()));
    app.refresh_visible();
}

fn read_json(path: &PathBuf) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn e_opens_path_dialog_in_documents_folder() {
    let (mut app, _) = open_findings();
    // 固定路徑讓快照穩定（實際匯出的測試使用臨時資料夾）
    app.services.documents_dir = PathBuf::from(r"C:\Users\me\Documents");
    filter_asset(&mut app, "FW-EDGE");
    press(&mut app, KeyCode::Char('E'));
    let Some(Dialog::ExportVex { input, count }) = &app.dialog else {
        panic!("expected export dialog");
    };
    assert_eq!(*count, app.findings.visible.len());
    assert_eq!(
        input.value(),
        format!(
            r"C:\Users\me\Documents{}{DEFAULT_NAME}",
            std::path::MAIN_SEPARATOR
        )
    );
    insta::assert_snapshot!(render(&app));
}

#[test]
fn exporting_filtered_findings_writes_only_that_asset() {
    let (mut app, rec) = open_findings();
    filter_asset(&mut app, "FW-EDGE");
    let expected = app.findings.visible.len();
    assert!(expected > 0);
    press(&mut app, KeyCode::Char('E'));
    press(&mut app, KeyCode::Enter);
    let path = rec.documents_dir().join(DEFAULT_NAME);
    let doc = read_json(&path);
    assert!(schema_errors(&doc).is_empty());
    let vulns = doc["vulnerabilities"].as_array().unwrap();
    assert_eq!(vulns.len(), expected);
    assert!(vulns
        .iter()
        .all(|v| v["affects"][0]["ref"] == "cvewatch:FW-EDGE:paloalto-panos"));
    assert_eq!(doc["components"].as_array().unwrap().len(), 1);
    let message = app.message.clone().unwrap();
    assert!(message.contains(&path.display().to_string()), "{message}");
    assert!(message.contains("此檔未加密"), "{message}");
    assert!(app.dialog.is_none());
}

#[test]
fn search_is_part_of_the_current_filter() {
    let (mut app, rec) = open_findings();
    keys(&mut app, "/2026-0001");
    press(&mut app, KeyCode::Enter);
    let expected = app.findings.visible.len();
    press(&mut app, KeyCode::Char('E'));
    press(&mut app, KeyCode::Enter);
    let doc = read_json(&rec.documents_dir().join(DEFAULT_NAME));
    let vulns = doc["vulnerabilities"].as_array().unwrap();
    assert_eq!(vulns.len(), expected);
    assert!(vulns.iter().all(|v| v["id"] == "CVE-2026-0001"));
}

#[test]
fn edited_path_is_used() {
    let (mut app, rec) = open_findings();
    press(&mut app, KeyCode::Char('E'));
    let Some(Dialog::ExportVex { input, .. }) = &app.dialog else {
        panic!("expected export dialog");
    };
    let len = input.value().chars().count();
    for _ in 0..len {
        press(&mut app, KeyCode::Backspace);
    }
    let target = rec.dir.path().join("team.json");
    keys(&mut app, &target.display().to_string());
    press(&mut app, KeyCode::Enter);
    assert!(target.exists());
    assert!(!rec.documents_dir().join(DEFAULT_NAME).exists());
}

#[test]
fn nothing_visible_does_not_open_dialog() {
    let (mut app, _) = open_findings();
    keys(&mut app, "/no-such-finding");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('E'));
    assert!(app.dialog.is_none());
    assert_eq!(app.message.as_deref(), Some("目前篩選沒有命中，未匯出"));
}

#[test]
fn second_export_in_same_minute_gets_suffix() {
    let (mut app, rec) = open_findings();
    let first = rec.documents_dir().join(DEFAULT_NAME);
    std::fs::write(&first, "keep").unwrap();
    press(&mut app, KeyCode::Char('E'));
    press(&mut app, KeyCode::Enter);
    let second = rec
        .documents_dir()
        .join(DEFAULT_NAME.replace(".json", "-2.json"));
    assert!(second.exists(), "{:?}", app.message);
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "keep");
}

#[test]
fn existing_file_typed_by_user_is_not_overwritten() {
    let (mut app, rec) = open_findings();
    let target = rec.dir.path().join("team.json");
    std::fs::write(&target, "keep").unwrap();
    press(&mut app, KeyCode::Char('E'));
    let Some(Dialog::ExportVex { input, .. }) = &app.dialog else {
        panic!("expected export dialog");
    };
    for _ in 0..input.value().chars().count() {
        press(&mut app, KeyCode::Backspace);
    }
    keys(&mut app, &target.display().to_string());
    press(&mut app, KeyCode::Enter);
    let message = app.message.clone().unwrap();
    assert!(message.starts_with("匯出 VEX 失敗"), "{message}");
    assert!(message.contains("已存在"), "{message}");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
}

#[test]
fn esc_cancels_without_writing() {
    let (mut app, rec) = open_findings();
    press(&mut app, KeyCode::Char('E'));
    press(&mut app, KeyCode::Esc);
    assert!(app.dialog.is_none());
    assert!(!rec.documents_dir().join(DEFAULT_NAME).exists());
}
