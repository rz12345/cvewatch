//! 10 任務 6.4、11 任務 3.1～3.4：資產編輯對話框（新增、欄位錯誤、修改版本、取消、未辨識產品改為
//! 目錄產品、單一標籤與必填標示、關閉時清除狀態列、加入元件的焦點流程）。

mod support;

use cvewatch_gui::dialogs::asset_edit::AssetEdit;
use cvewatch_gui::dialogs::AssetDialog;
use eframe::egui;
use egui_kittest::kittest::Queryable;
use support::*;

fn asset_count(h: &egui_kittest::Harness<'_, cvewatch_gui::GuiApp>) -> i64 {
    h.state()
        .ws
        .conn
        .query_row("SELECT count(*) FROM asset", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn asset_edit_adding_an_asset_from_the_window() {
    let (mut h, f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "新增資產");
    replace_text(&mut h, "資產編號 *", "FW-02");
    replace_text(&mut h, "名稱 *", "fw02");
    assert!(!shows(&h, "選擇類別"), "下拉選單只有左側一個標籤");
    h.get_by_label("類別 *").click();
    h.run();
    click_option(&mut h, "HW03 資安設備");
    add_component(&mut h, "pan", "PAN-OS（Palo Alto Networks）");
    replace_text(&mut h, "PAN-OS 版本", "11.2.3");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert_eq!(h.state().assets_selected.as_deref(), Some("FW-02"));
    let entry = h.state().ws.data.asset("FW-02").expect("清單出現 FW-02");
    assert_eq!(entry.components[0].version_raw, "11.2.3");
    assert_eq!(
        entry.asset.record.cia,
        cvewatch_core::assets::Cia::new(2, 3, 4),
        "C/I/A 依類別"
    );
    assert_eq!(entry.asset.record.category.as_str(), "HW03", "儲存代碼");
    assert!(listed(&h, "FW-02") && listed(&h, "fw02"), "清單列出 FW-02");
    assert_eq!(match_calls(&f), 1, "儲存後背景比對");
}

#[test]
fn asset_edit_invalid_field_keeps_the_dialog_open() {
    let (mut h, _f) = gui(seeded_world());
    let before = asset_count(&h);
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "新增資產");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_some(), "對話框不關閉");
    assert!(shows(&h, "必填"));
    assert!(!shows(&h, "asset_tag"), "不出現清冊的英文欄位名");
    assert!(shows(&h, "至少需要一個元件"));
    assert!(shows(&h, "請修正標示的欄位"), "對話框底部摘要");
    assert!(h.state().status.is_none(), "本地驗證失敗不寫狀態列");
    assert_eq!(asset_count(&h), before, "資料庫不變");
}

#[test]
fn asset_edit_editing_a_component_version() {
    let (mut h, _f) = gui(seeded_world());
    edit_version(&mut h, "PA-460", "PAN-OS", "11.1.6");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert!(shows(&h, "PAN-OS 11.1.6"), "詳情顯示新版本");
    assert!(shows(&h, "先前版本 11.1.2"), "版本歷史多一筆");
}

