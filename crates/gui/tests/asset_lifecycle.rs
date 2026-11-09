//! 22 任務 9.1～9.4：資產清單的生命週期標示與篩選、詳情的「生命週期」區塊（來源網址經 Opener
//! 開啟）、資產對話框的生命週期欄位與元件 EoL 覆寫，以及 Dashboard 的生命週期數字
//! （gui-console 的 GUI lifecycle markers and filter／in asset detail／fields input／dashboard
//! lifecycle reminder）。
//!
//! 測試時鐘為 2026-11-07（UTC）；日期依此安排狀態。硬體型號只用虛構的 Aruba XYZ-100。

mod support;

use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::lifecycle::{LifecycleFilter, LifecycleFlags};
use cvewatch_frontend::asset_form::{DATE_INVALID, OVERRIDE_SOURCE_REQUIRED};
use cvewatch_gui::dialogs::asset_edit::AssetEdit;
use cvewatch_gui::dialogs::asset_lifecycle::{
    COMPONENT_EOL_DATE, COMPONENT_EOL_SOURCE, EOL_APPLY, EOL_CLEAR, EOL_OVERRIDE_BUTTON,
};
use cvewatch_gui::dialogs::AssetDialog;
use cvewatch_gui::screens::asset_list::{
    lifecycle_marks, EOL_MARK, EOL_SOON_MARK, EXPIRED_MARK, EXPIRING_MARK, LIFECYCLE_FILTER_LABEL,
};
use cvewatch_gui::screens::dashboard::{lifecycle_count_text, reminder_text};
use cvewatch_gui::screens::lifecycle_view::lifecycle_heading;
use cvewatch_gui::{GuiApp, Screen};
use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use support::common::matching::{at, world_with, World};
use support::*;

/// 虛構的硬體型號清單（目錄來源）。
const HARDWARE: &str = r#"
[[hardware]]
vendor = "Aruba"
model = "XYZ-100"
eol = "2026-10-31"
source = "https://vendor.example/eol/xyz-100"
"#;

const HARDWARE_URL: &str = "https://vendor.example/eol/xyz-100";

/// FW-EDGE（PAN-OS 11.1.2，分支 11.1 EoL 2027-03-01）、SRV-01（ESXi 8.0，維護合約 20 天後到期）、
/// SW-CORE（Aruba XYZ-100，硬體已 EoL）、PVE-01（Proxmox VE 6.4，無法對應分支）、
/// NAS-01（硬體 EoL 覆寫，已 EoL）與已退役的 NAS-02（硬體 EoL 覆寫、維護合約即將到期）。
fn lifecycle_world() -> World {
    let w = world_with(Some(HARDWARE));
    w.asset("FW-EDGE", "internet", &[("paloalto-panos", "11.1.2")]);
    w.asset("SRV-01", "internal", &[("vmware-esxi", "8.0")]);
    w.asset("SW-CORE", "internal", &[("hpe-aoscx", "10.13.1000")]);
    w.asset("PVE-01", "internal", &[("proxmox-ve", "6.4")]);
    w.asset("NAS-01", "internal", &[("truenas", "13.0")]);
    w.asset("NAS-02", "internal", &[("truenas", "13.0")]);
    w.conn
        .execute_batch(
            "UPDATE asset SET name = 'edge-fw' WHERE asset_tag = 'FW-EDGE';
             UPDATE asset SET name = 'srv01', support_end = '2026-11-27' WHERE asset_tag = 'SRV-01';
             UPDATE asset SET name = 'core-sw', brand = 'Aruba', model = 'XYZ-100'
                 WHERE asset_tag = 'SW-CORE';
             UPDATE asset SET name = 'pve01' WHERE asset_tag = 'PVE-01';
             UPDATE asset SET name = 'nas01', hw_eol_override = '2026-01-31',
                 hw_eol_source = '原廠公告' WHERE asset_tag = 'NAS-01';
             UPDATE asset SET name = 'nas02', hw_eol_override = '2026-01-31',
                 hw_eol_source = '原廠公告', support_end = '2026-11-20' WHERE asset_tag = 'NAS-02';
             INSERT INTO eol_product (slug, status, fetched_at, html_link)
                 VALUES ('panos', 'ok', '2026-11-06T00:00:00+00:00', 'https://endoflife.date/panos'),
                        ('proxmox-ve', 'ok', '2026-11-06T00:00:00+00:00',
                         'https://endoflife.date/proxmox-ve');
             INSERT INTO eol_release (slug, name, is_eol, eol_from, latest_name, position)
                 VALUES ('panos', '11.1', 0, '2027-03-01', '11.1.6', 0),
                        ('proxmox-ve', '8', 0, '2026-12-31', '8.4.1', 0),
                        ('proxmox-ve', '7', 1, '2024-07-31', '7.4.19', 1);",
        )
        .expect("lifecycle data");
    w.retire("NAS-02");
    w
}

