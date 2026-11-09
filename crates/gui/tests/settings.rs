//! 設定畫面：即時驗證、Redmine 清單挑選、排程變更提示、儲存與測試、關閉保護。

mod support;

use cvewatch_core::secrets::{SecretKey, SecretStore};
use cvewatch_frontend::settings::RedmineLists;
use cvewatch_gui::app::Leave;
use cvewatch_gui::settings::schedule::SCHEDULE_CHANGED;
use cvewatch_gui::{GuiApp, Screen};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use support::*;

fn settings_gui() -> (Harness<'static, GuiApp>, Fakes) {
    let (mut h, f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num6);
    assert_eq!(h.state().screen, Screen::Settings);
    (h, f)
}

/// 取代欄位內容：取得焦點、全選、輸入。
fn replace(h: &mut Harness<'_, GuiApp>, label: &str, text: &str) {
    h.get_by_label(label).focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_label(label).type_text(text);
    h.run();
}

fn tab(h: &mut Harness<'_, GuiApp>, title: &str) {
    h.get_by_label(title).click();
    h.run();
}

/// 送出視窗關閉要求（標題列的關閉鈕、Alt+F4）。
fn request_close(h: &mut Harness<'_, GuiApp>) {
    h.input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    h.run();
}

#[test]
fn invalid_port_is_marked_and_save_disabled() {
    let (mut h, f) = settings_gui();
    let before = f.rec.config_text();
    replace(&mut h, "連接埠", "70000");
    assert_eq!(h.state().settings.form.smtp_port, "70000");
    assert!(shows(&h, "連接埠："), "欄位標示錯誤");
    assert!(is_disabled(&h, "儲存"));
    assert!(h.query_by_label("無加密").is_none(), "沒有無加密選項");
    assert!(h.query_by_label("STARTTLS").is_some() && h.query_by_label("TLS").is_some());
    assert_eq!(f.rec.config_text(), before, "設定檔不變");
}

#[test]
fn redmine_lists_are_picked_by_name_and_saved_as_ids() {
    let (mut h, f) = settings_gui();
    *f.rec.backend.lists.lock().unwrap() = Some(Ok(RedmineLists {
        projects: Some(vec![named(12, "基礎設施資安"), named(15, "網路組")]),
        priorities: Some(vec![named(5, "Urgent"), named(4, "High")]),
        ..RedmineLists::default()
    }));
    f.rec.backend.lists_by_project.lock().unwrap().insert(
        "12".into(),
        RedmineLists {
            trackers: Some(vec![named(3, "漏洞"), named(1, "Bug")]),
            members: Some(vec![named(7, "Rz Chen")]),
            ..RedmineLists::default()
        },
    );
    tab(&mut h, "Redmine");
    replace(&mut h, "網址", "https://redmine.example.edu.tw");
    replace(&mut h, "API key", "k-123");
    click(&mut h, "從 Redmine 載入");
    h.get_by_label("專案").click();
    h.run();
    click_option(&mut h, "基礎設施資安 (#12)");
    h.get_by_label("Tracker").click();
    h.run();
    click_option(&mut h, "漏洞 (#3)");
    assert_eq!(h.state().settings.form.project, "12");
    assert_eq!(h.state().settings.form.tracker, "3");
    // 下拉選單顯示「名稱 (#ID)」
    assert_eq!(
        h.get_by_label("專案").value().as_deref(),
        Some("基礎設施資安 (#12)")
    );
    assert_eq!(
        h.get_by_label("Tracker").value().as_deref(),
        Some("漏洞 (#3)")
    );
    click(&mut h, "儲存");
    let text = f.rec.config_text();
    assert!(text.contains("project_id = 12"), "{text}");
    assert!(text.contains("tracker_id = 3"), "{text}");
    assert!(!text.contains("k-123"), "API key 不寫入設定檔");
    let stored = f.rec.secrets.get(SecretKey::RedmineApiKey).unwrap();
    assert_eq!(stored.as_deref().map(String::as_str), Some("k-123"));
    assert!(shows(&h, "已設定"));
    assert!(!h.state().settings.dirty());
}

#[test]
fn lists_failure_falls_back_to_numeric_input() {
    let (mut h, f) = settings_gui();
    *f.rec.backend.lists.lock().unwrap() = Some(Err("連線逾時".into()));
    tab(&mut h, "Redmine");
    replace(&mut h, "網址", "https://redmine.example.edu.tw");
    replace(&mut h, "API key", "k-123");
    click(&mut h, "從 Redmine 載入");
    assert!(shows(&h, "無法載入 Redmine 清單：連線逾時"));
    let project = h.get_by_label("專案");
    assert_eq!(
        project.accesskit_node().role(),
        egui::accesskit::Role::TextInput
    );
    replace(&mut h, "專案", "12");
    assert_eq!(h.state().settings.form.project, "12");
}