#[test]
fn asset_edit_cancel_leaves_the_database_unchanged() {
    let (mut h, f) = gui(seeded_world());
    select_asset(&mut h, "PA-460");
    click(&mut h, "編輯");
    replace_text(&mut h, "名稱 *", "改過的名稱");
    replace_text(&mut h, "PAN-OS 版本", "99.0");
    click_last(&mut h, "取消");
    assert!(h.state().asset_dialog.is_none());
    let (name, version): (String, String) = h
        .state()
        .ws
        .conn
        .query_row(
            "SELECT a.name, c.version_raw FROM asset a JOIN component c ON c.asset_id = a.id
             WHERE a.asset_tag = 'PA-460'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((name.as_str(), version.as_str()), ("PA-460", "11.1.2"));
    assert_eq!(match_calls(&f), 0);
}

#[test]
fn asset_edit_mapping_an_unresolved_product() {
    let (mut h, _f) = gui(seeded_world());
    select_asset(&mut h, "PVE-01");
    assert!(shows(&h, "Synology DS920+ 7.2"));
    click(&mut h, "編輯");
    assert!(
        h.query_by_label("Proxmox VE 版本").is_some(),
        "元件列用顯示名稱"
    );
    // 原文與版本 → 處置選單 → 刪除
    let combo = h
        .get_all_by_label("Synology DS920+ 7.2")
        .last()
        .unwrap()
        .rect();
    let delete = h.get_by_label("刪除「Synology DS920+」").rect();
    assert!(combo.right() <= delete.left(), "刪除在處置選單之後");
    // 原文與版本是處置選單的標籤（詳情中同名的文字先繪製）
    click_last(&mut h, "Synology DS920+ 7.2");
    click_option(&mut h, "改為 DiskStation Manager（Synology）");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    let entry = h.state().ws.data.asset("PVE-01").unwrap();
    assert!(entry.unresolved.is_empty());
    assert!(entry
        .components
        .iter()
        .any(|c| c.label() == "synology-dsm" && c.version_raw == "7.2"));
    assert!(!shows(&h, "Synology DS920+ 7.2"), "詳情不再列出該筆");
}

#[test]
fn asset_edit_package_rows_are_read_only() {
    let w = seeded_world();
    w.package(
        "PVE-01",
        "pkg:deb/debian/openssl",
        "3.0.11-1",
        common::matching::T0,
    );
    let (mut h, _f) = gui(w);
    select_asset(&mut h, "PVE-01");
    click(&mut h, "編輯");
    assert!(shows(&h, "由套件清單管理"));
    assert!(
        h.query_by_label("pkg:deb/debian/openssl 版本").is_none(),
        "套件列沒有可編輯欄位"
    );
    assert!(h.query_by_label("移除 pkg:deb/debian/openssl").is_none());
}

#[test]
fn asset_edit_escape_does_not_discard_unsaved_input() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "新增資產");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().asset_dialog.is_none(), "沒有修改時 Esc 關閉");
    click(&mut h, "新增資產");
    replace_text(&mut h, "名稱 *", "fw02");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().asset_dialog.is_some(), "有修改時 Esc 不關閉");
    assert!(h.state().has_unsaved(), "關閉視窗前需確認");
}

/// 開啟「加入元件」、輸入搜尋字後點選產品（選擇器只列前 12 筆）。
fn add_component(
    h: &mut egui_kittest::Harness<'_, cvewatch_gui::GuiApp>,
    query: &str,
    option: &str,
) {
    h.get_by_label("加入元件").click();
    h.run();
    h.get_by_label("搜尋產品").type_text(query);
    h.run();
    click_option(h, option);
}

fn dialog<'a>(h: &'a egui_kittest::Harness<'_, cvewatch_gui::GuiApp>) -> &'a AssetEdit {
    match &h.state().asset_dialog {
        Some(AssetDialog::Edit(e)) => e,
        _ => panic!("編輯對話框未開啟"),
    }
}

#[test]
fn asset_edit_cancelled_dialog_leaves_no_error_behind() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "新增資產");
    click(&mut h, "儲存");
    assert!(shows(&h, "必填"));
    click_last(&mut h, "取消");
    assert!(h.state().asset_dialog.is_none());
    assert!(h.state().status.is_none(), "狀態列不顯示欄位錯誤");
}