fn lifecycle_gui() -> (Harness<'static, GuiApp>, Fakes) {
    gui(lifecycle_world())
}

fn dialog<'a>(h: &'a Harness<'_, GuiApp>) -> &'a AssetEdit {
    match &h.state().asset_dialog {
        Some(AssetDialog::Edit(e)) => e,
        _ => panic!("編輯對話框未開啟"),
    }
}

/// 取代文字欄位的內容：同名的詳情項目（例如「保固到期」）是 Label，只取輸入框。
fn replace_field(h: &mut Harness<'_, GuiApp>, label: &str, text: &str) {
    let role = egui::accesskit::Role::TextInput;
    h.get_by_role_and_label(role, label).focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_role_and_label(role, label).type_text(text);
    h.run();
}

fn asset_text(h: &Harness<'_, GuiApp>, tag: &str, column: &str) -> Option<String> {
    h.state()
        .ws
        .conn
        .query_row(
            &format!("SELECT {column} FROM asset WHERE asset_tag = ?1"),
            [tag],
            |r| r.get(0),
        )
        .expect("asset column")
}

fn component_override(h: &Harness<'_, GuiApp>, tag: &str) -> (Option<String>, Option<String>) {
    h.state()
        .ws
        .conn
        .query_row(
            "SELECT c.eol_override, c.eol_override_source FROM component c
             JOIN asset a ON a.id = c.asset_id WHERE a.asset_tag = ?1",
            [tag],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("component override")
}

/// 選擇資產畫面的生命週期篩選。
fn choose_lifecycle(h: &mut Harness<'_, GuiApp>, option: &str) {
    h.get_by_label(LIFECYCLE_FILTER_LABEL).click();
    h.run();
    click_option(h, option);
}

// ---- 9.1 清單標示與篩選 ----

#[test]
fn marks_follow_the_design_order_and_alert_only_reached_states() {
    let all = LifecycleFlags {
        eol: true,
        eol_soon: true,
        expired: true,
        expiring: true,
    };
    assert_eq!(
        lifecycle_marks(all),
        [
            (EOL_MARK, true),
            (EOL_SOON_MARK, false),
            (EXPIRED_MARK, true),
            (EXPIRING_MARK, false)
        ]
    );
    assert!(lifecycle_marks(LifecycleFlags::default()).is_empty());
}

/// spec Marker in the list：今天為 2027-01-10、FW-EDGE 的 PAN-OS EoL 為 2027-03-01。
/// 也驗證切換畫面時跨日重算（design D11）。
#[test]
fn marker_in_the_list() {
    let (mut h, _f) = lifecycle_gui();
    ctrl(&mut h, egui::Key::Num3);
    assert!(
        listed(&h, "edge-fw"),
        "2026-11-07 時 EoL 還有 114 天，不標示"
    );
    assert!(listed(&h, &format!("core-sw{EOL_MARK}")), "硬體已 EoL");
    assert!(
        listed(&h, &format!("srv01{EXPIRING_MARK}")),
        "維護合約即將到期"
    );
    h.state_mut().ws.services.clock = Box::new(|| at("2027-01-10T00:00:00+00:00"));
    ctrl(&mut h, egui::Key::Num1);
    ctrl(&mut h, egui::Key::Num3);
    assert!(
        listed(&h, &format!("edge-fw{EOL_SOON_MARK}")),
        "跨日後重算：剩 50 天"
    );
    assert!(
        listed(&h, &format!("srv01{EXPIRED_MARK}")),
        "維護合約已到期"
    );
}

/// spec Filtering expiring contracts：SRV-01 的維護合約 30 天內到期、FW-EDGE 沒有到期項目。
#[test]
fn filtering_expiring_contracts() {
    let (mut h, _f) = lifecycle_gui();
    ctrl(&mut h, egui::Key::Num3);
    choose_lifecycle(&mut h, LifecycleFilter::Expiry.label());
    assert_eq!(h.state().assets_lifecycle, Some(LifecycleFilter::Expiry));
    assert!(listed(&h, "SRV-01"));
    for tag in ["FW-EDGE", "SW-CORE", "PVE-01", "NAS-01"] {
        assert!(!listed(&h, tag), "{tag} 沒有到期項目");
    }
    // 與「顯示已退役」同時使用：退役資產不符合生命週期篩選
    click(&mut h, "顯示已退役");
    assert!(listed(&h, "SRV-01"));
    assert!(!listed(&h, "NAS-02"), "退役資產不列入");
    choose_lifecycle(&mut h, "全部");
    assert!(listed(&h, "FW-EDGE") && listed(&h, "NAS-02"));
}

// ---- 9.2 詳情 ----

/// spec Opening the source of an EoL date：SW-CORE 的硬體 EoL 來源網址以預設瀏覽器開啟。
#[test]
fn opening_the_source_of_an_eol_date() {
    let (mut h, f) = lifecycle_gui();
    select_asset(&mut h, "SW-CORE");
    assert!(shows(&h, &lifecycle_heading(90)));
    assert!(shows(&h, "硬體 EoL（Aruba XYZ-100）"));
    assert!(shows(&h, "2026-10-31"));
    assert!(shows(&h, "已 EoL（7 天前）"));
    click(&mut h, HARDWARE_URL);
    assert_eq!(f.rec.opened.borrow().as_slice(), [HARDWARE_URL]);
    assert!(status(&h).contains(HARDWARE_URL), "{}", status(&h));
}

/// spec Unmapped version in the detail pane：PVE-01 的 Proxmox VE 6.4 無法對應版本分支。
#[test]
fn unmapped_version_in_the_detail_pane() {
    let (mut h, _f) = lifecycle_gui();
    select_asset(&mut h, "PVE-01");
    assert!(shows(&h, "Proxmox VE 6.4"), "項目含元件版本");
    assert!(shows(&h, "無法對應版本分支（6.4）"));
    assert!(
        shows(&h, "保固到期") && shows(&h, "未填"),
        "沒有資料的項目顯示原因"
    );
}

#[test]
fn endoflife_date_source_opens_the_product_page() {
    let (mut h, f) = lifecycle_gui();
    select_asset(&mut h, "FW-EDGE");
    assert!(shows(&h, "PAN-OS 11.1.2（分支 11.1）"));
    assert!(shows(&h, "剩 114 天"));
    click(&mut h, "endoflife.date");
    assert_eq!(
        f.rec.opened.borrow().as_slice(),
        ["https://endoflife.date/panos"]
    );
}

// ---- 9.3 對話框 ----

/// spec Entering a warranty date：保固到期填 2027/12/31 並儲存。
#[test]
fn entering_a_warranty_date() {
    let (mut h, _f) = lifecycle_gui();
    select_asset(&mut h, "SRV-01");
    click(&mut h, "編輯");
    replace_field(&mut h, "保固到期", "2027/12/31");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert_eq!(
        asset_text(&h, "SRV-01", "warranty_end").as_deref(),
        Some("2027-12-31")
    );
    assert!(shows(&h, "2027-12-31"), "詳情顯示保固到期日");
}

/// spec Invalid date in the dialog：維護合約到期填 2027-13-01。
#[test]
fn invalid_date_in_the_dialog() {
    let (mut h, _f) = lifecycle_gui();
    select_asset(&mut h, "SRV-01");
    click(&mut h, "編輯");
    replace_field(&mut h, "維護合約到期", "2027-13-01");
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_some(), "有錯誤時不儲存");
    assert_eq!(
        dialog(&h).errors.field(StandardField::SupportEnd),
        Some(DATE_INVALID)
    );
    assert!(shows(&h, DATE_INVALID), "錯誤顯示在欄位旁");
    assert_eq!(
        asset_text(&h, "SRV-01", "support_end").as_deref(),
        Some("2026-11-27"),
        "資產不變"
    );
}

