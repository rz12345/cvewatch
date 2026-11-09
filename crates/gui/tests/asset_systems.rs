//! 16 任務 5.3～5.5：資產清單依資產說明分組、「資產說明…」對話框，以及資產對話框的
//! ISMS 管理區塊與詳情欄位。

mod support;

use cvewatch_core::assets::systems::create_system;
use cvewatch_frontend::asset_form::ISMS_SYSTEM_MISMATCH;
use cvewatch_frontend::workspace::systems::delete_blocked;
use cvewatch_gui::dialogs::asset_systems::{NEW_CODE_LABEL, NEW_NAME_LABEL};
use cvewatch_gui::dialogs::AssetDialog;
use cvewatch_gui::screens::asset_list::{group_id, group_open_text};
use eframe::egui;
use egui_kittest::kittest::Queryable;
use rusqlite::params;
use support::*;

fn assign(w: &support::common::matching::World, tag: &str, code: &str) {
    w.conn
        .execute(
            "UPDATE asset SET system_code = ?2 WHERE asset_tag = ?1",
            params![tag, code],
        )
        .unwrap();
}

/// FW-EDGE、FW-CORE 歸屬 NET；PVE-01 未歸屬。
fn grouped_world() -> support::common::matching::World {
    let w = seeded_world();
    create_system(&w.conn, "NET", "網路服務").unwrap();
    assign(&w, "FW-EDGE", "NET");
    assign(&w, "FW-CORE", "NET");
    w
}

fn open_in(h: &egui_kittest::Harness<'_, cvewatch_gui::GuiApp>, tags: &[&str]) -> usize {
    tags.iter()
        .map(|t| h.state().ws.data.asset(t).unwrap().open_findings)
        .sum()
}

/// Scenario: Assets under their systems
#[test]
fn assets_are_listed_under_their_systems() {
    let (mut h, _f) = gui(grouped_world());
    ctrl(&mut h, egui::Key::Num3);
    let net = h.get_by_label("NET 網路服務（2）").rect();
    let unassigned = h.get_by_label("未歸屬資產說明（1）").rect();
    assert!(net.top() < unassigned.top(), "未歸屬的組在最後");
    let pve = list_cells(&h, "PVE-01")[0].rect();
    let fw = list_cells(&h, "FW-EDGE")[0].rect();
    assert!(
        fw.top() > net.top() && fw.top() < unassigned.top(),
        "FW-EDGE 在 NET 組內"
    );
    assert!(pve.top() > unassigned.top(), "PVE-01 在未歸屬組內");
}

/// Scenario: Collapsed group shows open findings
#[test]
fn collapsed_group_shows_open_findings() {
    let (mut h, _f) = gui(grouped_world());
    ctrl(&mut h, egui::Key::Num3);
    let open = open_in(&h, &["FW-EDGE", "FW-CORE"]);
    assert!(open > 0);
    assert!(shows(&h, &group_open_text(open)));
    let ctx = h.ctx.clone();
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        &ctx,
        group_id("NET"),
        true,
    );
    state.set_open(false);
    state.store(&ctx);
    h.run();
    assert!(!listed(&h, "FW-EDGE"), "收合後組內資產不顯示");
    assert!(shows(&h, &group_open_text(open)), "標題仍顯示未處理數");
    assert!(listed(&h, "PVE-01"));
}

/// Scenario: Group with only retired assets；跨組選取後 Enter 開啟編輯
#[test]
fn group_with_only_retired_assets_and_keyboard() {
    let w = grouped_world();
    create_system(&w.conn, "SYS", "核心系統").unwrap();
    assign(&w, "PVE-01", "SYS");
    w.retire("PVE-01");
    let (mut h, _f) = gui(w);
    ctrl(&mut h, egui::Key::Num3);
    assert!(!shows(&h, "SYS 核心系統"));
    h.state_mut().assets_show_retired = true;
    h.run();
    assert!(shows(&h, "SYS 核心系統（1）"));
    h.state_mut().assets_show_retired = false;
    h.state_mut().assets_selected = Some("FW-CORE".into());
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    match &h.state().asset_dialog {
        Some(AssetDialog::Edit(e)) => assert_eq!(e.form.asset_tag, "FW-CORE"),
        _ => panic!("Enter 應開啟 FW-CORE 的編輯"),
    }
}

