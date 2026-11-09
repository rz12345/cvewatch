//! 16 任務 5.3～5.5：資產清單依資產說明分組、資產說明對話框，以及資產對話框的
//! ISMS 管理區塊與詳情欄位；19 任務 4.1～4.2：編輯資產說明代碼與確認畫面。

mod support;

use cvewatch_core::assets::systems::create_system;
use cvewatch_core::assets::systems_edit::PLAN_CHANGED;
use cvewatch_core::assets::SystemCode;
use cvewatch_frontend::asset_form::ISMS_SYSTEM_MISMATCH;
use cvewatch_frontend::workspace::systems::delete_blocked;
use cvewatch_frontend::workspace::systems::RECODE_NOTE;
use cvewatch_gui::dialogs::asset_systems::{
    code_label, edit_button, rename_label, BACK_TO_EDIT, CANCEL_EDIT, CONFIRM_EDIT, NEW_CODE_LABEL,
    NEW_NAME_LABEL, SAVE_EDIT,
};
use cvewatch_gui::dialogs::AssetDialog;
use cvewatch_gui::screens::asset_list::{
    group_id, group_open_text, NEW_ASSET_BUTTON, SYSTEMS_BUTTON,
};
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

/// 資產清單預設約佔內容區寬度的三分之二，右側詳情取其餘寬度。
#[test]
fn asset_list_takes_two_thirds_by_default() {
    let (mut h, _f) = gui(grouped_world());
    ctrl(&mut h, egui::Key::Num3);
    let content_left = h.get_by_label(SYSTEMS_BUTTON).rect().left();
    let detail_left = h.get_by_label("請從左側選取資產。").rect().left();
    let right = h.ctx.content_rect().right();
    let ratio = (detail_left - content_left) / (right - content_left);
    assert!(
        (0.6..0.75).contains(&ratio),
        "清單佔內容區寬度的比例為 {ratio}，預設應約為三分之二"
    );
}

/// 展開的組只佔列數所需的高度：下一組的標題緊接在上一組最後一列之後，不留大片空白。
#[test]
fn expanded_group_has_no_trailing_gap() {
    let (mut h, _f) = gui(grouped_world());
    ctrl(&mut h, egui::Key::Num3);
    let last_row = ["FW-EDGE", "FW-CORE"]
        .iter()
        .map(|t| list_cells(&h, t)[0].rect().bottom())
        .fold(f32::MIN, f32::max);
    let next_heading = h.get_by_label("未歸屬資產說明（1）").rect().top();
    let gap = next_heading - last_row;
    assert!(
        (0.0..40.0).contains(&gap),
        "NET 組最後一列與下一組標題相距 {gap}，展開的組不應佔滿剩餘高度"
    );
}

/// 工具列：「新增資產說明」在「新增資產元件」左邊，兩者同一列。
#[test]
fn toolbar_puts_new_system_left_of_new_asset() {
    let (mut h, _f) = gui(grouped_world());
    ctrl(&mut h, egui::Key::Num3);
    let system = h.get_by_label(SYSTEMS_BUTTON).rect();
    let asset = h.get_by_label(NEW_ASSET_BUTTON).rect();
    assert!(system.right() <= asset.left(), "新增資產說明在左");
    assert!((system.center().y - asset.center().y).abs() < 4.0, "同一列");
    assert!(h.query_by_label("新增資產").is_none(), "舊按鈕名稱不再出現");
    assert!(
        h.query_by_label("資產說明…").is_none(),
        "舊按鈕名稱不再出現"
    );
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
    click(&mut h, SYSTEMS_BUTTON);
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
    click(&mut h, SYSTEMS_BUTTON);
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
    click(&mut h, NEW_ASSET_BUTTON);
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

fn code(c: &str) -> SystemCode {
    SystemCode::parse(c).unwrap()
}

fn systems_dialog<'a>(
    h: &'a egui_kittest::Harness<'_, cvewatch_gui::GuiApp>,
) -> &'a cvewatch_gui::dialogs::asset_systems::SystemsDialog {
    match &h.state().asset_dialog {
        Some(AssetDialog::Systems(d)) => d,
        _ => panic!("資產說明對話框應開啟"),
    }
}