#[test]
fn asset_edit_cancel_clears_the_write_failure_from_the_status_bar() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "新增資產");
    replace_text(&mut h, "資產編號 *", "PA-460");
    replace_text(&mut h, "名稱 *", "重複");
    h.get_by_label("類別 *").click();
    h.run();
    click_option(&mut h, "HW03 資安設備");
    add_component(&mut h, "pan", "PAN-OS（Palo Alto Networks）");
    replace_text(&mut h, "PAN-OS 版本", "11.2.3");
    click(&mut h, "儲存");
    let failure = dialog(&h).failure.clone().expect("core 拒絕重複的資產編號");
    assert_eq!(status(&h), failure, "寫入失敗也顯示在狀態列");
    // 再次儲存時本地驗證失敗：上一次的寫入失敗不再顯示
    replace_text(&mut h, "名稱 *", "x");
    h.key_press(egui::Key::Backspace);
    h.run();
    click(&mut h, "儲存");
    assert!(shows(&h, "必填"));
    assert!(dialog(&h).failure.is_none(), "對話框不留舊的寫入失敗");
    assert!(h.state().status.is_none(), "狀態列也清除");
    click_last(&mut h, "取消");
    assert!(h.state().status.is_none(), "{}", status(&h));
}

#[test]
fn asset_edit_keyboard_flow_when_adding_a_component() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "新增資產");
    h.get_by_label("加入元件").click();
    h.run();
    assert!(
        h.get_by_label("搜尋產品").is_focused(),
        "開啟時搜尋框取得焦點"
    );
    h.get_by_label("搜尋產品").type_text("pan");
    h.run();
    assert_eq!(dialog(&h).add_picker.search, "pan");
    click_option(&mut h, "PAN-OS（Palo Alto Networks）");
    assert!(h.get_by_label("PAN-OS 版本").is_focused(), "焦點移到版本欄");
    h.get_by_label("PAN-OS 版本").type_text("11.2.3");
    h.run();
    let form = &dialog(&h).form;
    assert_eq!(form.components[0].product_key, "paloalto-panos");
    assert_eq!(form.components[0].version, "11.2.3");
}

#[test]
fn asset_edit_duplicate_rows_have_separate_version_fields() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "新增資產");
    for _ in 0..2 {
        add_component(&mut h, "pan", "PAN-OS（Palo Alto Networks）");
    }
    assert_eq!(h.get_all_by_label("PAN-OS 版本").count(), 2);
    // 第二列剛加入，焦點在它的版本欄
    h.get_all_by_label("PAN-OS 版本")
        .last()
        .unwrap()
        .type_text("11.2.3");
    h.run();
    let versions: Vec<_> = dialog(&h)
        .form
        .components
        .iter()
        .map(|c| c.version.clone())
        .collect();
    assert_eq!(versions, ["", "11.2.3"]);
    click(&mut h, "儲存");
    assert!(shows(&h, "已有產品 PAN-OS，請改為修改版本"));
}

/// 新增資產並填入必填欄位與一個元件（類別由呼叫端選擇）。
fn new_asset_with_component(h: &mut egui_kittest::Harness<'_, cvewatch_gui::GuiApp>, tag: &str) {
    ctrl(h, egui::Key::Num3);
    click(h, "新增資產");
    replace_text(h, "資產編號 *", tag);
    replace_text(h, "名稱 *", tag);
    add_component(h, "pan", "PAN-OS（Palo Alto Networks）");
    replace_text(h, "PAN-OS 版本", "11.2.3");
}

/// C/I/A 下拉選單目前顯示的文字。
fn rating_shown(h: &egui_kittest::Harness<'_, cvewatch_gui::GuiApp>, label: &str) -> String {
    h.get_by_label(label).value().unwrap_or_default()
}

fn choose_class(h: &mut egui_kittest::Harness<'_, cvewatch_gui::GuiApp>, label: &str) {
    h.get_by_label("類別 *").click();
    h.run();
    click_option(h, label);
}

/// Scenario: Template values follow the chosen class
#[test]
fn asset_edit_template_values_follow_the_chosen_class() {
    let (mut h, _f) = gui(seeded_world());
    new_asset_with_component(&mut h, "NAS-09");
    assert_eq!(rating_shown(&h, "機密性"), "依類別（無預設）", "尚未選類別");
    choose_class(&mut h, "HW05 儲存設備");
    assert_eq!(
        ["機密性", "完整性", "可用性"].map(|l| rating_shown(&h, l)),
        ["依類別（2）", "依類別（3）", "依類別（4）"]
    );
    let label = h.get_by_label("資產價值").rect();
    assert!(
        h.query_all_by_label("4")
            .any(|n| (n.rect().center().y - label.center().y).abs() < 8.0
                && n.rect().left() > label.left()),
        "即時顯示資產價值 4"
    );
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    let record = &h
        .state()
        .ws
        .data
        .asset("NAS-09")
        .expect("NAS-09")
        .asset
        .record;
    assert_eq!(record.category.as_str(), "HW05");
    assert_eq!(record.cia, cvewatch_core::assets::Cia::new(2, 3, 4));
}

