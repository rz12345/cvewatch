//! 06 任務 6.7：設定畫面（spec tui-console）— 三個分頁、編輯與儲存（toml_edit 保留註解）、
//! 秘密只進憑證庫、即時驗證、測試按鍵使用未儲存值、Redmine 清單挑選、未儲存確認、明文警示、
//! 排程變更提示。

mod support;

use cvewatch_core::secrets::{SecretKey, SecretStore};
use cvewatch_tui::settings::{FieldId, RedmineLists, Row, Tab};
use cvewatch_tui::{glyphs, App, Screen};
use ratatui::crossterm::event::KeyCode;
use support::{app_with_config, ctrl, empty_world, keys, named, press, render, Recorder};

const CONFIG: &str = r#"# 使用者的設定檔
[notify]
urgent_cvss = 9.0

[notify.email]
enabled = true
smtp_host = "smtp.example.edu.tw"   # 機構 SMTP
smtp_port = 587
security = "starttls"
username = "rz12345"
from = "cvewatch <rz12345@example.edu.tw>"
to = ["rz12345@example.edu.tw"]

[notify.redmine]
enabled = true
base_url = "https://redmine.example.edu.tw"
project_id = "infra-sec"
tracker_id = 1
priority_map = { P1 = 5, P2 = 4 }
assigned_to_id = 0
add_note_on_triage = true   # 狀態變更時加註記

[schedule]
sync_every_hours = 4
digest_at = "08:30"
"#;

fn open() -> (App, Recorder) {
    let (mut app, rec) = app_with_config(empty_world(), glyphs::UNICODE, CONFIG);
    keys(&mut app, "6");
    (app, rec)
}

/// 移到欄位所在的列。
fn goto(app: &mut App, row: Row) {
    let tab = Tab::ALL
        .into_iter()
        .find(|t| app.page.form.rows(*t).contains(&row))
        .expect("row in some tab");
    while app.page.tab != tab {
        press(app, KeyCode::Tab);
    }
    let idx = app.page.rows().iter().position(|r| *r == row).expect("row");
    press(app, KeyCode::Home);
    for _ in 0..idx {
        press(app, KeyCode::Down);
    }
}

/// 以新值取代文字欄位。
fn type_into(app: &mut App, row: Row, text: &str) {
    goto(app, row);
    press(app, KeyCode::Enter);
    for _ in 0..80 {
        press(app, KeyCode::Backspace);
    }
    keys(app, text);
    press(app, KeyCode::Enter);
}

fn tick(app: &mut App) {
    app.on_tick(std::time::Instant::now());
}

#[test]
fn key_6_opens_email_tab_with_config_values() {
    let (app, _) = open();
    assert_eq!(app.screen, Screen::Settings);
    assert_eq!(app.page.tab, Tab::Email);
    insta::assert_snapshot!(render(&app));
}

#[test]
fn redmine_and_schedule_tabs() {
    let (mut app, _) = open();
    press(&mut app, KeyCode::Tab);
    insta::assert_snapshot!("redmine_tab", render(&app));
    press(&mut app, KeyCode::Tab);
    insta::assert_snapshot!("schedule_tab", render(&app));
}

#[test]
fn changing_smtp_host_keeps_other_content_and_comments() {
    let (mut app, rec) = open();
    type_into(
        &mut app,
        Row::Field(FieldId::SmtpHost),
        "mail.example.edu.tw",
    );
    assert!(app.page.dirty());
    ctrl(&mut app, 's');
    assert!(!app.page.dirty());
    let expected = CONFIG.replace("\"smtp.example.edu.tw\"", "\"mail.example.edu.tw\"");
    assert_eq!(rec.config_text(), expected, "只改主機，其他內容與註解不變");
}

#[test]
fn toggling_triage_notes_writes_false() {
    let (mut app, rec) = open();
    goto(&mut app, Row::Field(FieldId::NoteOnTriage));
    press(&mut app, KeyCode::Char(' '));
    ctrl(&mut app, 's');
    assert!(rec
        .config_text()
        .contains("add_note_on_triage = false   # 狀態變更時加註記"));
}

#[test]
fn invalid_port_is_marked_and_not_saved() {
    let (mut app, rec) = open();
    type_into(&mut app, Row::Field(FieldId::SmtpPort), "70000");
    ctrl(&mut app, 's');
    assert_eq!(rec.config_text(), CONFIG);
    assert!(app.message.as_deref().unwrap().contains("未儲存"));
    assert!(app.page.errors().contains_key(&FieldId::SmtpPort));
    insta::assert_snapshot!(render(&app));
}

