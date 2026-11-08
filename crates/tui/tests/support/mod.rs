//! TUI 測試共用：記憶體資料庫（沿用 core 的比對測試建構器）、外部效果替身、TestBackend 繪製與按鍵。
#![allow(dead_code)]

#[path = "../../../core/tests/common/mod.rs"]
pub mod common;
#[path = "../../../frontend/tests/shared/mod.rs"]
pub mod shared;

use common::matching::{at, world_with, World};
#[allow(unused_imports)]
pub use cvewatch_frontend::test_support::{
    self, named, FakeBackend, InlineSpawner, Recorder, SharedStore,
};
use cvewatch_tui::data::Settings;
use cvewatch_tui::services::Services;
use cvewatch_tui::{glyphs, ui, App, Context};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use rusqlite::params;
use shared::{keyword_only_pending, panos_cves};
#[allow(unused_imports)]
pub use shared::{PANOS_CVES, PENDING_CVE, PROXMOX_CNA};

/// 測試時鐘。
pub const NOW: &str = "2026-11-07T00:00:00+00:00";
pub const WIDTH: u16 = 110;
pub const HEIGHT: u16 = 26;

pub const KEV_CVE: &str = "CVE-2026-0001";
pub const LONG_NAME: &str = "資訊所機房核心防火牆主機";

/// 外部效果替身（與 GUI 測試共用，見 `cvewatch_frontend::test_support`）。
pub fn services(rec: &Recorder) -> Services {
    test_support::services(rec, at(NOW))
}

/// 以 World 的資料庫與設定開啟 App（Unicode 框線）。
pub fn app(world: World) -> (App, Recorder) {
    app_with_glyphs(world, glyphs::UNICODE)
}

pub fn app_with_glyphs(world: World, glyphs: glyphs::Glyphs) -> (App, Recorder) {
    app_with_config(world, glyphs, &cvewatch_core::config::example_config())
}

/// 以指定的設定檔內容開啟 App（設定檔寫入臨時目錄，設定畫面寫回此檔）。
pub fn app_with_config(world: World, glyphs: glyphs::Glyphs, config_text: &str) -> (App, Recorder) {
    let rec = Recorder::default();
    std::fs::write(rec.config_path(), config_text).expect("write config");
    let (config, _) =
        cvewatch_core::config::parse(config_text, &rec.config_path()).expect("parse config");
    let base = cvewatch_frontend::Context {
        config,
        config_path: rec.config_path(),
        conn: world.conn,
        settings: Settings {
            catalog: world.catalog,
            classes: world.classes,
            scoring: world.settings.scoring.clone(),
            stale_days: world.settings.stale_days,
        },
        services: services(&rec),
    };
    let ctx = Context { base, glyphs };
    (App::new(ctx).expect("app"), rec)
}

pub fn empty_world() -> World {
    world_with(None)
}

/// PA-460（PAN-OS 11.1.2，三筆命中，其一列入 KEV）、名稱很長的核心防火牆、PVE-01 與一筆僅關鍵字相符的等待區 CVE、
/// 兩筆同步紀錄（cvelist 成功、osv 失敗）、一筆未辨識產品。
pub fn seeded_world() -> World {
    let mut w = world_with(Some(PROXMOX_CNA));
    w.asset("PA-460", "internet", &[("paloalto-panos", "11.1.2")]);
    w.asset("FW-CORE", "internal", &[("paloalto-panos", "11.1.5")]);
    w.conn
        .execute(
            "UPDATE asset SET name = ?1, confidentiality = 3 WHERE asset_tag = 'FW-CORE'",
            params![LONG_NAME],
        )
        .expect("name");
    w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.2")]);
    panos_cves(&w, true);
    w.kev(KEV_CVE, "2026-10-20");
    w.epss(KEV_CVE, 0.945);
    // 僅關鍵字相符：進等待區、不自動建立 probable 命中
    keyword_only_pending(&w);
    w.run_at("2026-11-01T00:00:00+00:00");
    sync_run(&w, "cvelist", "2026-11-06T22:00:00+00:00", "success", None);
    sync_run(
        &w,
        "osv",
        "2026-11-06T22:10:00+00:00",
        "failed",
        Some("HTTP 503"),
    );
    w.conn
        .execute(
            "INSERT INTO asset_unresolved (asset_id, product_raw, version_raw, seen_at)
             VALUES ((SELECT id FROM asset WHERE asset_tag = 'PVE-01'), 'Synology DS920+', '7.2', ?1)",
            params![NOW],
        )
        .expect("unresolved");
    w
}

/// 已結束的同步紀錄。
pub fn sync_run(w: &World, source: &str, at: &str, status: &str, error: Option<&str>) {
    insert_run(&w.conn, source, at, status, error);
}

pub fn insert_run(
    conn: &rusqlite::Connection,
    source: &str,
    at: &str,
    status: &str,
    error: Option<&str>,
) {
    conn.execute(
        "INSERT INTO sync_run (started_at, finished_at, source, status, added_count, updated_count, error)
         VALUES (?1, ?1, ?2, ?3, 2, 1, ?4)",
        params![at, source, status, error],
    )
    .expect("sync_run");
}

pub fn render(app: &App) -> String {
    render_size(app, WIDTH, HEIGHT)
}

pub fn render_size(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal.draw(|f| ui::draw(f, app)).expect("draw");
    terminal.backend().to_string()
}

pub fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::from(code));
}

pub fn ctrl(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
}

/// 依序按下每個字元。
pub fn keys(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

/// 命中的 (CVE, 資產, 狀態)。
pub fn finding_status(app: &App, cve: &str, asset: &str) -> Option<(String, Option<String>)> {
    app.conn
        .query_row(
            "SELECT f.status, f.ignore_reason FROM finding f
             JOIN component c ON c.id = f.component_id JOIN asset a ON a.id = c.asset_id
             WHERE f.cve_id = ?1 AND a.asset_tag = ?2",
            params![cve, asset],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
}
