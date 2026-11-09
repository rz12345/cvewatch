//! 命中畫面：預設檢視、即時搜尋、欄位排序、多選與狀態變更、詳情、匯出 VEX。

mod support;

use cvewatch_frontend::filter::{self, Filter, SortKey};
use cvewatch_gui::Screen;
use eframe::egui;
use egui_kittest::kittest::Queryable;
use support::*;

fn findings_gui() -> (egui_kittest::Harness<'static, cvewatch_gui::GuiApp>, Fakes) {
    let (mut h, f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num2);
    assert_eq!(h.state().screen, Screen::Findings);
    (h, f)
}

fn expected(app: &cvewatch_gui::GuiApp, f: &Filter, search: &str, sort: SortKey) -> Vec<String> {
    let d = &app.ws.data;
    filter::visible(&d.findings, &d.search_keys, f, search, sort)
        .into_iter()
        .map(|i| format!("{} {}", d.findings[i].cve_id, d.findings[i].asset_tag))
        .collect()
}

fn shown(app: &cvewatch_gui::GuiApp) -> Vec<String> {
    table_cves(app)
        .into_iter()
        .map(|(c, a)| format!("{c} {a}"))
        .collect()
}

#[test]
fn default_view_is_open_findings_by_risk() {
    let (h, _f) = findings_gui();
    let app = h.state();
    let rows = shown(app);
    assert_eq!(rows.len(), 5, "{rows:?}");
    assert_eq!(rows, expected(app, &Filter::default(), "", SortKey::Risk));
    assert_eq!(rows[0], format!("{KEV_CVE} FW-EDGE"), "KEV 的 P1 排最前");
    for cve in PANOS_CVES.iter().chain([&OLD_CVE]) {
        assert!(h.query_all_by_label(cve).next().is_some(), "{cve} 在表格中");
    }
    assert!(shows(&h, LONG_NAME), "中文資產名稱完整顯示");
}

#[test]
fn search_narrows_as_you_type_and_header_sorts() {
    let (mut h, _f) = findings_gui();
    search(&mut h, "2026-0");
    let rows = shown(h.state());
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert!(rows.iter().all(|r| r.contains("2026-0")));
    assert!(h.query_all_by_label(OLD_CVE).next().is_none());
    // 清除搜尋後依 CVSS 排序
    h.get_by_label("搜尋").focus();
    h.run();
    for _ in 0..6 {
        h.key_press(egui::Key::Backspace);
    }
    h.run();
    assert_eq!(h.state().ws.findings.search, "");
    click(&mut h, "CVSS");
    let app = h.state();
    assert_eq!(app.ws.findings.sort, SortKey::Cvss);
    assert_eq!(
        shown(app),
        expected(app, &Filter::default(), "", SortKey::Cvss)
    );
}

#[test]
fn batch_ignore_with_reason_justification_and_note() {
    let (mut h, _f) = findings_gui();
    let rows = [
        (PANOS_CVES[1], "FW-EDGE"),
        (PANOS_CVES[2], "FW-EDGE"),
        (OLD_CVE, "FW-CORE"),
    ];
    select_rows(&mut h, &rows);
    assert_eq!(h.state().ws.findings.selected.len(), 3);
    click(&mut h, "忽略");
    click(&mut h, "判定不受影響");
    click(&mut h, "需特定設定");
    h.get_by_label("備註（寫入時間軸）")
        .type_text("只在啟用 GlobalProtect 時受影響");
    h.run();
    click(&mut h, "確認");
    for (cve, asset) in rows {
        let (status, reason, j) = finding_status(h.state(), cve, asset);
        assert_eq!(status, "ignored", "{cve} {asset}");
        assert_eq!(reason.as_deref(), Some("not_affected"));
        assert_eq!(j.as_deref(), Some("requires_configuration"));
        let id = finding_id(h.state(), cve, asset);
        assert_eq!(
            last_event(h.state(), id),
            ("ignored".into(), "只在啟用 GlobalProtect 時受影響".into())
        );
    }
    assert!(shows(&h, "已將 3 筆命中設為忽略"));
    assert_eq!(finding_status(h.state(), KEV_CVE, "FW-EDGE").0, "open");
}

#[test]
fn cancelling_note_dialog_changes_nothing() {
    let (mut h, _f) = findings_gui();
    click_finding(&mut h, KEV_CVE, "FW-EDGE", egui::Modifiers::NONE);
    click(&mut h, "已處理");
    h.get_by_label("備註（寫入時間軸）").type_text("寫一半");
    h.run();
    click(&mut h, "取消");
    assert!(h.state().dialog.is_none());
    assert_eq!(finding_status(h.state(), KEV_CVE, "FW-EDGE").0, "open");
    // Esc 也取消
    click(&mut h, "已處理");
    assert!(h.state().dialog.is_some());
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().dialog.is_none());
    assert_eq!(finding_status(h.state(), KEV_CVE, "FW-EDGE").0, "open");
}