/// NET：FW-EDGE 改名為 NET-HW-001（ISMS 編號相同）、FW-CORE（ISMS 編號 NET-HW-002）、
/// PVE-01（沒有 ISMS 編號）。
fn recode_world() -> support::common::matching::World {
    let w = grouped_world();
    assign(&w, "PVE-01", "NET");
    w.conn
        .execute(
            "UPDATE asset SET asset_tag = 'NET-HW-001', isms_id = 'NET-HW-001'
             WHERE asset_tag = 'FW-EDGE'",
            [],
        )
        .unwrap();
    w.conn
        .execute(
            "UPDATE asset SET isms_id = 'NET-HW-002' WHERE asset_tag = 'FW-CORE'",
            [],
        )
        .unwrap();
    w
}

/// 開啟對話框、編輯 `from` 並把代碼改為 `to` 後按「儲存」。
fn edit_code(h: &mut egui_kittest::Harness<'_, cvewatch_gui::GuiApp>, from: &str, to: &str) {
    if !matches!(h.state().asset_dialog, Some(AssetDialog::Systems(_))) {
        click(h, SYSTEMS_BUTTON);
    }
    click(h, &edit_button(&code(from)));
    replace_text(h, &code_label(&code(from)), to);
    click(h, SAVE_EDIT);
}

/// 19 Scenario: Editing a code without assets
#[test]
fn editing_a_code_without_assets() {
    let w = grouped_world();
    create_system(&w.conn, "LAB", "實驗室").unwrap();
    let (mut h, _f) = gui(w);
    ctrl(&mut h, egui::Key::Num3);
    edit_code(&mut h, "LAB", "tst");
    assert!(systems_dialog(&h).confirm.is_none(), "不顯示確認");
    assert!(systems_dialog(&h).editing.is_none(), "編輯列結束");
    assert_eq!(h.state().ws.data.systems.label("TST"), "TST 實驗室");
    assert!(h.state().ws.data.systems.find("LAB").is_none());
    assert!(h.query_by_label(&edit_button(&code("TST"))).is_some());
    assert!(h.query_by_label(&edit_button(&code("LAB"))).is_none());
}

/// 19 Scenario: Code already used in the dialog；只改名稱沿用同一列
#[test]
fn code_already_used_in_the_dialog() {
    let w = grouped_world();
    create_system(&w.conn, "SYS", "核心系統").unwrap();
    let (mut h, _f) = gui(w);
    ctrl(&mut h, egui::Key::Num3);
    edit_code(&mut h, "NET", "SYS");
    assert!(systems_dialog(&h).editing.is_some(), "對話框與編輯列保留");
    assert!(shows(&h, "資產說明代碼 SYS 已被使用"));
    assert_eq!(h.state().ws.data.systems.label("NET"), "NET 網路服務");

    replace_text(&mut h, &code_label(&code("NET")), "NET");
    replace_text(&mut h, &rename_label(&code("NET")), "校園網路服務");
    click(&mut h, SAVE_EDIT);
    assert!(systems_dialog(&h).confirm.is_none(), "只改名稱不顯示確認");
    assert_eq!(h.state().ws.data.systems.label("NET"), "NET 校園網路服務");

    click(&mut h, &edit_button(&code("NET")));
    click(&mut h, CANCEL_EDIT);
    assert!(systems_dialog(&h).editing.is_none());
}

/// 19 Scenario: Confirming a code change / Selected asset follows its new tag
#[test]
fn confirming_a_code_change() {
    let (mut h, _f) = gui(recode_world());
    select_asset(&mut h, "NET-HW-001");
    edit_code(&mut h, "NET", "NWS");
    let plan = systems_dialog(&h).confirm.clone().expect("顯示確認");
    assert_eq!(plan.assets.len(), 3);
    assert!(shows(&h, "將資產說明代碼 NET 改為 NWS？"));
    assert!(shows(&h, "受影響的資產：3 個"));
    assert!(shows(&h, "NET-HW-001 → NWS-HW-001"));
    assert!(shows(&h, "NET-HW-002 → NWS-HW-002"));
    assert!(shows(&h, RECODE_NOTE));
    assert_eq!(
        h.state().ws.data.systems.label("NET"),
        "NET 網路服務",
        "尚未寫入"
    );

    click(&mut h, CONFIRM_EDIT);
    let d = systems_dialog(&h);
    assert!(d.confirm.is_none() && d.editing.is_none());
    assert!(h.query_by_label(&edit_button(&code("NWS"))).is_some());
    assert_eq!(h.state().assets_selected.as_deref(), Some("NWS-HW-001"));
    click(&mut h, "關閉");
    assert!(shows(&h, "ISMS 編號 NWS-HW-001"), "詳情顯示新的 ISMS 編號");
    assert!(h
        .state()
        .status
        .as_ref()
        .is_some_and(|r| r.message.starts_with("已將資產說明代碼 NET 改為 NWS")));
}

