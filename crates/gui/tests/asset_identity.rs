//! 20 任務 6.1～6.3：資產對話框的「識別與網路」區塊、資產詳情的識別欄位與管理介面連結、
//! 資產清單的搜尋框（gui-console 的 GUI asset identity input／detail、GUI asset list search）。

mod support;

use cvewatch_core::assets::systems::create_system;
use cvewatch_core::assets::{AssetIp, AssetIpRepo};
use cvewatch_frontend::asset_form::{IP_INVALID, MANAGEMENT_URL_FORMAT};
use cvewatch_gui::dialogs::asset_identity::{ip_label, ADD_IP, PURPOSE_LABEL, SECTION_TITLE};
use cvewatch_gui::screens::asset_list::{group_id, no_match_text, CLEAR_SEARCH, SEARCH_LABEL};
use eframe::egui;
use egui_kittest::kittest::Queryable;
use rusqlite::params;
use support::common::matching::World;
use support::*;

/// seeded_world 加上 SRV-01 與已退役的 NAS-02（財產編號 P-0002、主機名稱 nas02）、NAS-01。
fn identity_world() -> World {
    let w = seeded_world();
    w.asset("SRV-01", "internal", &[("vmware-esxi", "8.0")]);
    w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.conn
        .execute_batch(
            "UPDATE asset SET property_number = 'P-0002', hostname = 'nas02' WHERE asset_tag = 'NAS-02';
             UPDATE asset SET property_number = 'P-0001' WHERE asset_tag = 'NAS-01';",
        )
        .unwrap();
    w.retire("NAS-02");
    w
}

fn asset_id(w: &World, tag: &str) -> i64 {
    w.conn
        .query_row("SELECT id FROM asset WHERE asset_tag = ?1", [tag], |r| {
            r.get(0)
        })
        .unwrap()
}

fn set_ips(w: &World, tag: &str, ips: &[(&str, Option<&str>)]) {
    let list: Vec<AssetIp> = ips
        .iter()
        .map(|(a, p)| AssetIp {
            address: a.parse().unwrap(),
            purpose: p.map(str::to_string),
        })
        .collect();
    AssetIpRepo::new(&w.conn)
        .replace(asset_id(w, tag), &list)
        .unwrap();
}

fn stored_ips(h: &egui_kittest::Harness<'_, cvewatch_gui::GuiApp>, tag: &str) -> Vec<String> {
    let conn = &h.state().ws.conn;
    let id: i64 = conn
        .query_row("SELECT id FROM asset WHERE asset_tag = ?1", [tag], |r| {
            r.get(0)
        })
        .unwrap();
    AssetIpRepo::new(conn)
        .for_asset(id)
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// 第 `index` 個「用途」欄位輸入文字。
fn type_purpose(h: &mut egui_kittest::Harness<'_, cvewatch_gui::GuiApp>, index: usize, text: &str) {
    h.get_all_by_label(PURPOSE_LABEL)
        .nth(index)
        .unwrap()
        .focus();
    h.run();
    h.get_all_by_label(PURPOSE_LABEL)
        .nth(index)
        .unwrap()
        .type_text(text);
    h.run();
}

/// 讓目前取得鍵盤焦點的元件放開焦點（清單的 Enter／Delete 只在沒有焦點時作用）。
fn release_focus(h: &mut egui_kittest::Harness<'_, cvewatch_gui::GuiApp>) {
    let ctx = h.ctx.clone();
    ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    h.run();
}

fn search_for(h: &mut egui_kittest::Harness<'_, cvewatch_gui::GuiApp>, text: &str) {
    replace_text(h, SEARCH_LABEL, text);
}

// ── GUI asset identity input ─────────────────────────────────

/// Scenario: Adding addresses in the dialog
#[test]
fn adding_addresses_in_the_dialog() {
    let (mut h, _f) = gui(identity_world());
    select_asset(&mut h, "SRV-01");
    click(&mut h, "編輯");
    assert!(shows(&h, SECTION_TITLE));
    click(&mut h, ADD_IP);
    // 新列的 IP 欄位取得焦點，可直接輸入
    h.get_by_label(&ip_label(0)).type_text("192.0.2.10");
    h.run();
    type_purpose(&mut h, 0, "管理");
    click(&mut h, ADD_IP);
    replace_text(&mut h, &ip_label(1), "2001:db8::10");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert_eq!(
        stored_ips(&h, "SRV-01"),
        ["192.0.2.10（管理）", "2001:db8::10"]
    );
    assert!(shows(&h, "IP 192.0.2.10（管理）、2001:db8::10"), "詳情列出");
}

/// Scenario: Invalid address on its row
#[test]
fn invalid_address_on_its_row() {
    let (mut h, _f) = gui(identity_world());
    select_asset(&mut h, "SRV-01");
    click(&mut h, "編輯");
    click(&mut h, ADD_IP);
    replace_text(&mut h, &ip_label(0), "192.0.2.10");
    click(&mut h, ADD_IP);
    replace_text(&mut h, &ip_label(1), "192.0.2.300");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_some(), "對話框不關閉");
    let error = h.get_by_label(IP_INVALID).rect();
    let second = h.get_by_label(&ip_label(1)).rect();
    assert!(error.top() >= second.bottom(), "錯誤在第二列下方");
    assert!(stored_ips(&h, "SRV-01").is_empty(), "資料庫不變");
}

/// Scenario: Property number in use
#[test]
fn property_number_in_use() {
    let (mut h, _f) = gui(identity_world());
    select_asset(&mut h, "NAS-01");
    click(&mut h, "編輯");
    replace_text(&mut h, "財產編號", "P-0002");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_some());
    assert!(shows(&h, "已被使用（已退役的 NAS-02）"));
    let n: String = h
        .state()
        .ws
        .conn
        .query_row(
            "SELECT property_number FROM asset WHERE asset_tag = 'NAS-01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, "P-0001", "資料庫不變");
}

