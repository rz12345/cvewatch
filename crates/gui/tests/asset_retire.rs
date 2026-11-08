//! 10 任務 7.3：刪除（標為退役）、顯示已退役、恢復與永久刪除確認。

mod support;

use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use support::*;

/// seeded world 加上 NAS-02（DSM）。
fn nas_world(retired: bool) -> common::matching::World {
    let w = seeded_world();
    w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.1-69057")]);
    if retired {
        w.retire("NAS-02");
    }
    w
}

fn listed(h: &Harness<'_, cvewatch_gui::GuiApp>, text: &str) -> bool {
    h.query_all_by_label_contains(text)
        .any(|n| n.accesskit_node().role() != eframe::egui::accesskit::Role::Label)
}

fn purge_button_disabled(h: &Harness<'_, cvewatch_gui::GuiApp>) -> bool {
    h.get_all_by_label("永久刪除")
        .last()
        .expect("button")
        .accesskit_node()
        .is_disabled()
}

#[test]
fn asset_retire_deleting_from_the_asset_list() {
    let (mut h, f) = gui(nas_world(false));
    select_asset(&mut h, "NAS-02");
    click(&mut h, "刪除");
    assert!(shows(&h, "命中與處理紀錄都保留"), "確認對話框說明退役語意");
    click(&mut h, "標為退役");
    assert!(h.state().asset_dialog.is_none());
    assert!(h.state().ws.data.asset("NAS-02").is_none());
    assert!(!listed(&h, "NAS-02"), "預設清單不再列出");
    click(&mut h, "顯示已退役");
    assert!(listed(&h, "NAS-02  NAS-02（已退役）"), "以退役樣式列出");
    assert_eq!(match_calls(&f), 1);
    // 已退役資產只提供恢復與永久刪除
    assert!(h.query_by_label("編輯").is_none());
    assert!(h.query_by_label("恢復").is_some());
}

#[test]
fn asset_retire_cancel_keeps_the_asset() {
    let (mut h, _f) = gui(nas_world(false));
    select_asset(&mut h, "NAS-02");
    click(&mut h, "刪除");
    click_last(&mut h, "取消");
    assert!(h.state().ws.data.asset("NAS-02").is_some());
}

#[test]
fn asset_retire_purge_confirmation_requires_the_tag() {
    let (mut h, _f) = gui(nas_world(true));
    h.state_mut().assets_show_retired = true;
    select_asset(&mut h, "NAS-02");
    click(&mut h, "永久刪除");
    assert!(shows(&h, "元件 1 個、命中 0 筆"));
    assert!(shows(&h, "cvewatch db backup"));
    assert!(purge_button_disabled(&h));
    replace_text(&mut h, "輸入資產編號確認", "NAS-0");
    assert!(purge_button_disabled(&h), "不完整的編號維持停用");
    replace_text(&mut h, "輸入資產編號確認", "nas-02");
    assert!(purge_button_disabled(&h), "區分大小寫");
    replace_text(&mut h, "輸入資產編號確認", "NAS-02");
    assert!(!purge_button_disabled(&h));
    click_last(&mut h, "永久刪除");
    assert!(h.state().asset_dialog.is_none());
    assert!(h.state().ws.data.any_asset("NAS-02").is_none());
    assert!(!listed(&h, "NAS-02"), "勾選顯示已退役也不再列出");
    assert!(status(&h).contains("已永久刪除 NAS-02"), "{}", status(&h));
}

#[test]
fn asset_retire_restore_returns_to_the_default_list() {
    let (mut h, f) = gui(nas_world(true));
    h.state_mut().assets_show_retired = true;
    select_asset(&mut h, "NAS-02");
    click(&mut h, "恢復");
    assert!(h.state().ws.data.asset("NAS-02").is_some());
    h.state_mut().assets_show_retired = false;
    h.run();
    assert!(listed(&h, "NAS-02"), "回到預設清單");
    assert_eq!(match_calls(&f), 1, "恢復後比對");
}
