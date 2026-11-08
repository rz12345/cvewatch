//! 11 任務 2.1～2.4：資產清單表格、詳情的使用者用語、危險操作按鈕與資產清單的鍵盤操作。

mod support;

use cvewatch_gui::dialogs::AssetDialog;
use eframe::egui;
use egui_kittest::kittest::Queryable;
use support::*;

/// 兩個節點位於同一列（垂直中心相差不到半列）。
fn same_row(a: egui::Rect, b: egui::Rect) -> bool {
    (a.center().y - b.center().y).abs() < 8.0
}

#[test]
fn asset_list_is_a_table_with_open_count_column() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    for column in ["編號", "名稱", "未處理"] {
        assert!(h.query_by_label(column).is_some(), "欄位 {column}");
    }
    let fw = list_cells(&h, "FW-CORE")[0].rect();
    assert!(
        list_cells(&h, LONG_NAME)
            .iter()
            .any(|n| same_row(n.rect(), fw)),
        "長名稱在自己的欄位"
    );
    // 未處理數靠右對齊，位置在「未處理」欄位標題的下方
    let header = h.get_by_label("未處理").rect();
    let count = h
        .query_all_by_label("1")
        .find(|n| same_row(n.rect(), fw))
        .expect("FW-CORE 的未處理數是獨立節點");
    assert!(count.rect().left() >= header.left(), "未處理數在自己的欄位");
    // 名稱與編號相同：名稱欄留空，不重複顯示
    assert_eq!(list_cells(&h, "PVE-01").len(), 1);
}

#[test]
fn asset_detail_uses_display_names_and_bilingual_values() {
    let (mut h, _f) = gui(seeded_world());
    select_asset(&mut h, "PVE-01");
    assert!(shows(&h, "Proxmox VE 8.2.2"));
    assert!(!shows(&h, "proxmox-ve 8.2.2"), "產品 key 不是主要文字");
    assert!(shows(&h, "HW01 伺服器"));
    assert!(shows(&h, "internal（內部網路）"));
    assert!(h.query_by_label("沒有命中").is_some());
    // 標題只顯示一次編號（名稱同編號）
    assert!(!shows(&h, "PVE-01  PVE-01"));
    select_asset(&mut h, "PA-460");
    assert!(h.query_by_label("沒有命中").is_none(), "有命中時列出命中");
    assert!(shows(&h, "PAN-OS 11.1.2"));
}

#[test]
fn asset_detail_retired_notes_no_matching() {
    let w = seeded_world();
    w.retire("PVE-01");
    let (mut h, _f) = gui(w);
    h.state_mut().assets_show_retired = true;
    select_asset(&mut h, "PVE-01");
    assert!(shows(&h, "不參與比對"));
    assert!(h.query_by_label("沒有命中").is_some());
}

#[test]
fn asset_detail_offers_retire_instead_of_delete() {
    let (mut h, _f) = gui(seeded_world());
    select_asset(&mut h, "PA-460");
    assert!(h.query_by_label("退役…").is_some());
    assert!(h.query_by_label("刪除").is_none(), "不再以「刪除」指稱退役");
    let edit = h.get_by_label("編輯").rect();
    let retire = h.get_by_label("退役…").rect();
    assert!(
        retire.left() - edit.right() > 16.0,
        "危險操作與一般操作間隔"
    );
}

#[test]
fn asset_list_enter_edits_and_delete_retires() {
    let (mut h, _f) = gui(seeded_world());
    select_asset(&mut h, "PA-460");
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(shows(&h, "編輯 PA-460"), "Enter 開啟編輯");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().asset_dialog.is_none());
    h.key_press(egui::Key::Delete);
    h.run();
    assert!(shows(&h, "將 PA-460 標為退役"), "Delete 開啟退役確認");
    assert!(matches!(
        h.state().asset_dialog,
        Some(AssetDialog::Retire { .. })
    ));
}

#[test]
fn asset_list_enter_after_clicking_a_row() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    pick_asset(&mut h, "FW-CORE");
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(shows(&h, "編輯 FW-CORE"), "點選清單列後 Enter 開啟編輯");
}

#[test]
fn asset_list_enter_on_a_focused_button_presses_the_button() {
    let (mut h, f) = gui(seeded_world());
    select_asset(&mut h, "PA-460");
    h.get_by_label("下載範本").focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.state().asset_dialog.is_none(), "不開啟編輯");
    assert_eq!(
        f.dialogs.save_asked.borrow().len(),
        1,
        "按下的是「下載範本」"
    );
}

#[test]
fn asset_list_keys_are_ignored_while_typing_or_for_retired() {
    let (mut h, _f) = gui(seeded_world());
    select_asset(&mut h, "PA-460");
    click(&mut h, "新增資產");
    replace_text(&mut h, "名稱 *", "x");
    h.key_press(egui::Key::Delete);
    h.run();
    assert!(
        matches!(h.state().asset_dialog, Some(AssetDialog::Edit(_))),
        "對話框開啟時 Delete 不開啟退役確認"
    );
    let w = seeded_world();
    w.retire("PVE-01");
    let (mut h, _f) = gui(w);
    h.state_mut().assets_show_retired = true;
    select_asset(&mut h, "PVE-01");
    h.key_press(egui::Key::Delete);
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(
        h.state().asset_dialog.is_none(),
        "已退役資產不回應 Enter／Delete"
    );
}

/// Scenario: Bilingual category value — 詳情與類別下拉選單顯示「HW01 伺服器」，儲存後仍為 `HW01`。
#[test]
fn asset_detail_bilingual_category_value() {
    let (mut h, _f) = gui(seeded_world());
    select_asset(&mut h, "PVE-01");
    assert!(shows(&h, "類別 HW01 伺服器"));
    click(&mut h, "編輯");
    let combo = h.get_by_label("類別 *");
    assert_eq!(
        combo.value().as_deref(),
        Some("HW01 伺服器"),
        "下拉選單顯示代碼與名稱"
    );
    replace_text(&mut h, "名稱 *", "pve01");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    let category: String = h
        .state()
        .ws
        .conn
        .query_row(
            "SELECT category FROM asset WHERE asset_tag = 'PVE-01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(category, "HW01");
}

/// Scenario: Ratings in the detail pane
#[test]
fn asset_detail_ratings_in_the_detail_pane() {
    let w = seeded_world();
    w.conn
        .execute(
            "UPDATE asset SET category = 'HW03', confidentiality = 2, integrity = 3,
                              availability = 4, isms_id = 'ALL-HW-003'
             WHERE asset_tag = 'PA-460'",
            [],
        )
        .unwrap();
    let (mut h, _f) = gui(w);
    select_asset(&mut h, "PA-460");
    assert!(shows(&h, "機密性 2｜完整性 3｜可用性 4｜資產價值 4"));
    assert!(shows(&h, "ISMS 編號 ALL-HW-003"));
    assert!(shows(&h, "類別 HW03 資安設備"));
}
