//! 05 任務 3.6：Findings 清單、搜尋、詳情與備註對話框快照；批次變更寫入事件；連結與複製。

mod support;

use cvewatch_core::matching::triage::Status;
use cvewatch_tui::dialog::Dialog;
use cvewatch_tui::View;
use ratatui::crossterm::event::KeyCode;
use rusqlite::params;
use support::{
    app, ctrl, finding_status, keys, press, render, render_size, seeded_world, KEV_CVE, PANOS_CVES,
};

fn open_findings() -> (cvewatch_tui::App, support::Recorder) {
    let (mut app, rec) = app(seeded_world());
    keys(&mut app, "2");
    (app, rec)
}

fn events(app: &cvewatch_tui::App, cve: &str) -> Vec<(String, String, String)> {
    let mut stmt = app
        .conn
        .prepare(
            "SELECT e.actor, e.to_status, e.note FROM finding_event e JOIN finding f ON f.id = e.finding_id
             WHERE f.cve_id = ?1 ORDER BY e.id",
        )
        .unwrap();
    stmt.query_map(params![cve], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn default_view_lists_open_findings_by_risk() {
    let (app, _) = open_findings();
    let rows: Vec<_> = app
        .findings
        .visible
        .iter()
        .map(|i| &app.data.findings[*i])
        .collect();
    assert!(rows.iter().all(|r| r.status == Status::Open));
    assert_eq!(rows[0].cve_id, KEV_CVE, "KEV 命中排第一");
    let priorities: Vec<_> = rows.iter().map(|r| r.priority).collect();
    let mut sorted = priorities.clone();
    sorted.sort();
    assert_eq!(priorities, sorted, "依 P1 到 P4 排列");
    insta::assert_snapshot!(render(&app));
}

#[test]
fn slash_search_filters_live() {
    let (mut app, _) = open_findings();
    keys(&mut app, "/2026-0");
    assert!(app.findings.search_input.is_some());
    let shown: Vec<_> = app
        .findings
        .visible
        .iter()
        .map(|i| app.data.findings[*i].cve_id.clone())
        .collect();
    assert!(!shown.is_empty());
    assert!(shown.iter().all(|c| c.contains("2026-0")));
    insta::assert_snapshot!(render(&app));
    press(&mut app, KeyCode::Enter);
    assert!(app.findings.search_input.is_none());
    assert_eq!(app.findings.search, "2026-0", "Enter 保留搜尋條件");
    press(&mut app, KeyCode::Esc);
    assert!(app.findings.search.is_empty(), "Esc 清除搜尋");
}

#[test]
fn filter_panel_and_sort_change_the_list() {
    let (mut app, _) = open_findings();
    let open = app.findings.visible.len();
    keys(&mut app, "f");
    assert!(matches!(app.dialog, Some(Dialog::Filter { .. })));
    insta::assert_snapshot!(render(&app));
    // 游標在「狀態 未處理」：取消後不限狀態
    keys(&mut app, " ");
    press(&mut app, KeyCode::Enter);
    assert!(app.findings.filter.statuses.is_empty());
    assert!(app.findings.visible.len() >= open);
    keys(&mut app, "s");
    assert_eq!(app.findings.sort.label(), "CVSS 高→低");
}

#[test]
fn detail_shows_evidence_range_and_component_version() {
    let (mut app, _) = open_findings();
    press(&mut app, KeyCode::Enter);
    let Some(View::FindingDetail { data, .. }) = app.view() else {
        panic!("應進入詳情");
    };
    assert_eq!(data.finding.row.cve_id, KEV_CVE);
    let out = render_size(&app, 110, 50);
    assert!(out.contains("[CNA] 受影響"), "{out}");
    assert!(out.contains("11.1.2"), "元件目前版本");
    assert!(out.contains("KEV 列入 2026-10-20"));
    insta::assert_snapshot!(out);
    press(&mut app, KeyCode::Esc);
    assert!(app.stack.is_empty());
}

#[test]
fn batch_ignore_not_affected_writes_one_event_per_finding() {
    let (mut app, _) = open_findings();
    let targets: Vec<String> = app.findings.visible[..3]
        .iter()
        .map(|i| app.data.findings[*i].cve_id.clone())
        .collect();
    keys(&mut app, "   ");
    assert_eq!(app.findings.selected.len(), 3);
    keys(&mut app, "i");
    press(&mut app, KeyCode::Enter); // 不受影響
    keys(&mut app, "jj"); // 需特定設定
    press(&mut app, KeyCode::Enter);
    keys(&mut app, "未啟用 GlobalProtect");
    insta::assert_snapshot!(render(&app));
    press(&mut app, KeyCode::Enter);
    assert!(app.dialog.is_none());
    assert!(app.findings.selected.is_empty());
    assert!(app
        .message
        .as_deref()
        .unwrap()
        .contains("已將 3 筆命中設為忽略"));
    for cve in &targets {
        let ignored = app.conn.query_row(
            "SELECT status, ignore_reason, justification FROM finding WHERE cve_id = ?1 AND status = 'ignored'",
            params![cve],
            |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        );
        assert_eq!(
            ignored.unwrap(),
            (
                "not_affected".to_string(),
                "requires_configuration".to_string()
            )
        );
        let last = events(&app, cve).pop().unwrap();
        assert_eq!(
            last,
            (
                "user".into(),
                "ignored".into(),
                "未啟用 GlobalProtect".into()
            )
        );
    }
}

#[test]
fn esc_in_note_dialog_changes_nothing() {
    let (mut app, _) = open_findings();
    let before = events(&app, KEV_CVE).len();
    keys(&mut app, "r");
    keys(&mut app, "abc");
    press(&mut app, KeyCode::Esc);
    assert!(app.dialog.is_none());
    assert_eq!(events(&app, KEV_CVE).len(), before);
    assert_eq!(finding_status(&app, KEV_CVE, "FW-EDGE").unwrap().0, "open");
}

#[test]
fn resolve_from_detail_then_reopen() {
    let (mut app, _) = open_findings();
    press(&mut app, KeyCode::Enter);
    keys(&mut app, "r");
    keys(&mut app, "已升級");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        finding_status(&app, KEV_CVE, "FW-EDGE").unwrap().0,
        "resolved"
    );
    let Some(View::FindingDetail { data, .. }) = app.view() else {
        panic!("仍在詳情");
    };
    assert_eq!(data.finding.row.status, Status::Resolved, "詳情已重新載入");
    keys(&mut app, "u");
    press(&mut app, KeyCode::Enter);
    assert_eq!(finding_status(&app, KEV_CVE, "FW-EDGE").unwrap().0, "open");
}

#[test]
fn ctrl_c_with_unsent_note_asks_first() {
    let (mut app, _) = open_findings();
    keys(&mut app, "r");
    keys(&mut app, "q");
    assert!(!app.quit, "備註框內的 q 是文字");
    ctrl(&mut app, 'c');
    assert!(matches!(app.dialog, Some(Dialog::ConfirmQuit { .. })));
    keys(&mut app, "n");
    assert!(app.dialog.as_ref().is_some_and(Dialog::has_unsaved_note));
    ctrl(&mut app, 'c');
    keys(&mut app, "y");
    assert!(app.quit);
}

#[test]
fn o_opens_menu_and_y_copies() {
    let (mut app, rec) = open_findings();
    keys(&mut app, "o");
    let Some(Dialog::Links { links, .. }) = &app.dialog else {
        panic!("多個連結時先顯示選單");
    };
    assert!(links.len() >= 2);
    keys(&mut app, "j");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        rec.opened.borrow().as_slice(),
        [format!("https://nvd.nist.gov/vuln/detail/{KEV_CVE}")]
    );
    keys(&mut app, "y");
    assert_eq!(rec.copied.borrow().as_slice(), [KEV_CVE.to_string()]);
    rec.clipboard_fails.set(true);
    keys(&mut app, "y");
    assert!(app.message.as_deref().unwrap().contains("無法複製"));
}