#[test]
fn hardware_override_without_source_is_marked() {
    let (mut h, _f) = lifecycle_gui();
    select_asset(&mut h, "SRV-01");
    click(&mut h, "編輯");
    replace_field(&mut h, "硬體 EoL 覆寫", "2027-06-30");
    click(&mut h, "儲存");
    assert_eq!(
        dialog(&h).errors.field(StandardField::Source),
        Some(OVERRIDE_SOURCE_REQUIRED)
    );
    assert_eq!(asset_text(&h, "SRV-01", "hw_eol_override"), None);
}

/// spec Component override in the dialog：ESXi 覆寫 2028-12-31、來源「延長支援合約」。
#[test]
fn component_override_in_the_dialog() {
    let (mut h, _f) = lifecycle_gui();
    select_asset(&mut h, "SRV-01");
    click(&mut h, "編輯");
    click(&mut h, EOL_OVERRIDE_BUTTON);
    replace_field(&mut h, COMPONENT_EOL_DATE, "2028-12-31");
    click(&mut h, EOL_APPLY);
    assert!(
        shows(&h, OVERRIDE_SOURCE_REQUIRED),
        "缺來源時標在欄位下方、不關閉"
    );
    assert!(dialog(&h).eol_dialog.is_some());
    replace_field(&mut h, COMPONENT_EOL_SOURCE, "延長支援合約");
    click(&mut h, EOL_APPLY);
    assert!(dialog(&h).eol_dialog.is_none());
    assert!(
        shows(&h, "EoL 2028-12-31（延長支援合約）"),
        "元件列顯示覆寫"
    );
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert_eq!(
        component_override(&h, "SRV-01"),
        (Some("2028-12-31".into()), Some("延長支援合約".into()))
    );
    assert!(shows(&h, "ESXi 8.0"));
    assert!(shows(&h, "2028-12-31"));
    assert!(shows(&h, "延長支援合約（手動覆寫）"));
}