/// Scenario: Adding a system from the window / Duplicate name in the dialog
#[test]
fn systems_dialog_adds_and_reports_duplicates() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "資產說明…");
    replace_text(&mut h, NEW_CODE_LABEL, "net");
    replace_text(&mut h, NEW_NAME_LABEL, "網路服務");
    click(&mut h, "新增");
    assert_eq!(h.state().ws.data.systems.label("NET"), "NET 網路服務");
    assert!(h.query_by_label("網路服務").is_some(), "清單列出名稱");
    assert_eq!(
        h.state()
            .ws
            .system_usage(&cvewatch_core::assets::SystemCode::parse("NET").unwrap()),
        0
    );
    replace_text(&mut h, NEW_CODE_LABEL, "WEB");
    replace_text(&mut h, NEW_NAME_LABEL, "網路服務");
    click(&mut h, "新增");
    assert!(
        matches!(h.state().asset_dialog, Some(AssetDialog::Systems(_))),
        "對話框不關閉"
    );
    assert!(shows(&h, "名稱「網路服務」已被資產說明 NET 使用"));
    assert!(
        h.state().ws.data.systems.find("WEB").is_none(),
        "資料庫不變"
    );
}

/// Scenario: System still in use
#[test]
fn system_in_use_cannot_be_deleted() {
    let w = grouped_world();
    assign(&w, "PVE-01", "NET");
    let (mut h, _f) = gui(w);
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "資產說明…");
    assert!(is_disabled(&h, "刪除 NET"));
    assert!(shows(&h, "3"), "資產數欄");
    // 停用按鈕的滑鼠提示需要停留時間，kittest 無法穩定觸發；確認提示文字與資產數一致
    assert_eq!(delete_blocked(3), "仍有 3 個資產歸屬此資產說明，無法刪除");
    let r = h.state_mut().ws.delete_system(
        &cvewatch_core::assets::SystemCode::parse("NET").unwrap(),
        &[],
    );
    assert_eq!(r.report.message, delete_blocked(3));
}

/// Scenario: ISMS ID from another system（對話框）
#[test]
fn isms_id_from_another_system_keeps_the_dialog_open() {
    let (mut h, _f) = gui(grouped_world());
    select_asset(&mut h, "PVE-01");
    click(&mut h, "編輯");
    h.get_by_label("資產說明").click();
    h.run();
    click_option(&mut h, "NET 網路服務");
    replace_text(&mut h, "ISMS 編號", "SYS-HW-001");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_some(), "對話框不關閉");
    assert!(shows(&h, ISMS_SYSTEM_MISMATCH));
    let r = &h.state().ws.data.asset("PVE-01").unwrap().asset.record;
    assert_eq!(r.system_code, None, "資料庫不變");
}

/// Scenario: Protection level follows compliance / Personal data follows the class
#[test]
fn protection_level_and_personal_data_in_the_dialog() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "新增資產");
    h.get_by_label("類別 *").click();
    h.run();
    click_option(&mut h, "HW04 週邊設備");
    let shown = |h: &egui_kittest::Harness<'_, cvewatch_gui::GuiApp>, label: &str| {
        h.get_by_label(label).value().unwrap_or_default()
    };
    assert_eq!(shown(&h, "是否含個人資料"), "依類別（否）");
    assert_eq!(shown(&h, "法規遵循性"), "未評定");
    assert!(shows(&h, "未評定"), "防護等級未評定");
    h.get_by_label("法規遵循性").click();
    h.run();
    click_option(&mut h, "4");
    let Some(AssetDialog::Edit(e)) = &h.state().asset_dialog else {
        panic!("對話框應開啟");
    };
    let classes = &h.state().ws.settings.classes;
    assert_eq!(e.form.asset_value(classes), Some(3));
    assert_eq!(e.form.protection_level(classes), Some(4));
    assert_eq!(shown(&h, "法規遵循性"), "4");
    assert!(!shows(&h, "未評定"), "防護等級不再是未評定");
}

/// Scenario: Administrative fields in the detail pane
#[test]
fn detail_pane_lists_admin_fields() {
    let w = seeded_world();
    create_system(&w.conn, "SYS", "核心系統").unwrap();
    w.conn
        .execute(
            "UPDATE asset SET system_code = 'SYS', custodian = '王小明' WHERE asset_tag = 'FW-EDGE'",
            [],
        )
        .unwrap();
    let (mut h, _f) = gui(w);
    select_asset(&mut h, "FW-EDGE");
    assert!(shows(&h, "資產說明 SYS 核心系統"));
    assert!(shows(&h, "權責單位 -｜保管人 王小明｜使用單位 -"));
    assert!(shows(
        &h,
        "法規遵循性 未評定｜防護等級 未評定｜是否含個人資料 否"
    ));
}