#[test]
fn http_url_is_rejected_in_console() {
    let (mut app, _) = open();
    type_into(
        &mut app,
        Row::Field(FieldId::BaseUrl),
        "http://redmine.local",
    );
    let err = app.page.errors()[&FieldId::BaseUrl].clone();
    assert!(err.contains("https"), "{err}");
}

#[test]
fn api_key_is_saved_only_to_credential_store() {
    let (mut app, rec) = open();
    goto(&mut app, Row::Field(FieldId::ApiKey));
    press(&mut app, KeyCode::Enter);
    keys(&mut app, "super-secret-key");
    let screen = render(&app);
    assert!(!screen.contains("super-secret-key"), "輸入時遮罩");
    press(&mut app, KeyCode::Enter);
    ctrl(&mut app, 's');
    assert_eq!(
        rec.secrets
            .get(SecretKey::RedmineApiKey)
            .unwrap()
            .as_deref()
            .map(String::as_str),
        Some("super-secret-key")
    );
    assert!(!rec.config_text().contains("super-secret-key"));
    assert!(render(&app).contains("已設定"));
    assert!(!render(&app).contains("super-secret-key"));
}

#[test]
fn clearing_smtp_password_deletes_it() {
    let (mut app, rec) = app_with_config(empty_world(), glyphs::UNICODE, CONFIG);
    rec.secrets.set(SecretKey::SmtpPassword, "pw").unwrap();
    keys(&mut app, "6");
    assert!(render(&app).contains("已設定"));
    goto(&mut app, Row::Field(FieldId::Password));
    press(&mut app, KeyCode::Delete);
    ctrl(&mut app, 's');
    assert!(rec.secrets.get(SecretKey::SmtpPassword).unwrap().is_none());
    assert!(render(&app).contains("未設定"));
}

#[test]
fn test_email_uses_unsaved_values_in_background() {
    let (mut app, rec) = open();
    type_into(&mut app, Row::Field(FieldId::SmtpPort), "465");
    goto(&mut app, Row::Field(FieldId::Security));
    press(&mut app, KeyCode::Enter);
    keys(&mut app, "t");
    assert_eq!(app.page.jobs.len(), 1, "背景執行");
    tick(&mut app);
    assert!(app.page.jobs.is_empty());
    assert_eq!(
        rec.backend.calls.lock().unwrap().as_slice(),
        ["test_email smtp.example.edu.tw:465 tls password=false"]
    );
    assert_eq!(app.message.as_deref(), Some("已寄出測試信"));
    assert_eq!(rec.config_text(), CONFIG, "設定檔不變");
}

#[test]
fn test_failure_is_shown() {
    let (mut app, rec) = open();
    *rec.backend.redmine.lock().unwrap() = Some(Err("Redmine HTTP 401（API key 無效）".into()));
    press(&mut app, KeyCode::Tab);
    keys(&mut app, "t");
    tick(&mut app);
    let msg = app.message.clone().unwrap();
    assert!(
        msg.contains("測試 Redmine失敗") && msg.contains("401"),
        "{msg}"
    );
}

#[test]
fn pick_project_and_tracker_from_redmine_lists() {
    let (mut app, rec) = open();
    rec.secrets.set(SecretKey::RedmineApiKey, "k").unwrap();
    *rec.backend.lists.lock().unwrap() = Some(Ok(RedmineLists {
        projects: Some(vec![named(7, "其他專案"), named(12, "基礎設施資安")]),
        priorities: Some(vec![named(4, "高"), named(5, "緊急")]),
        ..RedmineLists::default()
    }));
    *rec.backend.project_lists.lock().unwrap() = Some(Ok(RedmineLists {
        trackers: Some(vec![named(1, "Bug"), named(3, "漏洞")]),
        members: Some(vec![named(5, "Rz Chen")]),
        ..RedmineLists::default()
    }));
    press(&mut app, KeyCode::Tab);
    keys(&mut app, "l");
    tick(&mut app);
    tick(&mut app);
    goto(&mut app, Row::Field(FieldId::Project));
    press(&mut app, KeyCode::Enter);
    assert!(app.page.picker.is_some());
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    tick(&mut app);
    goto(&mut app, Row::Field(FieldId::Tracker));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    let screen = render(&app);
    assert!(screen.contains("基礎設施資安 (#12)"), "{screen}");
    assert!(screen.contains("漏洞 (#3)"), "{screen}");
    ctrl(&mut app, 's');
    let text = rec.config_text();
    assert!(text.contains("project_id = 12"), "{text}");
    assert!(text.contains("tracker_id = 3"), "{text}");
    let calls = rec.backend.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        [
            "lists None",
            "lists Some(\"infra-sec\")",
            "lists Some(\"12\")"
        ]
    );
}