/// Scenario: Application system needs ratings
#[test]
fn asset_edit_application_system_needs_ratings() {
    use cvewatch_core::assets::columns::StandardField as F;
    let (mut h, _f) = gui(seeded_world());
    let before = asset_count(&h);
    new_asset_with_component(&mut h, "APP-01");
    choose_class(&mut h, "SW01 應用系統");
    assert_eq!(rating_shown(&h, "可用性"), "依類別（無預設）");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_some(), "對話框不關閉");
    let errors = &dialog(&h).errors;
    for field in [F::Confidentiality, F::Integrity, F::Availability] {
        assert_eq!(errors.field(field), Some("必填"), "{field:?} {errors:?}");
    }
    assert_eq!(
        h.query_all_by_label("必填").count(),
        3,
        "三個欄位旁顯示必填"
    );
    assert_eq!(asset_count(&h), before, "資料庫不變");
    // 選擇 C/I/A 後可儲存
    for label in ["機密性", "完整性", "可用性"] {
        h.get_by_label(label).click();
        h.run();
        click_option(&mut h, "3");
    }
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    let record = &h
        .state()
        .ws
        .data
        .asset("APP-01")
        .expect("APP-01")
        .asset
        .record;
    assert_eq!(record.cia, cvewatch_core::assets::Cia::new(3, 3, 3));
}

/// Scenario: ISMS ID with the wrong major class
#[test]
fn asset_edit_isms_id_with_the_wrong_major_class() {
    use cvewatch_core::assets::columns::StandardField as F;
    use cvewatch_frontend::asset_form::ISMS_CLASS_MISMATCH;
    let (mut h, _f) = gui(seeded_world());
    let before = asset_count(&h);
    new_asset_with_component(&mut h, "FW-09");
    choose_class(&mut h, "HW03 資安設備");
    replace_text(&mut h, "ISMS 編號", "ALL-SW-001");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_some(), "對話框不關閉");
    assert_eq!(
        dialog(&h).errors.field(F::IsmsId),
        Some(ISMS_CLASS_MISMATCH)
    );
    assert!(shows(&h, ISMS_CLASS_MISMATCH), "錯誤顯示在欄位旁");
    assert_eq!(asset_count(&h), before, "資料庫不變");
}

// ── 14 任務 5.5：產品選擇器 ──────────────────────────────────

fn open_add_component(h: &mut egui_kittest::Harness<'_, cvewatch_gui::GuiApp>) {
    ctrl(h, egui::Key::Num3);
    click(h, "新增資產");
    h.get_by_label("加入元件").click();
    h.run();
}

/// Scenario: Picking with the keyboard
#[test]
fn asset_edit_picking_a_product_with_the_keyboard() {
    let (mut h, _f) = gui(seeded_world());
    open_add_component(&mut h);
    assert!(h.get_by_label("搜尋產品").is_focused());
    h.get_by_label("搜尋產品").type_text("cisco ios");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    let form = &dialog(&h).form;
    assert_eq!(form.components.len(), 1);
    assert_eq!(form.components[0].product_key, "cisco-ios");
    assert!(
        h.get_by_label("Cisco IOS 版本").is_focused(),
        "焦點移到版本欄"
    );
    // ↓ 移動反白後 Enter：選第二筆
    h.get_by_label("加入元件").click();
    h.run();
    h.get_by_label("搜尋產品").type_text("cisco ios");
    h.run();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(dialog(&h).form.components[1].product_key, "cisco-iosxe");
}

