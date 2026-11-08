//! 外框：導覽與快捷鍵、明文傳輸警示列、companion 不存在、新資料提示、Dashboard。

mod support;

use std::time::Duration;

use cvewatch_core::matching::scoring::Priority;
use cvewatch_core::matching::triage::Status;
use cvewatch_frontend::filter::KevFilter;
use cvewatch_gui::shell::INSECURE_BANNER;
use cvewatch_gui::Screen;
use eframe::egui;
use egui_kittest::kittest::Queryable;
use support::*;

#[test]
fn ctrl_number_switches_screens() {
    let (mut h, _f) = gui(seeded_world());
    assert_eq!(h.state().screen, Screen::Dashboard);
    ctrl(&mut h, egui::Key::Num3);
    assert_eq!(h.state().screen, Screen::Assets);
    assert!(shows(&h, "匯入清冊"));
    ctrl(&mut h, egui::Key::Num2);
    assert_eq!(h.state().screen, Screen::Findings);
    click(&mut h, "同步紀錄");
    assert_eq!(h.state().screen, Screen::SyncLog);
}

#[test]
fn insecure_banner_on_every_screen() {
    let config = format!(
        "{}\n",
        cvewatch_core::config::example_config().replace(
            "allow_insecure_transport = false",
            "allow_insecure_transport = true"
        )
    );
    assert!(
        config.contains("allow_insecure_transport = true"),
        "範例設定含此鍵"
    );
    let (mut h, _f) = gui_with(
        seeded_world(),
        Setup {
            config,
            ..Setup::default()
        },
    );
    for s in Screen::ALL {
        ctrl(&mut h, s.key());
        assert_eq!(h.state().screen, s);
        assert!(shows(&h, INSECURE_BANNER), "{s:?}");
    }
    let (h, _f) = gui(seeded_world());
    assert!(!shows(&h, INSECURE_BANNER));
}

#[test]
fn missing_companion_disables_sync_and_import() {
    let (mut h, f) = gui_with(
        seeded_world(),
        Setup {
            companion: false,
            ..Setup::default()
        },
    );
    assert!(is_disabled(&h, "同步"));
    ctrl(&mut h, egui::Key::Num3);
    assert!(is_disabled(&h, "匯入清冊"));
    ctrl(&mut h, egui::Key::Num6);
    click(&mut h, "排程與門檻");
    assert!(is_disabled(&h, "重新安裝排程"));
    // 其他瀏覽與處理功能照常
    ctrl(&mut h, egui::Key::Num2);
    assert!(!h.state().ws.findings.visible.is_empty());
    assert_eq!(f.rec.launches.get(), 0);
}

#[test]
fn sync_button_runs_background_sync_and_reports() {
    let (mut h, f) = gui(seeded_world());
    click(&mut h, "同步");
    assert_eq!(f.rec.launches.get(), 1);
    assert!(h.state().busy(), "sync 執行中：較頻繁地喚醒");
    assert!(is_disabled(&h, "同步"), "不重複啟動");
    // 排程 sync 已在執行：子程序以結束碼 3 結束
    *f.rec.exit.borrow_mut() = Some(Some(3));
    h.run();
    assert!(shows(&h, "已有 sync 正在執行"));
    assert!(!h.state().busy());
}

#[test]
fn new_data_prompt_keeps_table_until_reload() {
    let (mut h, f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num2);
    let before = table_cves(h.state());
    assert!(!shows(&h, "有新資料"));
    // 排程 sync 完成：同步標記改變，並把一筆命中改為已處理（重新載入後才會消失）
    let conn = &h.state().ws.conn;
    sync_run(conn, "kev", "2026-11-07T01:00:00+00:00", "success", None);
    conn.execute(
        "UPDATE finding SET status = 'resolved' WHERE cve_id = ?1",
        [KEV_CVE],
    )
    .unwrap();
    f.advance(Duration::from_secs(1));
    h.run();
    assert!(!shows(&h, "有新資料"), "未到輪詢間隔");
    f.advance(Duration::from_secs(5));
    h.run();
    assert!(shows(&h, "有新資料"));
    assert_eq!(table_cves(h.state()), before, "不自動重排表格");
    click(&mut h, "重新載入");
    assert!(!shows(&h, "有新資料"));
    let after = table_cves(h.state());
    assert!(after.iter().all(|(c, _)| c != KEV_CVE), "{after:?}");
}

#[test]
fn dashboard_shows_failed_source_and_jumps_with_filter() {
    let (mut h, _f) = gui(seeded_world());
    assert!(shows(&h, "osv"));
    assert!(shows(&h, "失敗"));
    assert!(shows(&h, "11-06 22:10"), "失敗時間");
    assert!(shows(&h, "HTTP 503"));
    h.get_by_label_contains("P1：").click();
    h.run();
    let app = h.state();
    assert_eq!(app.screen, Screen::Findings);
    assert_eq!(app.ws.findings.filter.statuses, [Status::Open]);
    assert_eq!(app.ws.findings.filter.priorities, [Priority::P1]);
    ctrl(&mut h, egui::Key::Num1);
    h.get_by_label_contains("KEV 命中").click();
    h.run();
    let app = h.state();
    assert_eq!(app.ws.findings.filter.kev, KevFilter::Only);
    assert!(app.ws.findings.filter.priorities.is_empty());
    assert!(table_cves(app).iter().all(|(c, _)| c == KEV_CVE));
}

#[test]
fn warning_bar_can_be_dismissed() {
    let (mut h, _f) = gui_with(
        seeded_world(),
        Setup {
            warnings: vec!["無法寫入日誌檔：x".into()],
            ..Setup::default()
        },
    );
    assert!(shows(&h, "警告：無法寫入日誌檔：x"));
    click(&mut h, "關閉警告");
    assert!(!shows(&h, "無法寫入日誌檔"));
}
