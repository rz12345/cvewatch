//! 18 任務 7.1：Dashboard 的版本待補計數，資產畫面的版本待補標示與元件版本「版本待補」
//! （寬字元對齊不跑版）。

mod support;

use cvewatch_tui::glyphs;
use cvewatch_tui::View;
use ratatui::crossterm::event::KeyCode;
use support::{app, app_with_glyphs, keys, press, render, render_size, seeded_world};

/// seeded_world 加上 NET-HW-003（DSM、iDRAC 版本待補）與 NET-HW-004（PAN-OS 版本待補）。
macro_rules! pending_world {
    () => {{
        let w = seeded_world();
        w.asset(
            "NET-HW-003",
            "internal",
            &[("synology-dsm", ""), ("dell-idrac9", "")],
        );
        w.asset("NET-HW-004", "internal", &[("paloalto-panos", "")]);
        w
    }};
}

#[test]
fn pending_versions_on_the_dashboard() {
    let (app, _) = app(pending_world!());
    let out = render(&app);
    assert!(
        out.contains("版本待補 3 個元件（2 個資產），未納入比對"),
        "{out}"
    );
}

#[test]
fn pending_component_in_the_assets_screen() {
    let (mut app, _) = app_with_glyphs(pending_world!(), glyphs::UNICODE);
    keys(&mut app, "3");
    let out = render_size(&app, 100, 16);
    let row = out
        .lines()
        .find(|l| l.contains("NET-HW-003"))
        .expect("NET-HW-003 列");
    assert!(row.contains(glyphs::UNICODE.warn), "版本待補標示：{row}");
    let plain = out
        .lines()
        .find(|l| l.contains("FW-EDGE"))
        .expect("FW-EDGE 列");
    assert!(!plain.contains(glyphs::UNICODE.warn), "{plain}");
    // 寬字元與警示字元不讓欄位錯位（快照中各列的右框線在同一欄）
    insta::assert_snapshot!(out);

    // 依編號排序：FW-CORE、FW-EDGE、NET-HW-003
    keys(&mut app, "jj");
    press(&mut app, KeyCode::Enter);
    let Some(View::AssetDetail { asset_tag, .. }) = app.view() else {
        panic!("應進入資產詳情");
    };
    assert_eq!(asset_tag, "NET-HW-003");
    let detail = render(&app);
    let dsm = detail
        .lines()
        .find(|l| l.contains("synology-dsm"))
        .expect("DSM 元件");
    assert!(dsm.contains("版本待補"), "{dsm}");
}