#[test]
fn selection_hidden_by_search_is_kept_and_not_changed() {
    let (mut h, _f) = findings_gui();
    let rows = [
        (KEV_CVE, "FW-EDGE"),
        (PANOS_CVES[1], "FW-EDGE"),
        (OLD_CVE, "FW-EDGE"),
    ];
    select_rows(&mut h, &rows);
    search(&mut h, OLD_CVE);
    click(&mut h, "已處理");
    h.get_by_label("備註（寫入時間軸）").type_text("已更新韌體");
    h.run();
    click(&mut h, "確認");
    assert_eq!(finding_status(h.state(), OLD_CVE, "FW-EDGE").0, "resolved");
    assert_eq!(finding_status(h.state(), KEV_CVE, "FW-EDGE").0, "open");
    assert_eq!(
        finding_status(h.state(), PANOS_CVES[1], "FW-EDGE").0,
        "open"
    );
    let app = h.state();
    let kept: Vec<i64> = [rows[0], rows[1]]
        .iter()
        .map(|(c, a)| finding_id(app, c, a))
        .collect();
    assert_eq!(
        app.ws.findings.selected.iter().copied().collect::<Vec<_>>(),
        kept,
        "被隱藏的兩筆仍保持選取"
    );
}

#[test]
fn single_key_shortcuts_only_without_text_focus() {
    let (mut h, _f) = findings_gui();
    click_finding(&mut h, KEV_CVE, "FW-EDGE", egui::Modifiers::NONE);
    h.get_by_label("搜尋").focus();
    h.run();
    h.key_press(egui::Key::R);
    h.run();
    assert!(h.state().dialog.is_none(), "搜尋框中的 r 不觸發狀態變更");
    // 移開焦點後 r 開啟備註對話框
    click_finding(&mut h, KEV_CVE, "FW-EDGE", egui::Modifiers::NONE);
    h.key_press(egui::Key::R);
    h.run();
    assert!(h.state().dialog.is_some());
}

#[test]
fn detail_shows_evidence_and_opens_vendor_link() {
    let (mut h, f) = findings_gui();
    let id = finding_id(h.state(), KEV_CVE, "FW-EDGE");
    // 按兩下開啟詳情（兩次點選間隔一個 step，0.25 秒）
    h.get_all_by_label(KEV_CVE).next().expect("row").click();
    h.step();
    h.get_all_by_label(KEV_CVE).next().expect("row").click();
    h.run();
    assert_eq!(h.state().detail.as_ref().map(|d| d.id), Some(id));
    assert!(shows(&h, "判定來源 原廠公告"), "判定來源");
    assert!(
        shows(&h, "[原廠公告] 受影響：paloalto 公告 PAN-SA-2026-0001"),
        "判定證據"
    );
    assert!(shows(&h, "vers:generic/>=11.1.0|<11.1.3"), "受影響區間");
    assert!(shows(&h, "修正版本：11.1.3"));
    assert!(shows(&h, "元件 paloalto-panos 11.1.2"), "元件目前版本");
    h.get_by_label_contains("原廠公告 paloalto").click();
    h.run();
    assert_eq!(f.rec.opened.borrow().as_slice(), [VENDOR_URL]);
    click(&mut h, "複製 CVE ID");
    assert_eq!(f.rec.copied.borrow().as_slice(), [KEV_CVE]);
}

#[test]
fn detail_closes_when_finding_is_gone_after_reload() {
    let (mut h, _f) = findings_gui();
    click_finding(&mut h, KEV_CVE, "FW-EDGE", egui::Modifiers::NONE);
    h.key_press(egui::Key::Enter);
    h.run();
    let id = h
        .state()
        .detail
        .as_ref()
        .map(|d| d.id)
        .expect("detail open");
    h.state()
        .ws
        .conn
        .execute("DELETE FROM finding WHERE id = ?1", [id])
        .unwrap();
    click(&mut h, "重新載入");
    assert!(h.state().detail.is_none());
    assert!(shows(&h, "已不存在"));
}

#[test]
fn export_filtered_findings_to_documents() {
    let (mut h, f) = findings_gui();
    click(&mut h, "篩選…");
    h.get_by_label("資產篩選").click();
    h.run();
    click_option(&mut h, "FW-CORE");
    assert_eq!(
        h.state().ws.findings.filter.asset.as_deref(),
        Some("FW-CORE")
    );
    *f.dialogs.save.borrow_mut() = Some(None);
    click(&mut h, "匯出 VEX");
    let asked = f.dialogs.save_asked.borrow().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].0, f.rec.documents_dir(), "預設文件資料夾");
    assert!(asked[0].1.starts_with("cvewatch-vex-"), "{}", asked[0].1);
    let path = asked[0].0.join(&asked[0].1);
    let text = std::fs::read_to_string(&path).expect("exported");
    assert!(text.contains("FW-CORE") && !text.contains("FW-EDGE"));
    assert!(shows(&h, "此檔未加密"));
    assert!(shows(&h, &path.display().to_string()));
}

