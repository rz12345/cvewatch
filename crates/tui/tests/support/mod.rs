//! TUI 測試共用：記憶體資料庫（沿用 core 的比對測試建構器）、外部效果替身、TestBackend 繪製與按鍵。
#![allow(dead_code)]

#[path = "../../../core/tests/common/mod.rs"]
pub mod common;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use common::matching::{affected, at, panos, world_with, World};
use cvewatch_core::config::Config;
use cvewatch_core::notify::redmine::Named;
use cvewatch_core::secrets::{MemoryStore, SecretError, SecretKey, SecretStore};
use cvewatch_tui::data::Settings;
use cvewatch_tui::services::{
    ClipboardSink, DisplayTz, Opener, RunningSync, Services, SyncLauncher,
};
use cvewatch_tui::settings::{NotifyBackend, RedmineLists, Spawner};
use cvewatch_tui::{glyphs, ui, App, Context};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use rusqlite::params;
use serde_json::json;
use zeroize::Zeroizing;

/// 測試時鐘。
pub const NOW: &str = "2026-11-07T00:00:00+00:00";
pub const WIDTH: u16 = 110;
pub const HEIGHT: u16 = 26;

pub const KEV_CVE: &str = "CVE-2026-0001";
pub const PANOS_CVES: [&str; 3] = ["CVE-2026-0001", "CVE-2026-0002", "CVE-2026-0003"];
pub const PENDING_CVE: &str = "CVE-2099-20001";
pub const LONG_NAME: &str = "資訊所機房核心防火牆主機";

/// 種子目錄沒有 Proxmox 的 CNA 寫法，以 catalog.d 補上。
pub const PROXMOX_CNA: &str = r#"
[[product]]
key = "proxmox-ve"
cna = [{ vendor = "Proxmox Server Solutions GmbH", product = "Proxmox VE" }]
"#;

/// 設定畫面的連線替身：記錄呼叫（含使用的設定值）並回傳預設結果。
#[derive(Default)]
pub struct FakeBackend {
    pub calls: Mutex<Vec<String>>,
    pub email: Mutex<Option<Result<String, String>>>,
    pub redmine: Mutex<Option<Result<String, String>>>,
    pub lists: Mutex<Option<Result<RedmineLists, String>>>,
    pub project_lists: Mutex<Option<Result<RedmineLists, String>>>,
    /// 依專案回傳不同的 tracker／成員清單（優先於 `project_lists`）
    pub lists_by_project: Mutex<std::collections::HashMap<String, RedmineLists>>,
}

impl FakeBackend {
    fn log(&self, s: String) {
        self.calls.lock().expect("calls").push(s);
    }
}

impl NotifyBackend for FakeBackend {
    fn test_email(
        &self,
        c: &Config,
        password: Option<Zeroizing<String>>,
    ) -> Result<String, String> {
        let e = &c.notify.email;
        self.log(format!(
            "test_email {}:{} {} password={}",
            e.smtp_host,
            e.smtp_port,
            e.security,
            password.is_some()
        ));
        self.email
            .lock()
            .expect("email")
            .clone()
            .unwrap_or(Ok("已寄出測試信".into()))
    }

    fn test_redmine(&self, c: &Config, key: Option<Zeroizing<String>>) -> Result<String, String> {
        self.log(format!(
            "test_redmine {} key={}",
            c.notify.redmine.base_url,
            key.is_some()
        ));
        self.redmine
            .lock()
            .expect("redmine")
            .clone()
            .unwrap_or(Ok("Redmine 連線正常".into()))
    }

    fn redmine_lists(
        &self,
        _c: &Config,
        _key: Option<Zeroizing<String>>,
        project: Option<String>,
    ) -> Result<RedmineLists, String> {
        self.log(format!("lists {project:?}"));
        if let Some(l) = project
            .as_ref()
            .and_then(|p| self.lists_by_project.lock().expect("by").get(p).cloned())
        {
            return Ok(l);
        }
        let slot = if project.is_some() {
            &self.project_lists
        } else {
            &self.lists
        };
        slot.lock()
            .expect("lists")
            .clone()
            .unwrap_or(Ok(RedmineLists::default()))
    }

    fn install_schedule(&self) -> Result<String, String> {
        self.log("install_schedule".into());
        Ok("排程已重新安裝".into())
    }
}

/// 立即執行（測試中背景工作同步完成，下一次 tick 即可取回結果）。
pub struct InlineSpawner;

impl Spawner for InlineSpawner {
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) {
        job();
    }
}

/// 共用的記憶體憑證庫；`fails` 為真時模擬 Credential Manager 無法存取。
pub struct SharedStore(pub Arc<MemoryStore>, pub Arc<std::sync::atomic::AtomicBool>);

impl SharedStore {
    fn check(&self, key: SecretKey) -> Result<(), SecretError> {
        if self.1.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SecretError::Backend {
                key,
                reason: "無法存取憑證庫".into(),
            });
        }
        Ok(())
    }
}

