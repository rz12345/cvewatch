//! 05 任務 4.3：Assets／Pending 快照；等待區套用後 Findings 出現 probable 命中、標為不相關。

mod support;

use cvewatch_core::matching::engine::Confidence;
use cvewatch_tui::dialog::Dialog;
use cvewatch_tui::View;
use ratatui::crossterm::event::KeyCode;
use rusqlite::params;
use support::{app, keys, press, render, render_size, seeded_world, LONG_NAME, PENDING_CVE};

#[test]
fn assets_screen_lists_assets_and_unresolved_products() {
    let (mut app, _) = app(seeded_world());
    keys(&mut app, "3");
    let out = render(&app);
    assert!(out.contains("Synology XYZ-300+"), "{out}");
    assert!(out.contains(LONG_NAME));
    insta::assert_snapshot!(out);
}

#[test]
fn long_chinese_name_is_truncated_without_shifting_columns() {
    let (mut app, _) = app(seeded_world());
    keys(&mut app, "3");
    let out = render_size(&app, 72, 14);
    assert!(!out.contains(LONG_NAME));
    assert!(out.contains("資訊所"), "{out}");
    assert!(out.contains('…'));
    insta::assert_snapshot!(out);
}

#[test]
fn asset_detail_shows_components_findings_and_unresolved() {
    let (mut app, _) = app(seeded_world());
    keys(&mut app, "3");
    keys(&mut app, "j"); // 依編號排序：FW-CORE、FW-EDGE、PVE-01
    press(&mut app, KeyCode::Enter);
    let Some(View::AssetDetail { asset_tag, .. }) = app.view() else {
        panic!("應進入資產詳情");
    };
    assert_eq!(asset_tag, "FW-EDGE");
    insta::assert_snapshot!(render(&app));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.view(), Some(View::FindingDetail { .. })));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    assert!(app.stack.is_empty());
}

#[test]
fn pending_screen_snapshot() {
    let (mut app, _) = app(seeded_world());
    keys(&mut app, "4");
    assert_eq!(app.data.pending.len(), 1);
    insta::assert_snapshot!(render(&app));
}

#[test]
fn applying_pending_cve_creates_probable_finding() {
    let (mut app, _) = app(seeded_world());
    let pve_findings = |app: &cvewatch_tui::App| {
        app.data
            .findings
            .iter()
            .filter(|f| f.asset_tag == "PVE-01" && f.cve_id == PENDING_CVE)
            .count()
    };
    assert_eq!(pve_findings(&app), 0);
    keys(&mut app, "4a");
    let Some(Dialog::PendingTarget { options, .. }) = &app.dialog else {
        panic!("應選擇套用目標");
    };
    let pos = options.iter().position(|o| o == "PVE-01").expect("PVE-01");
    for _ in 0..pos {
        keys(&mut app, "j");
    }
    press(&mut app, KeyCode::Enter);
    keys(&mut app, "已確認使用受影響功能");
    insta::assert_snapshot!(render(&app));
    press(&mut app, KeyCode::Enter);
    assert!(app.data.pending.is_empty(), "移出等待區：{:?}", app.message);
    keys(&mut app, "2");
    let row = app
        .findings
        .visible
        .iter()
        .map(|i| &app.data.findings[*i])
        .find(|f| f.asset_tag == "PVE-01" && f.cve_id == PENDING_CVE)
        .expect("Findings 出現 PVE-01 的命中");
    assert_eq!(row.confidence, Confidence::Probable);
    assert!(row.manual);
}

#[test]
fn dismissing_pending_cve_records_manual_resolution() {
    let (mut app, _) = app(seeded_world());
    keys(&mut app, "4d");
    keys(&mut app, "不使用此功能");
    press(&mut app, KeyCode::Enter);
    assert!(app.data.pending.is_empty());
    let reason: String = app
        .conn
        .query_row(
            "SELECT resolved_reason FROM pending WHERE cve_id = ?1",
            params![PENDING_CVE],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reason, "manual_dismiss");
    assert!(app.message.as_deref().unwrap().contains("不相關"));
}

#[test]
fn resolve_error_is_shown_in_status_line() {
    let (mut app, _) = app(seeded_world());
    keys(&mut app, "4d");
    // 對話框開啟期間，另一個程序已處理此 CVE
    app.conn
        .execute(
            "UPDATE pending SET resolved_at = '2026-11-07T00:00:00+00:00', resolved_reason = 'manual_dismiss'
             WHERE cve_id = ?1",
            params![PENDING_CVE],
        )
        .unwrap();
    press(&mut app, KeyCode::Enter);
    let msg = app.message.as_deref().unwrap();
    assert!(
        msg.starts_with("等待區處理失敗") && msg.contains("不在等待區"),
        "{msg}"
    );
    assert!(app.dialog.is_none());
}

#[test]
fn apply_targets_skip_assets_without_products() {
    let w = seeded_world();
    w.asset("HOST-01", "internal", &[]);
    w.package(
        "HOST-01",
        "pkg:deb/debian/openssl",
        "3.0.11-1",
        "2026-11-01T00:00:00+00:00",
    );
    let (mut app, _) = app(w);
    keys(&mut app, "4a");
    let Some(Dialog::PendingTarget { options, .. }) = &app.dialog else {
        panic!("應選擇套用目標");
    };
    assert_eq!(options, &["FW-CORE", "FW-EDGE", "PVE-01"]);
}