/// Scenario: Management address without a scheme
#[test]
fn management_address_without_a_scheme() {
    let (mut h, _f) = gui(identity_world());
    select_asset(&mut h, "SRV-01");
    click(&mut h, "編輯");
    replace_text(&mut h, "管理介面位址", "192.0.2.1");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_some());
    assert!(shows(&h, MANAGEMENT_URL_FORMAT));
    assert!(!shows(&h, "management_url"), "不出現清冊欄名");
}

// ── GUI asset identity detail ────────────────────────────────

/// Scenario: Identity in the detail pane / Opening the management address
#[test]
fn identity_in_the_detail_pane_and_opening_the_link() {
    let w = identity_world();
    w.conn
        .execute(
            "UPDATE asset SET brand = 'Aruba', model = 'XYZ-100', hostname = 'fw-edge',
                              management_url = 'http://192.0.2.1/'
             WHERE asset_tag = 'FW-EDGE'",
            [],
        )
        .unwrap();
    set_ips(&w, "FW-EDGE", &[("192.0.2.1", Some("管理"))]);
    let (mut h, f) = gui(w);
    select_asset(&mut h, "FW-EDGE");
    assert!(shows(&h, "廠牌 Aruba｜型號 XYZ-100｜序號 -｜財產編號 -"));
    assert!(shows(&h, "主機名稱 fw-edge"));
    assert!(shows(&h, "IP 192.0.2.1（管理）"));
    click(&mut h, "http://192.0.2.1/");
    assert_eq!(*f.rec.opened.borrow(), ["http://192.0.2.1/"]);
    assert_eq!(status(&h), "已開啟 http://192.0.2.1/");
}

/// Scenario: Asset without a management address
#[test]
fn asset_without_a_management_address() {
    let (mut h, _f) = gui(identity_world());
    select_asset(&mut h, "NAS-01");
    assert!(shows(&h, "主機名稱 -｜管理介面位址"));
    assert!(shows(&h, "IP -"));
    assert!(
        h.query_all_by_label_contains("http").next().is_none(),
        "不顯示連結"
    );
}

// ── GUI asset list search ────────────────────────────────────

/// Scenario: Searching by address / by property number with letter case / No matching asset
#[test]
fn searching_by_address_and_property_number() {
    let w = identity_world();
    set_ips(&w, "SRV-01", &[("192.0.2.10", None)]);
    let (mut h, _f) = gui(w);
    ctrl(&mut h, egui::Key::Num3);
    search_for(&mut h, "192.0.2.1");
    assert!(listed(&h, "SRV-01"));
    assert!(!listed(&h, "FW-EDGE") && !listed(&h, "NAS-01"));
    search_for(&mut h, "p-00");
    assert!(listed(&h, "NAS-01"));
    assert!(!listed(&h, "SRV-01"));
    search_for(&mut h, "zzz");
    assert!(shows(&h, &no_match_text("zzz")));
    click(&mut h, CLEAR_SEARCH);
    assert!(
        listed(&h, "SRV-01") && listed(&h, "FW-EDGE"),
        "清除後全部列出"
    );
}

/// Scenario: Search combined with the retired filter
#[test]
fn search_combined_with_the_retired_filter() {
    let (mut h, _f) = gui(identity_world());
    ctrl(&mut h, egui::Key::Num3);
    search_for(&mut h, "nas0");
    assert!(!listed(&h, "NAS-02"));
    click(&mut h, "顯示已退役");
    assert!(listed(&h, "NAS-02"), "勾選後以退役樣式列出");
    assert!(shows(&h, "（已退役）"));
}

/// Scenario: Collapsed group while searching
#[test]
fn collapsed_group_while_searching() {
    let w = identity_world();
    create_system(&w.conn, "NET", "網路服務").unwrap();
    w.conn
        .execute(
            "UPDATE asset SET system_code = 'NET', hostname = 'fw-edge' WHERE asset_tag = ?1",
            params!["FW-EDGE"],
        )
        .unwrap();
    let (mut h, _f) = gui(w);
    ctrl(&mut h, egui::Key::Num3);
    let ctx = h.ctx.clone();
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        &ctx,
        group_id("NET"),
        true,
    );
    state.set_open(false);
    state.store(&ctx);
    h.run();
    assert!(!listed(&h, "FW-EDGE"), "NET 組已收合");
    search_for(&mut h, "fw-e");
    assert!(listed(&h, "FW-EDGE"), "搜尋中展開");
    click(&mut h, CLEAR_SEARCH);
    assert!(!listed(&h, "FW-EDGE"), "清除搜尋後恢復收合");
}

/// 被搜尋篩掉的選取資產：詳情保留，但 Delete 不開啟退役確認（20 code review）
#[test]
fn keys_do_not_act_on_an_asset_hidden_by_the_search() {
    let (mut h, _f) = gui(identity_world());
    select_asset(&mut h, "FW-EDGE");
    search_for(&mut h, "nas");
    assert!(!listed(&h, "FW-EDGE"));
    release_focus(&mut h);
    h.key_press(egui::Key::Delete);
    h.run();
    assert!(h.state().asset_dialog.is_none(), "不開啟退役確認");
    assert_eq!(h.state().assets_selected.as_deref(), Some("FW-EDGE"));
    click(&mut h, CLEAR_SEARCH);
    release_focus(&mut h);
    h.key_press(egui::Key::Delete);
    h.run();
    assert!(
        h.state().asset_dialog.is_some(),
        "清單中看得到時 Delete 照常作用"
    );
}