#[test]
fn panos_findings_exist_for_fw_edge() {
    let (app, _) = open_findings();
    for cve in PANOS_CVES {
        assert!(finding_status(&app, cve, "FW-EDGE").is_some(), "{cve}");
    }
}

#[test]
fn selection_hidden_by_search_is_not_changed() {
    let (mut app, _) = open_findings();
    keys(&mut app, "   "); // 選取前三筆（FW-EDGE 的 PAN-OS 命中）
    assert_eq!(app.findings.selected.len(), 3);
    keys(&mut app, "/2026-0001");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.findings.selected.len(), 3, "多選保留");
    assert_eq!(app.visible_selection().len(), 1, "但只有可見的會變更");
    assert!(render(&app).contains("另 2 筆未顯示"));
    keys(&mut app, "r");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        finding_status(&app, KEV_CVE, "FW-EDGE").unwrap().0,
        "resolved"
    );
    for cve in &PANOS_CVES[1..] {
        assert_eq!(
            finding_status(&app, cve, "FW-EDGE").unwrap().0,
            "open",
            "{cve} 被隱藏，不應變更"
        );
    }
}

#[test]
fn message_is_cleared_by_next_key() {
    let (mut app, _) = open_findings();
    keys(&mut app, "y");
    assert!(app.message.is_some());
    keys(&mut app, "4");
    assert!(app.message.is_none());
    assert!(render(&app).contains("a 套用到資產"), "恢復鍵位提示");
}

#[test]
fn second_ctrl_c_on_confirm_quits() {
    let (mut app, _) = open_findings();
    keys(&mut app, "rabc");
    ctrl(&mut app, 'c');
    ctrl(&mut app, 'c');
    assert!(app.quit);
}

