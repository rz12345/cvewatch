//! 資產、等待區與同步紀錄畫面。

mod support;

use cvewatch_core::matching::engine::Confidence;
use eframe::egui;
use egui_kittest::kittest::Queryable;
use support::*;

/// 點選資產清單中的資產（清單項目以「資產編號  名稱」開頭）。
fn pick_asset(h: &mut egui_kittest::Harness<'_, cvewatch_gui::GuiApp>, tag: &str) {
    h.get_by(|n| {
        n.label()
            .is_some_and(|l| l.starts_with(&format!("{tag}  ")))
    })
    .click();
    h.run();
}

#[test]
fn selecting_asset_shows_components_findings_and_unresolved() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num3);
    pick_asset(&mut h, "PVE-01");
    assert_eq!(h.state().assets_selected.as_deref(), Some("PVE-01"));
    assert!(shows(&h, "元件（1）"));
    assert!(shows(&h, "proxmox-ve 8.2.2"));
    assert!(shows(&h, "未處理命中 0 筆｜全部命中 0 筆"));
    assert!(shows(&h, "Synology DS920+ 7.2"), "未辨識產品");
    pick_asset(&mut h, "PA-460");
    assert!(shows(&h, "未處理命中 4 筆｜全部命中 4 筆"));
    assert!(shows(&h, "paloalto-panos 11.1.2"));
    pick_asset(&mut h, "FW-CORE");
    assert!(shows(&h, LONG_NAME), "中文資產名稱");
}

#[test]
fn applying_pending_cve_to_asset_creates_probable_finding() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num4);
    assert!(shows(&h, "僅關鍵字相符"));
    click(&mut h, PENDING_CVE);
    click(&mut h, "套用到資產");
    click(&mut h, "PVE-01");
    h.get_by_label("備註（寫入時間軸）")
        .type_text("確認 PVE-01 受影響");
    h.run();
    click(&mut h, "確認");
    let app = h.state();
    assert!(app.ws.data.pending.iter().all(|p| p.cve_id != PENDING_CVE));
    let f = app
        .ws
        .data
        .findings
        .iter()
        .find(|f| f.cve_id == PENDING_CVE)
        .expect("probable finding");
    assert_eq!(f.asset_tag, "PVE-01");
    assert_eq!(f.confidence, Confidence::Probable);
    assert!(shows(&h, "已移出等待區並套用到 PVE-01"));
    // 命中畫面出現該筆 probable 命中（標記 ?）
    ctrl(&mut h, egui::Key::Num2);
    assert!(h.query_all_by_label(PENDING_CVE).next().is_some());
    assert!(h.query_all_by_label("?").next().is_some());
}

#[test]
fn dismissing_pending_cve_removes_it() {
    let (mut h, f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num4);
    click(&mut h, PENDING_CVE);
    click(&mut h, "複製 CVE ID");
    assert_eq!(f.rec.copied.borrow().as_slice(), [PENDING_CVE]);
    click(&mut h, "標為不相關");
    h.get_by_label("備註（寫入時間軸）")
        .type_text("不是我們的版本");
    h.run();
    click(&mut h, "確認");
    assert!(h.state().ws.data.pending.is_empty());
    assert!(shows(&h, "等待區沒有 CVE"));
    assert!(shows(&h, "已移出等待區（人工判定不相關）"));
}

#[test]
fn pending_links_open_only_https() {
    let (mut h, f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num4);
    click(&mut h, PENDING_CVE);
    click(&mut h, "CVE.org");
    assert_eq!(
        f.rec.opened.borrow().as_slice(),
        [format!("https://www.cve.org/CVERecord?id={PENDING_CVE}")]
    );
}

#[test]
fn sync_log_is_newest_first_with_error_summary() {
    let (mut h, _f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num5);
    let y = |label: &str| h.get_by_label(label).rect().top();
    assert!(y("osv") < y("cvelist"), "新到舊");
    assert!(shows(&h, "HTTP 503"));
    assert!(shows(&h, "失敗") && shows(&h, "成功"));
}