impl SecretStore for SharedStore {
    fn get(&self, key: SecretKey) -> Result<Option<Zeroizing<String>>, SecretError> {
        self.check(key)?;
        self.0.get(key)
    }
    fn set(&self, key: SecretKey, value: &str) -> Result<(), SecretError> {
        self.check(key)?;
        self.0.set(key, value)
    }
    fn delete(&self, key: SecretKey) -> Result<(), SecretError> {
        self.check(key)?;
        self.0.delete(key)
    }
}

pub fn named(id: u64, name: &str) -> Named {
    Named {
        id,
        name: name.into(),
    }
}

/// 外部效果的紀錄與替身設定。
#[derive(Clone)]
pub struct Recorder {
    /// 設定檔所在的臨時目錄
    pub dir: Rc<tempfile::TempDir>,
    pub secrets: Arc<MemoryStore>,
    /// 模擬憑證庫無法存取
    pub store_fails: Arc<std::sync::atomic::AtomicBool>,
    pub backend: Arc<FakeBackend>,
    pub opened: Rc<RefCell<Vec<String>>>,
    pub copied: Rc<RefCell<Vec<String>>>,
    pub launches: Rc<Cell<usize>>,
    /// 背景 sync 的結束狀態；None 表示仍在執行
    pub exit: Rc<RefCell<Option<Option<i32>>>>,
    pub clipboard_fails: Rc<Cell<bool>>,
}

struct FakeOpener(Recorder);
impl Opener for FakeOpener {
    fn open(&self, url: &str) -> Result<(), String> {
        self.0.opened.borrow_mut().push(url.to_string());
        Ok(())
    }
}

struct FakeClipboard(Recorder);
impl ClipboardSink for FakeClipboard {
    fn copy(&self, text: &str) -> Result<(), String> {
        if self.0.clipboard_fails.get() {
            return Err("剪貼簿無法使用".into());
        }
        self.0.copied.borrow_mut().push(text.to_string());
        Ok(())
    }
}

struct FakeLauncher(Recorder);
struct FakeChild(Recorder);

impl SyncLauncher for FakeLauncher {
    fn launch(&self) -> Result<Box<dyn RunningSync>, String> {
        self.0.launches.set(self.0.launches.get() + 1);
        Ok(Box::new(FakeChild(self.0.clone())))
    }
}

impl RunningSync for FakeChild {
    fn try_wait(&mut self) -> Result<Option<Option<i32>>, String> {
        Ok(*self.0.exit.borrow())
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder {
            dir: Rc::new(tempfile::tempdir().expect("tempdir")),
            secrets: Arc::new(MemoryStore::new()),
            store_fails: Arc::default(),
            backend: Arc::new(FakeBackend::default()),
            opened: Rc::default(),
            copied: Rc::default(),
            launches: Rc::default(),
            exit: Rc::default(),
            clipboard_fails: Rc::default(),
        }
    }
}

impl Recorder {
    pub fn documents_dir(&self) -> PathBuf {
        self.dir.path().join("Documents")
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.path().join("cvewatch.toml")
    }

    pub fn config_text(&self) -> String {
        std::fs::read_to_string(self.config_path()).expect("config")
    }
}

pub fn services(rec: &Recorder) -> Services {
    Services {
        opener: Box::new(FakeOpener(rec.clone())),
        clipboard: Box::new(FakeClipboard(rec.clone())),
        launcher: Box::new(FakeLauncher(rec.clone())),
        clock: Box::new(|| at(NOW)),
        tz: DisplayTz::Fixed(chrono::FixedOffset::east_opt(0).expect("utc")),
        secrets: Box::new(SharedStore(rec.secrets.clone(), rec.store_fails.clone())),
        backend: rec.backend.clone(),
        spawner: Box::new(InlineSpawner),
        documents_dir: rec.documents_dir(),
    }
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
    let ctx = Context {
        config,
        config_path: rec.config_path(),
        conn: world.conn,
        settings: Settings {
            catalog: world.catalog,
            scoring: world.settings.scoring.clone(),
            stale_days: world.settings.stale_days,
        },
        services: services(&rec),
        glyphs,
    };
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
            "UPDATE asset SET name = ?1, criticality = 3 WHERE asset_tag = 'FW-CORE'",
            params![LONG_NAME],
        )
        .expect("name");
    w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.2")]);
    for (i, cve) in PANOS_CVES.iter().enumerate() {
        let upper = format!("11.1.{}", 3 + i);
        w.cve(
            cve,
            panos(json!([{"version": "11.1.0", "lessThan": upper, "status": "affected"}])),
            json!([]),
        );
        w.cvss(cve, 9.8 - i as f64);
    }
    w.kev(KEV_CVE, "2026-10-20");
    w.epss(KEV_CVE, 0.945);
    w.cve(
        PENDING_CVE,
        json!([affected(
            "Proxmox Server Solutions GmbH",
            "Proxmox VE",
            None,
            json!([])
        )]),
        json!([]),
    );
    // 僅關鍵字相符：進等待區、不自動建立 probable 命中
    w.conn
        .execute(
            "DELETE FROM cve_affected WHERE cve_id = ?1",
            params![PENDING_CVE],
        )
        .expect("affected");
    w.conn
        .execute(
            "UPDATE cve SET relevance_reason = 'keyword_only',
                            description_en = 'Proxmox VE privilege escalation' WHERE id = ?1",
            params![PENDING_CVE],
        )
        .expect("desc");
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