#[test]
fn detail_scroll_stops_at_end_of_content() {
    let (mut app, _) = open_findings();
    press(&mut app, KeyCode::Enter);
    render_size(&app, 110, 12);
    let max = app.offsets.detail_max.get();
    assert!(max > 0, "內容超過一頁");
    for _ in 0..50 {
        press(&mut app, KeyCode::PageDown);
    }
    let Some(View::FindingDetail { scroll, .. }) = app.view() else {
        panic!()
    };
    assert_eq!(*scroll, max);
    press(&mut app, KeyCode::Up);
    let Some(View::FindingDetail { scroll, .. }) = app.view() else {
        panic!()
    };
    assert_eq!(*scroll, max - 1, "往上一次就回到內容");
}

#[test]
fn reload_failure_after_write_is_reported() {
    let (mut app, _) = open_findings();
    app.conn
        .execute("ALTER TABLE sync_run RENAME TO sync_run_gone", [])
        .unwrap();
    keys(&mut app, "r");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        finding_status(&app, KEV_CVE, "FW-EDGE").unwrap().0,
        "resolved"
    );
    let msg = app.message.clone().unwrap();
    assert!(
        msg.contains("已將 1 筆命中設為已處理") && msg.contains("重新載入失敗"),
        "{msg}"
    );
}

#[test]
fn selection_survives_a_narrower_search() {
    let (mut app, _) = open_findings();
    keys(&mut app, "   ");
    keys(&mut app, "/zzz");
    assert!(app.findings.visible.is_empty());
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.findings.selected.len(), 3);
    assert_eq!(app.visible_selection().len(), 3);
}

#[test]
fn single_change_from_detail_keeps_list_selection() {
    let (mut app, _) = open_findings();
    keys(&mut app, "  "); // 選取前兩筆，游標移到第三筆
    press(&mut app, KeyCode::Enter);
    keys(&mut app, "r");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        finding_status(&app, PANOS_CVES[2], "FW-EDGE").unwrap().0,
        "resolved"
    );
    assert_eq!(app.findings.selected.len(), 2, "清單上的多選保留");
}

#[test]
fn detail_keeps_paragraph_breaks_and_ascii_glyphs() {
    let w = seeded_world();
    w.conn
        .execute(
            "UPDATE cve SET description_en = ?2 WHERE id = ?1",
            params![
                KEV_CVE,
                "Affects PAN-OS 10.2.
Fixed in	10.2.9."
            ],
        )
        .unwrap();
    let (mut app, _) = support::app_with_glyphs(w, cvewatch_tui::glyphs::ASCII);
    keys(&mut app, "2");
    press(&mut app, KeyCode::Enter);
    let out = render_size(&app, 110, 50);
    assert!(!out.contains("10.2.Fixed"), "{out}");
    assert!(out.contains("Affects PAN-OS 10.2."));
    assert!(out.contains("Fixed in 10.2.9."));
    assert!(!out.contains('→') && !out.contains('⚠'), "{out}");
    assert!(out.contains("（新） -> 未處理"));
}

#[test]
fn batch_skips_findings_already_in_that_status() {
    let (mut app, _) = open_findings();
    let before = events(&app, KEV_CVE).len();
    keys(&mut app, "  "); // 兩筆未處理
    keys(&mut app, "u");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        events(&app, KEV_CVE).len(),
        before,
        "不寫入「未處理 → 未處理」"
    );
    assert!(app
        .message
        .as_deref()
        .unwrap()
        .contains("原本就是未處理，未變更"));
}

#[test]
fn hidden_selection_blocks_triage_with_notice() {
    let (mut app, _) = open_findings();
    keys(&mut app, " "); // 選取 KEV 命中
    keys(&mut app, "/2026-0002");
    press(&mut app, KeyCode::Enter);
    keys(&mut app, "r");
    assert!(app.dialog.is_none());
    assert!(app.message.as_deref().unwrap().contains("目前都未顯示"));
}

#[test]
fn detail_of_deleted_finding_is_closed_on_reload() {
    let (mut app, _) = open_findings();
    press(&mut app, KeyCode::Enter);
    let Some(View::FindingDetail { data, .. }) = app.view() else {
        panic!()
    };
    let id = data.finding.row.id;
    app.conn
        .execute(
            "DELETE FROM finding_event WHERE finding_id = ?1",
            params![id],
        )
        .unwrap();
    app.conn
        .execute(
            "DELETE FROM finding_evidence WHERE finding_id = ?1",
            params![id],
        )
        .unwrap();
    app.conn
        .execute("DELETE FROM finding WHERE id = ?1", params![id])
        .unwrap();
    keys(&mut app, "R");
    assert!(app.stack.is_empty(), "詳情已關閉");
    assert!(app
        .message
        .as_deref()
        .unwrap()
        .contains("已不存在，已關閉詳情"));
}

#[test]
fn conhost_truncation_uses_ascii_marker() {
    let (mut app, _) = support::app_with_glyphs(seeded_world(), cvewatch_tui::glyphs::ASCII);
    keys(&mut app, "3");
    let out = render_size(&app, 72, 14);
    // 22 加入「週期」欄後名稱欄變窄
    assert!(out.contains("資訊所機~"), "{out}");
    assert!(!out.contains('…'));
}
