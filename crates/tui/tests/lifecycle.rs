//! 22 任務 10.1–10.2：資產表格「週期」欄與 `l` 生命週期篩選、資產詳情的生命週期段落、
//! Dashboard 的生命週期計數（spec tui-console: Lifecycle in the console）。

mod support;

use cvewatch_tui::glyphs;
use cvewatch_tui::View;
use ratatui::crossterm::event::KeyCode;
use rusqlite::params;
use support::common::matching::World;
use support::{app, app_at, empty_world, keys, press, render};

/// 依編號排序後的列（含資產編號的那一行）。
fn row<'a>(out: &'a str, tag: &str) -> Option<&'a str> {
    out.lines().find(|l| l.contains(tag))
}

fn set(w: &World, sql: &str, tag: &str) {
    w.conn
        .execute(&format!("{sql} WHERE asset_tag = ?1"), params![tag])
        .expect("update asset");
}

/// 今天 2026-11-07（測試時鐘）、提醒 90 天：
/// FW-EDGE 硬體已 EoL（覆寫）、SW-CORE 的 PAN-OS 元件已 EoL（覆寫）、SRV-01 保固即將到期、
/// NAS-01 已退役且硬體已 EoL（不列入）。
fn eol_world() -> World {
    let w = empty_world();
    w.asset("FW-EDGE", "internet", &[]);
    w.asset("SRV-01", "internal", &[]);
    w.asset("SW-CORE", "internal", &[("paloalto-panos", "10.2.9")]);
    w.asset("NAS-01", "internal", &[]);
    set(
        &w,
        "UPDATE asset SET hw_eol_override = '2026-01-01', hw_eol_source = '原廠公告'",
        "FW-EDGE",
    );
    set(&w, "UPDATE asset SET warranty_end = '2026-12-01'", "SRV-01");
    w.conn
        .execute(
            "UPDATE component SET eol_override = '2025-06-30', eol_override_source = '原廠公告'
             WHERE asset_id = (SELECT id FROM asset WHERE asset_tag = 'SW-CORE')",
            [],
        )
        .expect("component override");
    set(
        &w,
        "UPDATE asset SET hw_eol_override = '2026-01-01', hw_eol_source = '原廠公告',
         status = 'retired', retired_at = '2026-10-01T00:00:00+00:00'",
        "NAS-01",
    );
    w
}

#[test]
fn lifecycle_column_shows_most_severe_state() {
    let (mut app, _) = app(eol_world());
    keys(&mut app, "3");
    let out = render(&app);
    let warn = glyphs::UNICODE.warn;
    assert!(out.contains("週期"), "{out}");
    let fw = row(&out, "FW-EDGE").expect("FW-EDGE 列");
    assert!(fw.contains(&format!("{warn} 已 EoL")), "{fw}");
    let srv = row(&out, "SRV-01").expect("SRV-01 列");
    assert!(srv.contains("即將到期"), "{srv}");
    assert!(!srv.contains(warn), "即將到期不加警示字元：{srv}");
    assert!(row(&out, "NAS-01").is_none(), "已退役資產不列出：\n{out}");
}

/// Scenario: Filtering EoL assets in the console。
#[test]
fn filtering_eol_assets_in_the_console() {
    let (mut app, _) = app(eol_world());
    keys(&mut app, "3");
    assert!(render(&app).contains("cvewatch ─ Assets"));
    keys(&mut app, "l");
    let out = render(&app);
    assert!(out.contains("資產（生命週期：已 EoL）"), "{out}");
    assert!(row(&out, "FW-EDGE").is_some(), "{out}");
    assert!(row(&out, "SW-CORE").is_some(), "{out}");
    assert!(row(&out, "SRV-01").is_none(), "{out}");
    assert!(
        row(&out, "NAS-01").is_none(),
        "已退役資產不符合篩選：\n{out}"
    );
    assert_eq!(app.visible_assets().len(), 2);
    insta::assert_snapshot!(out);

    // 篩選後第二列為 SW-CORE（未篩選時第二列是 SRV-01）：Enter 開啟篩選清單中的資產
    keys(&mut app, "j");
    press(&mut app, KeyCode::Enter);
    let Some(View::AssetDetail { asset_tag, .. }) = app.view() else {
        panic!("應進入資產詳情");
    };
    assert_eq!(asset_tag, "SW-CORE");
}