#[test]
fn unreachable_redmine_falls_back_to_manual_ids() {
    let (mut app, rec) = open();
    rec.secrets.set(SecretKey::RedmineApiKey, "k").unwrap();
    *rec.backend.lists.lock().unwrap() = Some(Err("無法連線 Redmine：connection refused".into()));
    press(&mut app, KeyCode::Tab);
    keys(&mut app, "l");
    tick(&mut app);
    assert!(app
        .message
        .as_deref()
        .unwrap()
        .contains("可直接輸入數字 ID"));
    type_into(&mut app, Row::Field(FieldId::Tracker), "9");
    assert_eq!(app.page.form.tracker, "9");
    assert!(app.page.picker.is_none());
}

#[test]
fn leaving_with_unsaved_changes_asks_save_discard_cancel() {
    let (mut app, rec) = open();
    type_into(&mut app, Row::Recipient(0), "noc@example.edu.tw");
    press(&mut app, KeyCode::Esc);
    assert!(app.page.confirm.is_some());
    insta::assert_snapshot!(render(&app));
    keys(&mut app, "c");
    assert_eq!(app.screen, Screen::Settings, "取消：留在設定畫面");
    keys(&mut app, "2");
    assert!(app.page.confirm.is_some(), "切換畫面也要確認");
    keys(&mut app, "d");
    assert_eq!(app.screen, Screen::Findings);
    assert_eq!(rec.config_text(), CONFIG, "放棄：設定檔不變");
}

#[test]
fn quitting_with_unsaved_changes_can_save_first() {
    let (mut app, rec) = open();
    type_into(&mut app, Row::Recipient(0), "noc@example.edu.tw");
    keys(&mut app, "q");
    assert!(!app.quit && app.page.confirm.is_some());
    keys(&mut app, "s");
    assert!(app.quit);
    assert!(rec.config_text().contains("to = [\"noc@example.edu.tw\"]"));
}

#[test]
fn adding_and_removing_recipients() {
    let (mut app, _) = open();
    type_into(&mut app, Row::AddRecipient, "noc@example.edu.tw");
    assert_eq!(app.page.form.to.len(), 2);
    goto(&mut app, Row::Recipient(0));
    press(&mut app, KeyCode::Delete);
    assert_eq!(app.page.form.to, ["noc@example.edu.tw"]);
}

#[test]
fn insecure_exception_shows_banner_on_every_screen() {
    let cfg = CONFIG.replace(
        "[notify]\nurgent_cvss = 9.0\n",
        "[notify]\nurgent_cvss = 9.0\nallow_insecure_transport = true\n",
    );
    let (mut app, _) = app_with_config(empty_world(), glyphs::UNICODE, &cfg);
    for key in ["1", "2", "3", "4", "5", "6"] {
        keys(&mut app, key);
        assert!(render(&app).contains("明文傳輸已啟用"), "畫面 {key}");
    }
    let (plain, _) = open();
    assert!(!render(&plain).contains("明文傳輸已啟用"));
}

#[test]
fn schedule_change_prompts_reinstall() {
    let (mut app, rec) = open();
    type_into(&mut app, Row::Field(FieldId::DigestAt), "07:45");
    keys(&mut app, "i");
    assert!(app.message.as_deref().unwrap().contains("請先按 Ctrl+S"));
    ctrl(&mut app, 's');
    assert!(app
        .message
        .as_deref()
        .unwrap()
        .contains("按 i 重新安裝排程"));
    assert!(rec.config_text().contains("digest_at = \"07:45\""));
    keys(&mut app, "i");
    tick(&mut app);
    assert!(!app.page.schedule_pending);
    assert_eq!(
        rec.backend.calls.lock().unwrap().as_slice(),
        ["install_schedule"]
    );
}

