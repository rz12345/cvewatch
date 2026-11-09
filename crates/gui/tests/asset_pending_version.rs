//! 18 任務 6.3：資產清單的版本待補標示、詳情「版本待補（未納入比對）」、「只顯示版本待補」篩選，
//! 以及 Dashboard 的版本待補數字切換到已篩選的資產畫面。

mod support;

use cvewatch_gui::screens::asset_list::{PENDING_FILTER, PENDING_MARK};
use cvewatch_gui::{GuiApp, Screen};
use eframe::egui;
use egui_kittest::Harness;
use support::*;

/// seeded_world 加上 NET-HW-003（DSM、iDRAC 版本待補）與 NET-HW-004（PAN-OS 版本待補）。
fn pending_gui() -> (Harness<'static, GuiApp>, Fakes) {
    let w = seeded_world();
    w.asset(
        "NET-HW-003",
        "internal",
        &[("synology-dsm", ""), ("dell-idrac9", "")],
    );
    w.asset("NET-HW-004", "internal", &[("paloalto-panos", "")]);
    gui(w)
}

#[test]
fn pending_component_in_the_detail_pane() {
    let (mut h, _f) = pending_gui();
    select_asset(&mut h, "NET-HW-003");
    assert!(listed(&h, PENDING_MARK), "清單有版本待補標示");
    assert!(
        shows(&h, "DiskStation Manager 版本待補（未納入比對）"),
        "詳情顯示版本待補"
    );
}

#[test]
fn jumping_to_pending_versions() {
    let (mut h, _f) = pending_gui();
    ctrl(&mut h, egui::Key::Num1);
    let label = "版本待補 3 個元件（2 個資產），未納入比對";
    assert!(shows(&h, label), "Dashboard 顯示待補數");
    click(&mut h, label);
    assert_eq!(h.state().screen, Screen::Assets);
    assert!(h.state().assets_pending_only);
    assert!(listed(&h, "NET-HW-003") && listed(&h, "NET-HW-004"));
    assert!(!listed(&h, "PVE-01"), "只列出有版本待補元件的資產");
    // 取消篩選後回到全部資產
    click(&mut h, PENDING_FILTER);
    assert!(listed(&h, "PVE-01"));
}