#[test]
fn l_cycles_eol_soon_expiry_and_back_to_all() {
    let (mut app, _) = app(eol_world());
    keys(&mut app, "3ll");
    let out = render(&app);
    assert!(out.contains("資產（生命週期：即將 EoL）"), "{out}");
    assert!(
        out.contains("沒有符合生命週期篩選「即將 EoL」的資產"),
        "{out}"
    );
    keys(&mut app, "l");
    let out = render(&app);
    assert!(out.contains("資產（生命週期：保固或合約到期）"), "{out}");
    assert!(row(&out, "SRV-01").is_some(), "{out}");
    assert!(row(&out, "FW-EDGE").is_none(), "{out}");
    keys(&mut app, "l");
    let out = render(&app);
    assert!(out.contains("cvewatch ─ Assets"), "{out}");
    assert_eq!(app.visible_assets().len(), 3);
}

/// Scenario: Lifecycle in the asset detail（今天 2027-01-10）。
#[test]
fn lifecycle_in_the_asset_detail() {
    let w = empty_world();
    w.asset("SRV-01", "internal", &[("paloalto-panos", "10.2.9")]);
    set(&w, "UPDATE asset SET warranty_end = '2026-12-31'", "SRV-01");
    w.conn
        .execute(
            "UPDATE component SET eol_override = '2027-03-01', eol_override_source = '延長支援合約'",
            [],
        )
        .expect("component override");
    let (mut app, _) = app_at(w, "2027-01-10T00:00:00+00:00");
    keys(&mut app, "3");
    press(&mut app, KeyCode::Enter);
    let out = render(&app);
    assert!(out.contains("生命週期（提醒 90 天）"), "{out}");
    assert!(
        out.contains("保固到期 2026-12-31 已到期（10 天前）"),
        "{out}"
    );
    assert!(out.contains("硬體 EoL 未填廠牌或型號"), "{out}");
    assert!(out.contains("維護合約到期 未填"), "{out}");
    assert!(
        out.contains("10.2.9 EoL 2027-03-01 即將 EoL（剩 50 天），來源 延長支援合約（手動覆寫）"),
        "{out}"
    );
    insta::assert_snapshot!(out);
}

/// Scenario: Lifecycle counts on the dashboard（今天 2026-11-07、提醒 90 天）。
#[test]
fn lifecycle_counts_on_the_dashboard() {
    let w = empty_world();
    for tag in ["FW-EDGE", "SRV-01", "SW-CORE", "NAS-01"] {
        w.asset(tag, "internal", &[]);
    }
    set(
        &w,
        "UPDATE asset SET hw_eol_override = '2027-01-15', hw_eol_source = '原廠公告'",
        "FW-EDGE",
    );
    set(&w, "UPDATE asset SET warranty_end = '2026-12-31'", "SRV-01");
    set(&w, "UPDATE asset SET support_end = '2026-10-01'", "SW-CORE");
    set(&w, "UPDATE asset SET license_end = '2027-01-31'", "NAS-01");
    let (app, _) = app(w);
    let out = render(&app);
    assert!(
        out.contains("即將 EoL 1、保固或合約到期 3（提醒 90 天）"),
        "{out}"
    );
    assert!(!out.contains("已 EoL"), "已 EoL 為 0 時省略：\n{out}");
}

#[test]
fn help_mentions_lifecycle_filter_key() {
    let (mut app, _) = app(empty_world());
    keys(&mut app, "?");
    let out = render(&app);
    assert!(out.contains("資產：切換生命週期篩選"), "{out}");
}

#[test]
fn dashboard_hides_lifecycle_line_when_all_zero() {
    let w = empty_world();
    w.asset("SRV-01", "internal", &[]);
    // 超過提醒天數：不算即將到期
    set(&w, "UPDATE asset SET warranty_end = '2028-01-01'", "SRV-01");
    let (app, _) = app(w);
    assert!(app.data.lifecycle_counts.is_empty());
    let out = render(&app);
    assert!(!out.contains("提醒"), "{out}");
    assert!(!out.contains("保固或合約到期"), "{out}");
}