#[test]
fn switching_project_quickly_ignores_stale_tracker_lists() {
    let (mut app, rec) = open();
    rec.secrets.set(SecretKey::RedmineApiKey, "k").unwrap();
    *rec.backend.lists.lock().unwrap() = Some(Ok(RedmineLists {
        projects: Some(vec![named(7, "A 專案"), named(12, "B 專案")]),
        ..RedmineLists::default()
    }));
    for (project, tracker) in [("7", named(1, "A-only")), ("12", named(3, "漏洞"))] {
        rec.backend.lists_by_project.lock().unwrap().insert(
            project.into(),
            RedmineLists {
                trackers: Some(vec![tracker]),
                members: Some(Vec::new()),
                ..RedmineLists::default()
            },
        );
    }
    press(&mut app, KeyCode::Tab);
    keys(&mut app, "l");
    tick(&mut app);
    tick(&mut app);
    // 連續選 A 再選 B，A 的結果較晚取回
    goto(&mut app, Row::Field(FieldId::Project));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::Enter);
    tick(&mut app);
    assert_eq!(app.page.form.project, "12");
    let trackers: Vec<String> = app
        .page
        .lists
        .trackers
        .clone()
        .unwrap()
        .iter()
        .map(|t| t.label())
        .collect();
    assert_eq!(trackers, ["漏洞 (#3)"], "只採用目前專案 B 的清單");
}

#[test]
fn discarded_stale_project_lists_still_trigger_redraw() {
    let (mut app, rec) = open();
    rec.secrets.set(SecretKey::RedmineApiKey, "k").unwrap();
    *rec.backend.lists.lock().unwrap() = Some(Ok(RedmineLists {
        projects: Some(vec![named(7, "A 專案")]),
        ..RedmineLists::default()
    }));
    press(&mut app, KeyCode::Tab);
    keys(&mut app, "l");
    tick(&mut app);
    tick(&mut app);
    goto(&mut app, Row::Field(FieldId::Project));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.page.jobs.len(), 1, "A 的 tracker 清單載入中");
    // 結果取回前專案已改為別的值：A 的結果被捨棄，但「載入中」的提示消失，仍需重繪
    app.page.form = app.page.form.with_text(FieldId::Project, "12");
    assert!(app.on_tick(std::time::Instant::now()));
    assert!(app.page.jobs.is_empty());
}

#[test]
fn credential_store_failure_saves_nothing() {
    let (mut app, rec) = open();
    type_into(
        &mut app,
        Row::Field(FieldId::SmtpHost),
        "mail.example.edu.tw",
    );
    goto(&mut app, Row::Field(FieldId::Password));
    press(&mut app, KeyCode::Enter);
    keys(&mut app, "pw");
    press(&mut app, KeyCode::Enter);
    rec.store_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    ctrl(&mut app, 's');
    assert!(app.message.as_deref().unwrap().contains("設定未儲存"));
    assert_eq!(rec.config_text(), CONFIG, "設定檔不變");
    assert!(app.page.dirty());
    rec.store_fails
        .store(false, std::sync::atomic::Ordering::SeqCst);
    ctrl(&mut app, 's');
    assert!(!app.page.dirty());
    assert!(rec.config_text().contains("mail.example.edu.tw"));
    assert!(rec.secrets.is_set(SecretKey::SmtpPassword).unwrap());
}

#[test]
fn unreadable_credential_store_is_reported_when_opening_settings() {
    let (mut app, rec) = app_with_config(empty_world(), glyphs::UNICODE, CONFIG);
    rec.store_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    keys(&mut app, "6");
    assert!(app
        .message
        .as_deref()
        .unwrap()
        .contains("無法讀取 Credential Manager"));
}

#[test]
fn dashboard_shows_undeliverable_notifications() {
    let (mut app, _) = app_with_config(empty_world(), glyphs::UNICODE, CONFIG);
    let now = support::common::matching::at(support::NOW);
    let id = cvewatch_core::notify::outbox::enqueue(
        &app.conn,
        cvewatch_core::notify::outbox::Channel::Redmine,
        "create_issue",
        "{}",
        now,
    )
    .unwrap();
    cvewatch_core::notify::outbox::fail(&app.conn, id, "HTTP 422", true, now).unwrap();
    keys(&mut app, "R");
    assert!(render(&app).contains("無法送出的通知 1"));
}