#[test]
fn digest_time_change_prompts_schedule_reinstall() {
    let (mut h, f) = settings_gui();
    tab(&mut h, "排程與門檻");
    assert_eq!(h.state().settings.form.digest_at, "08:30");
    replace(&mut h, "每日摘要時間", "07:45");
    click(&mut h, "儲存");
    assert!(f.rec.config_text().contains("digest_at = \"07:45\""));
    assert!(shows(&h, SCHEDULE_CHANGED));
    click(&mut h, "重新安裝排程");
    let calls = f.rec.backend.calls.lock().unwrap().clone();
    assert!(calls.iter().any(|c| c == "install_schedule"), "{calls:?}");
    assert!(shows(&h, "排程已重新安裝"));
    assert!(!shows(&h, SCHEDULE_CHANGED));
}

#[test]
fn save_keeps_comments_and_secrets_stay_out_of_file() {
    let (mut h, f) = settings_gui();
    replace(&mut h, "SMTP 主機", "mail.example.edu.tw");
    replace(&mut h, "密碼", "pw-secret");
    click(&mut h, "儲存");
    let text = f.rec.config_text();
    assert!(
        text.contains("smtp_host = \"mail.example.edu.tw\""),
        "{text}"
    );
    assert!(text.contains("# starttls | tls"), "原註解仍在");
    assert!(!text.contains("pw-secret"));
    assert!(f.rec.secrets.is_set(SecretKey::SmtpPassword).unwrap());
    assert!(h.state().settings.password.is_empty(), "秘密緩衝已清空");
}

#[test]
fn testing_uses_unsaved_values_without_writing_file() {
    let (mut h, f) = settings_gui();
    let before = f.rec.config_text();
    replace(&mut h, "SMTP 主機", "smtp.example.edu.tw");
    replace(&mut h, "連接埠", "465");
    click(&mut h, "TLS");
    click(&mut h, "測試寄信");
    let calls = f.rec.backend.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        ["test_email smtp.example.edu.tw:465 tls password=false"]
    );
    assert!(shows(&h, "已寄出測試信"));
    assert_eq!(f.rec.config_text(), before, "設定檔不變");
    assert!(h.state().settings.dirty());
}

#[test]
fn closing_with_unsaved_settings_asks_first() {
    let (mut h, f) = settings_gui();
    click(&mut h, "新增收件者");
    h.get_by_label("1.").focus();
    h.run();
    h.get_by_label("1.").type_text("rz@example.edu.tw");
    h.run();
    assert!(h.state().settings.dirty());
    request_close(&mut h);
    assert_eq!(h.state().leave, Some(Leave::Quit));
    assert!(shows(&h, "有未儲存的變更"));
    click(&mut h, "取消");
    assert!(!h.state().allow_close, "視窗未關閉");
    assert_eq!(
        h.state().settings.form.to,
        ["rz@example.edu.tw"],
        "修改仍在"
    );
    request_close(&mut h);
    click(&mut h, "放棄");
    assert!(h.state().allow_close, "選放棄後才關閉");
    assert!(!h.state().settings.dirty());
    assert!(!f.rec.config_text().contains("rz@example.edu.tw"));
}

#[test]
fn closing_with_unsaved_note_asks_first() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num2);
    click_finding(&mut h, KEV_CVE, "FW-EDGE", egui::Modifiers::NONE);
    click(&mut h, "已處理");
    h.get_by_label("備註（寫入時間軸）").type_text("寫到一半");
    h.run();
    request_close(&mut h);
    assert_eq!(h.state().leave, Some(Leave::Quit));
    click_last(&mut h, "取消");
    assert!(h
        .state()
        .dialog
        .as_ref()
        .is_some_and(|d| d.has_unsaved_note()));
    assert!(!h.state().allow_close);
}

#[test]
fn closing_without_changes_is_not_blocked() {
    let (mut h, _f) = settings_gui();
    request_close(&mut h);
    assert_eq!(h.state().leave, None);
}

#[test]
fn leaving_settings_with_changes_asks_and_save_proceeds() {
    let (mut h, f) = settings_gui();
    replace(&mut h, "SMTP 主機", "mail.example.edu.tw");
    ctrl(&mut h, egui::Key::Num1);
    assert_eq!(h.state().screen, Screen::Settings, "先詢問");
    assert_eq!(h.state().leave, Some(Leave::Screen(Screen::Dashboard)));
    click_last(&mut h, "儲存");
    assert_eq!(h.state().screen, Screen::Dashboard);
    assert!(f.rec.config_text().contains("mail.example.edu.tw"));
}

