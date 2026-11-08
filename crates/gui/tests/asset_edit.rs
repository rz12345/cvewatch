//! 10 任務 6.4：資產編輯對話框（新增、欄位錯誤、修改版本、取消、未辨識產品改為目錄產品）。

mod support;

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
    replace_text(&mut h, "資產編號", "FW-02");
    replace_text(&mut h, "名稱", "fw02");
    h.get_by_label("選擇類別").click();
    h.run();
    click_option(&mut h, "security");
    h.get_by_label("加入元件").click();
    h.run();
    click_option(&mut h, "PAN-OS（Palo Alto Networks）");
    replace_text(&mut h, "paloalto-panos 版本", "11.2.3");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert_eq!(h.state().assets_selected.as_deref(), Some("FW-02"));
    let entry = h.state().ws.data.asset("FW-02").expect("清單出現 FW-02");
    assert_eq!(entry.components[0].version_raw, "11.2.3");
    assert_eq!(entry.asset.record.criticality, 2);
    assert!(shows(&h, "FW-02  fw02"));
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
    assert!(shows(&h, "缺少 asset_tag"));
    assert!(shows(&h, "至少需要一個元件"));
    assert_eq!(asset_count(&h), before, "資料庫不變");
}

#[test]
fn asset_edit_editing_a_component_version() {
    let (mut h, _f) = gui(seeded_world());
    edit_version(&mut h, "PA-460", "paloalto-panos", "11.1.6");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert!(shows(&h, "paloalto-panos 11.1.6"), "詳情顯示新版本");
    assert!(shows(&h, "先前版本 11.1.2"), "版本歷史多一筆");
}

#[test]
fn asset_edit_cancel_leaves_the_database_unchanged() {
    let (mut h, f) = gui(seeded_world());
    select_asset(&mut h, "PA-460");
    click(&mut h, "編輯");
    replace_text(&mut h, "名稱", "改過的名稱");
    replace_text(&mut h, "paloalto-panos 版本", "99.0");
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
    h.get_by_label("Synology DS920+ 7.2 處置").click();
    h.run();
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
    replace_text(&mut h, "名稱", "fw02");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().asset_dialog.is_some(), "有修改時 Esc 不關閉");
    assert!(h.state().has_unsaved(), "關閉視窗前需確認");
}