/// Scenario: Escape leaves components unchanged
#[test]
fn asset_edit_escape_closes_the_picker_without_changes() {
    let (mut h, _f) = gui(seeded_world());
    open_add_component(&mut h);
    h.key_press(egui::Key::ArrowDown);
    h.run();
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.query_by_label("搜尋產品").is_none(), "選擇器關閉");
    assert!(h.state().asset_dialog.is_some(), "Esc 只關閉選擇器");
    assert!(dialog(&h).form.components.is_empty(), "元件清單不變");
}

/// Scenario: Too many results（內建目錄 78 項，一次列 12 項）
#[test]
fn asset_edit_picker_shows_the_result_limit() {
    let (mut h, _f) = gui(seeded_world());
    open_add_component(&mut h);
    assert!(shows(&h, "還有 66 項，再輸入文字縮小範圍"));
    h.get_by_label("搜尋產品").type_text("zzz");
    h.run();
    assert!(shows(&h, "沒有相符的產品"));
}

/// PVE-01 加入一筆未辨識產品後開啟編輯。
fn edit_with_unresolved(raw: &str) -> egui_kittest::Harness<'static, cvewatch_gui::GuiApp> {
    let w = seeded_world();
    w.conn
        .execute(
            "INSERT INTO asset_unresolved (asset_id, product_raw, version_raw, seen_at)
             VALUES ((SELECT id FROM asset WHERE asset_tag = 'PVE-01'), ?1, '1.0', ?2)",
            rusqlite::params![raw, common::matching::T0],
        )
        .unwrap();
    let (mut h, _f) = gui(w);
    select_asset(&mut h, "PVE-01");
    click(&mut h, "編輯");
    h
}

/// Scenario: Suggestion for a typo
#[test]
fn asset_edit_unresolved_picker_suggests_similar_products() {
    let mut h = edit_with_unresolved("Synolgy DSM");
    click_last(&mut h, "Synolgy DSM 1.0");
    assert!(shows(&h, "建議"));
    let keep = h.get_all_by_label("保留").last().unwrap().rect();
    let suggestion = h
        .get_all_by_label("改為 DiskStation Manager（Synology）")
        .last()
        .unwrap()
        .rect();
    let search = h.get_by_label("搜尋產品").rect();
    assert!(search.top() < keep.top() && keep.top() < suggestion.top());
    assert_eq!(
        h.query_all_by_label("改為 DiskStation Manager（Synology）")
            .count(),
        1,
        "建議不在搜尋結果中重複列出"
    );
    click_option(&mut h, "改為 DiskStation Manager（Synology）");
    assert_eq!(
        h.get_all_by_label("Synolgy DSM 1.0")
            .last()
            .unwrap()
            .value()
            .as_deref(),
        Some("改為 DiskStation Manager"),
        "處置顯示改為 DiskStation Manager"
    );
    let form = &dialog(&h).form;
    let row = form
        .unresolved
        .iter()
        .find(|u| u.product_raw == "Synolgy DSM")
        .unwrap();
    assert_eq!(
        row.action,
        cvewatch_frontend::asset_form::UnresolvedAction::MapTo("synology-dsm".into())
    );
}

/// Scenario: Searching beyond suggestions
#[test]
fn asset_edit_unresolved_picker_searches_beyond_suggestions() {
    let mut h = edit_with_unresolved("核心交換器");
    click_last(&mut h, "核心交換器 1.0");
    assert!(!shows(&h, "建議"), "沒有相似產品");
    h.get_by_label("搜尋產品").type_text("cx");
    h.run();
    click_option(&mut h, "改為 AOS-CX（HPE Aruba Networking）");
    let row = dialog(&h)
        .form
        .unresolved
        .iter()
        .find(|u| u.product_raw == "核心交換器")
        .unwrap()
        .action
        .clone();
    assert_eq!(
        row,
        cvewatch_frontend::asset_form::UnresolvedAction::MapTo("hpe-aoscx".into())
    );
}