/// 19 Scenario: Going back from the confirmation（按鈕與 Esc）
#[test]
fn going_back_from_the_confirmation() {
    let (mut h, _f) = gui(recode_world());
    ctrl(&mut h, egui::Key::Num3);
    edit_code(&mut h, "NET", "NWS");
    assert!(systems_dialog(&h).confirm.is_some());
    click(&mut h, BACK_TO_EDIT);
    let d = systems_dialog(&h);
    assert!(d.confirm.is_none());
    assert_eq!(d.editing.as_ref().map(|r| r.new_code.as_str()), Some("NWS"));
    assert_eq!(
        h.get_by_label(&code_label(&code("NET"))).value().as_deref(),
        Some("NWS"),
        "代碼欄保留輸入"
    );
    assert!(
        h.state().ws.data.systems.find("NWS").is_none(),
        "資料庫不變"
    );

    click(&mut h, SAVE_EDIT);
    assert!(systems_dialog(&h).confirm.is_some());
    h.key_press(egui::Key::Escape);
    h.run();
    let d = systems_dialog(&h);
    assert!(d.confirm.is_none(), "Esc 回到編輯列");
    assert!(d.editing.is_some(), "對話框不關閉");
    assert!(
        h.state().ws.data.asset("NET-HW-001").is_some(),
        "資料庫不變"
    );
}

/// 19 Scenario: Conflicting asset tag in the dialog
#[test]
fn conflicting_asset_tag_in_the_dialog() {
    let w = recode_world();
    w.asset("NWS-HW-001", "internal", &[]);
    let (mut h, _f) = gui(w);
    ctrl(&mut h, egui::Key::Num3);
    edit_code(&mut h, "NET", "NWS");
    let d = systems_dialog(&h);
    assert!(d.confirm.is_none(), "不顯示確認");
    assert!(d.editing.is_some());
    assert!(shows(&h, "資產編號 NWS-HW-001 已被使用"));
    assert_eq!(h.state().ws.data.systems.label("NET"), "NET 網路服務");
}

/// 19 code review：確認後受影響的資產有變動時不寫入，以新計畫重新顯示確認。
#[test]
fn changed_assets_after_confirmation_ask_again() {
    let (mut h, _f) = gui(recode_world());
    ctrl(&mut h, egui::Key::Num3);
    edit_code(&mut h, "NET", "NWS");
    assert_eq!(systems_dialog(&h).confirm.as_ref().unwrap().assets.len(), 3);
    h.state()
        .ws
        .conn
        .execute(
            "INSERT INTO asset (asset_tag, name, category, exposure, first_seen, last_seen,
                                system_code)
             VALUES ('LAB-09', 'LAB-09', 'HW01', 'internal', ?1, ?1, 'NET')",
            [support::common::matching::T0],
        )
        .unwrap();
    click(&mut h, CONFIRM_EDIT);
    let plan = systems_dialog(&h).confirm.clone().expect("重新顯示確認");
    assert_eq!(plan.assets.len(), 4);
    assert!(shows(&h, "受影響的資產：4 個"));
    assert!(shows(&h, PLAN_CHANGED), "狀態列說明未寫入");
    assert!(
        h.state().ws.data.systems.find("NWS").is_none(),
        "資料庫不變"
    );
    click(&mut h, CONFIRM_EDIT);
    assert!(systems_dialog(&h).confirm.is_none());
    assert_eq!(h.state().ws.data.systems.label("NWS"), "NWS 網路服務");
}