#[test]
fn export_never_overwrites_existing_file() {
    let (mut h, f) = findings_gui();
    let target = f.rec.documents_dir().join("existing.json");
    std::fs::write(&target, "original").unwrap();
    *f.dialogs.save.borrow_mut() = Some(Some(target.clone()));
    click(&mut h, "匯出 VEX");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "original");
    assert!(shows(&h, "已存在"));
    let status = h.state().status.clone().expect("status");
    assert_eq!(status.level, cvewatch_frontend::Level::Error);
}

#[test]
fn context_menu_acts_on_right_clicked_row() {
    let (mut h, f) = findings_gui();
    select_rows(&mut h, &[(OLD_CVE, "FW-EDGE"), (KEV_CVE, "FW-EDGE")]);
    // 已選兩列、游標在 KEV_CVE；在 OLD_CVE 那列按右鍵複製
    let id = finding_id(h.state(), OLD_CVE, "FW-EDGE");
    let pos = h
        .state()
        .ws
        .visible_ids()
        .iter()
        .position(|x| *x == id)
        .unwrap();
    let earlier = (0..pos)
        .filter(|p| h.state().ws.row_at(*p).is_some_and(|r| r.cve_id == OLD_CVE))
        .count();
    h.get_all_by_label(OLD_CVE)
        .nth(earlier)
        .unwrap()
        .click_secondary();
    h.run();
    assert_eq!(h.state().ws.findings.selected.len(), 2, "右鍵不改變多選");
    click(&mut h, "複製 CVE ID");
    assert_eq!(f.rec.copied.borrow().as_slice(), [OLD_CVE]);
}

#[test]
fn hidden_cursor_row_is_not_a_triage_target() {
    let (mut h, _f) = findings_gui();
    click_finding(&mut h, OLD_CVE, "FW-CORE", egui::Modifiers::NONE);
    click(&mut h, "已處理");
    click(&mut h, "確認");
    assert_eq!(finding_status(h.state(), OLD_CVE, "FW-CORE").0, "resolved");
    // 游標列已被「只看未處理」隱藏：按 i 不得變更它
    h.key_press(egui::Key::I);
    h.run();
    assert!(h.state().dialog.is_none());
    assert!(shows(&h, "請先選取命中"));
    assert_eq!(finding_status(h.state(), OLD_CVE, "FW-CORE").0, "resolved");
}

#[test]
fn escape_closes_dialog_but_keeps_detail() {
    let (mut h, _f) = findings_gui();
    click_finding(&mut h, KEV_CVE, "FW-EDGE", egui::Modifiers::NONE);
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.state().detail.is_some());
    h.key_press(egui::Key::I);
    h.run();
    assert!(h.state().dialog.is_some());
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().dialog.is_none());
    assert!(h.state().detail.is_some(), "一次 Esc 只關閉對話框");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().detail.is_none());
}

#[test]
fn arrow_keys_move_cursor() {
    let (mut h, _f) = findings_gui();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    let first = h.state().ws.visible_ids()[0];
    assert_eq!(h.state().ws.findings.view.current, Some(first));
    h.key_press(egui::Key::ArrowDown);
    h.run();
    let second = h.state().ws.visible_ids()[1];
    assert_eq!(h.state().ws.findings.view.current, Some(second));
    assert_eq!(h.state().ws.findings.view.scroll_to, None, "捲動要求已套用");
}

#[test]
fn failed_write_keeps_note_dialog() {
    let (mut h, _f) = findings_gui();
    click_finding(&mut h, KEV_CVE, "FW-EDGE", egui::Modifiers::NONE);
    click(&mut h, "已處理");
    h.get_by_label("備註（寫入時間軸）").type_text("很長的備註");
    h.run();
    let id = finding_id(h.state(), KEV_CVE, "FW-EDGE");
    h.state()
        .ws
        .conn
        .execute("DELETE FROM finding WHERE id = ?1", [id])
        .unwrap();
    click(&mut h, "確認");
    assert!(
        h.state()
            .dialog
            .as_ref()
            .is_some_and(|d| d.has_unsaved_note()),
        "寫入失敗時保留備註"
    );
    let status = h.state().status.clone().expect("status");
    assert_eq!(status.level, cvewatch_frontend::Level::Error);
}

#[test]
fn http_links_are_hidden_and_refused() {
    let (h, _f) = findings_gui();
    let app = h.state();
    assert!(!app.ws.link_allowed("http://example.com/advisory"));
    assert!(app.ws.link_allowed("https://example.com/advisory"));
    let r = app.ws.open_url("http://example.com/advisory");
    assert!(r.message.contains("只開啟 https"), "{}", r.message);
}

#[test]
fn control_characters_in_cells_are_flattened() {
    let (mut h, _f) = gui(seeded_world());
    h.state()
        .ws
        .conn
        .execute(
            "UPDATE asset SET name = 'line1\nline2' WHERE asset_tag = 'FW-EDGE'",
            [],
        )
        .unwrap();
    click(&mut h, "重新載入");
    ctrl(&mut h, egui::Key::Num2);
    assert!(shows(&h, "line1 line2"));
}