#[test]
fn escape_on_close_prompt_keeps_note() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num2);
    click_finding(&mut h, KEV_CVE, "FW-EDGE", egui::Modifiers::NONE);
    click(&mut h, "已處理");
    h.get_by_label("備註（寫入時間軸）").type_text("寫到一半");
    h.run();
    request_close(&mut h);
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().leave, None);
    assert!(
        h.state()
            .dialog
            .as_ref()
            .is_some_and(|d| d.has_unsaved_note()),
        "Esc 只取消確認，備註仍在"
    );
}

#[test]
fn failed_note_on_close_keeps_window_open() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num2);
    click_finding(&mut h, KEV_CVE, "FW-EDGE", egui::Modifiers::NONE);
    click(&mut h, "已處理");
    h.get_by_label("備註（寫入時間軸）").type_text("已更新韌體");
    h.run();
    // 命中在背景被刪除：送出會失敗
    let id = finding_id(h.state(), KEV_CVE, "FW-EDGE");
    h.state()
        .ws
        .conn
        .execute("DELETE FROM finding WHERE id = ?1", [id])
        .unwrap();
    request_close(&mut h);
    click_last(&mut h, "儲存");
    assert!(!h.state().allow_close, "送出失敗時不關閉");
    assert!(h
        .state()
        .dialog
        .as_ref()
        .is_some_and(|d| d.has_unsaved_note()));
}

#[test]
fn clearing_unset_secret_is_not_a_change() {
    let (mut h, _f) = settings_gui();
    click(&mut h, "清除密碼");
    assert!(!h.state().settings.dirty(), "原本就未設定：不算變更");
}

#[test]
fn ctrl_f_blocked_by_unsaved_prompt_does_not_steal_focus_later() {
    let (mut h, _f) = settings_gui();
    replace(&mut h, "SMTP 主機", "mail.example.edu.tw");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Escape);
    h.run();
    // 讓輸入框失去焦點後按 Ctrl+F：被未儲存確認攔下
    h.get_by_label("Email").click();
    h.run();
    ctrl(&mut h, egui::Key::F);
    assert_eq!(h.state().leave, Some(Leave::Screen(Screen::Findings)));
    click_last(&mut h, "取消");
    assert!(!h.state().ws.findings.view.focus_search);
}

#[test]
fn entering_settings_rereads_file_and_secret_status() {
    let (mut h, f) = gui(seeded_world());
    // GUI 開啟後由 TUI／CLI 修改：秘密寫入憑證庫、設定檔改摘要時間
    f.rec.secrets.set(SecretKey::SmtpPassword, "pw").unwrap();
    let text = f
        .rec
        .config_text()
        .replace("digest_at = \"08:30\"", "digest_at = \"06:15\"");
    std::fs::write(f.rec.config_path(), text).unwrap();
    ctrl(&mut h, egui::Key::Num6);
    let page = &h.state().settings;
    assert_eq!(page.form.digest_at, "06:15");
    assert_eq!(page.form.smtp_password.status(), "已設定");
    assert!(!page.dirty());
    assert_eq!(h.state().ws.config.schedule.digest_at, "06:15");
}

#[test]
fn clearing_then_retyping_and_erasing_keeps_deletion() {
    let (mut h, f) = gui(seeded_world());
    f.rec.secrets.set(SecretKey::SmtpPassword, "pw").unwrap();
    ctrl(&mut h, egui::Key::Num6);
    click(&mut h, "清除密碼");
    assert!(h.state().settings.dirty());
    h.get_by_label("密碼").focus();
    h.run();
    h.get_by_label("密碼").type_text("x");
    h.run();
    h.key_press(egui::Key::Backspace);
    h.run();
    assert!(h.state().settings.password.is_empty());
    assert_eq!(
        h.state().settings.form.smtp_password.status(),
        "未設定（未儲存）",
        "仍維持刪除"
    );
}

#[test]
fn undo_cannot_bring_back_saved_secret() {
    let (mut h, _f) = settings_gui();
    replace(&mut h, "密碼", "pw-secret");
    click(&mut h, "儲存");
    assert!(h.state().settings.password.is_empty());
    h.get_by_label("密碼").focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert!(
        h.state().settings.password.is_empty(),
        "Ctrl+Z 不會叫回秘密"
    );
}