#[test]
fn clearing_a_component_override() {
    let w = lifecycle_world();
    w.conn
        .execute_batch(
            "UPDATE component SET eol_override = '2028-12-31', eol_override_source = '延長支援合約'
             WHERE asset_id = (SELECT id FROM asset WHERE asset_tag = 'SRV-01')",
        )
        .unwrap();
    let (mut h, _f) = gui(w);
    select_asset(&mut h, "SRV-01");
    click(&mut h, "編輯");
    click(&mut h, EOL_OVERRIDE_BUTTON);
    click(&mut h, EOL_CLEAR);
    click(&mut h, "儲存");
    assert!(h.state().asset_dialog.is_none(), "{}", status(&h));
    assert_eq!(component_override(&h, "SRV-01"), (None, None));
}

#[test]
fn escape_closes_only_the_override_dialog() {
    let (mut h, _f) = lifecycle_gui();
    select_asset(&mut h, "SRV-01");
    click(&mut h, "編輯");
    click(&mut h, EOL_OVERRIDE_BUTTON);
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(dialog(&h).eol_dialog.is_none(), "小對話框關閉");
    assert!(h.state().asset_dialog.is_some(), "資產對話框仍開啟");
}

// ---- 9.4 Dashboard ----

/// spec Jumping to EoL assets：2 個使用中資產已 EoL（退役的 NAS-02 不計）。
#[test]
fn jumping_to_eol_assets() {
    let (mut h, _f) = lifecycle_gui();
    h.state_mut().assets_pending_only = true;
    h.state_mut().assets_show_retired = true;
    h.run();
    let eol = lifecycle_count_text(LifecycleFilter::Eol, 2);
    assert!(shows(&h, &eol), "Dashboard 顯示已 EoL 數");
    assert!(shows(
        &h,
        &lifecycle_count_text(LifecycleFilter::EolSoon, 0)
    ));
    assert!(shows(&h, &lifecycle_count_text(LifecycleFilter::Expiry, 1)));
    assert!(shows(&h, &reminder_text(90)));
    click(&mut h, &eol);
    assert_eq!(h.state().screen, Screen::Assets);
    assert_eq!(h.state().assets_lifecycle, Some(LifecycleFilter::Eol));
    assert!(!h.state().assets_pending_only && !h.state().assets_show_retired);
    assert_eq!(
        h.get_by_label(LIFECYCLE_FILTER_LABEL).value().as_deref(),
        Some(LifecycleFilter::Eol.label())
    );
    assert!(listed(&h, "SW-CORE") && listed(&h, "NAS-01"));
    for tag in ["FW-EDGE", "SRV-01", "PVE-01", "NAS-02"] {
        assert!(!listed(&h, tag), "{tag} 不是使用中的已 EoL 資產");
    }
}

#[test]
fn no_lifecycle_row_when_all_counts_are_zero() {
    let (h, _f) = gui(seeded_world());
    assert!(h.state().ws.data.lifecycle_counts.is_empty());
    assert!(!shows(&h, &reminder_text(90)), "三者都為 0 時不顯示");
    assert!(!shows(&h, &lifecycle_count_text(LifecycleFilter::Eol, 0)));
}
