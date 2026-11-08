//! GUI 測試共用：記憶體資料庫（core 的比對測試建構器）、外部效果替身（frontend 的
//! `test_support`）、fake companion、檔案對話框替身、可推進的單調時鐘與 kittest Harness。
#![allow(dead_code)]

#[path = "../../../core/tests/common/mod.rs"]
pub mod common;
#[path = "../../../frontend/tests/shared/mod.rs"]
pub mod shared;

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::matching::{at, world_with, World, T0};
use cvewatch_frontend::data::Settings;
use cvewatch_frontend::services::CliTarget;
#[allow(unused_imports)]
pub use cvewatch_frontend::test_support::{self, named, FakeBackend, Recorder};
use cvewatch_gui::companion::{Companion, Output};
use cvewatch_gui::files::FileDialogs;
use cvewatch_gui::{GuiApp, Options};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use rusqlite::params;
use shared::{keyword_only_pending, panos_cves, PROXMOX_CNA};
#[allow(unused_imports)]
pub use shared::{PANOS_CVES, PENDING_CVE};

pub const NOW: &str = "2026-11-07T00:00:00+00:00";
pub const KEV_CVE: &str = "CVE-2026-0001";
pub const LONG_NAME: &str = "資訊所機房核心防火牆主機";
pub const VENDOR_URL: &str = "https://security.paloaltonetworks.com/CVE-2026-0001";
/// 影響 PAN-OS 11.1.0–11.1.6 前（PA-460 與 FW-CORE 都受影響）、不含 `2026-0` 的 CVE。
pub const OLD_CVE: &str = "CVE-2025-1111";

/// fake companion：以與正式環境相同的 `CliTarget` 組出命令並記錄其參數（含 `--config`），
/// 不啟動子程序，回傳固定輸出。
#[derive(Clone)]
pub struct FakeCompanion {
    pub target: CliTarget,
    pub calls: Arc<Mutex<Vec<Vec<String>>>>,
    pub output: Arc<Mutex<Output>>,
}

impl FakeCompanion {
    pub fn new(config: Option<PathBuf>) -> Self {
        FakeCompanion {
            target: CliTarget {
                exe: PathBuf::from(r"C:\Tools\cvewatch.exe"),
                config,
            },
            calls: Arc::default(),
            output: Arc::new(Mutex::new(Output {
                code: Some(0),
                stdout: "新增 2、更新 1（版本變更 1）".into(),
                stderr: String::new(),
            })),
        }
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().expect("calls").clone()
    }
}

impl Companion for FakeCompanion {
    fn run(&self, args: &[OsString]) -> Result<Output, String> {
        let cmd = self.target.command(args);
        let recorded = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        self.calls.lock().expect("calls").push(recorded);
        Ok(self.output.lock().expect("output").clone())
    }
}

/// 檔案對話框替身：回傳預先設定的路徑，記錄另存對話框的預設資料夾與檔名。
#[derive(Clone, Default)]
pub struct FakeDialogs {
    /// None 表示使用者取消；`Some(None)` 表示照預設資料夾與檔名確認
    pub save: Rc<RefCell<Option<Option<PathBuf>>>>,
    pub save_asked: Rc<RefCell<Vec<(PathBuf, String)>>>,
    /// 另存 Excel 對話框的標題
    pub xlsx_titles: Rc<RefCell<Vec<String>>>,
    pub inventory: Rc<RefCell<Option<PathBuf>>>,
    pub components: Rc<RefCell<Option<PathBuf>>>,
}

impl FileDialogs for FakeDialogs {
    fn save_vex(&self, dir: &Path, file_name: &str) -> Option<PathBuf> {
        self.save_asked
            .borrow_mut()
            .push((dir.to_path_buf(), file_name.to_string()));
        match self.save.borrow().clone() {
            None => None,
            Some(None) => Some(dir.join(file_name)),
            Some(Some(p)) => Some(p),
        }
    }

    fn save_xlsx(&self, title: &str, dir: &Path, file_name: &str) -> Option<PathBuf> {
        self.xlsx_titles.borrow_mut().push(title.to_string());
        self.save_vex(dir, file_name)
    }

    fn open_inventory(&self) -> Option<PathBuf> {
        self.inventory.borrow().clone()
    }

    fn open_components(&self) -> Option<PathBuf> {
        self.components.borrow().clone()
    }
}

/// 外部效果與替身。
pub struct Fakes {
    pub rec: Recorder,
    pub companion: FakeCompanion,
    pub dialogs: FakeDialogs,
    /// 單調時鐘的位移
    pub elapsed: Rc<Cell<Duration>>,
}

impl Fakes {
    /// 推進時鐘。
    pub fn advance(&self, d: Duration) {
        self.elapsed.set(self.elapsed.get() + d);
    }
}

/// 建立 App 的參數。
pub struct Setup {
    pub config: String,
    pub companion: bool,
    pub cli_config: Option<PathBuf>,
    pub warnings: Vec<String>,
}

impl Default for Setup {
    fn default() -> Self {
        Setup {
            config: cvewatch_core::config::example_config(),
            companion: true,
            cli_config: None,
            warnings: Vec::new(),
        }
    }
}

pub fn app(world: World, setup: Setup) -> (GuiApp, Fakes) {
    let rec = Recorder::default();
    std::fs::write(rec.config_path(), &setup.config).expect("write config");
    std::fs::create_dir_all(rec.documents_dir()).expect("documents");
    let (config, _) =
        cvewatch_core::config::parse(&setup.config, &rec.config_path()).expect("parse config");
    let ctx = cvewatch_frontend::Context {
        config,
        config_path: rec.config_path(),
        conn: world.conn,
        settings: Settings {
            catalog: world.catalog,
            classes: world.classes,
            scoring: world.settings.scoring.clone(),
            stale_days: world.settings.stale_days,
        },
        services: test_support::services(&rec, at(NOW)),
    };
    let companion = FakeCompanion::new(setup.cli_config);
    let dialogs = FakeDialogs::default();
    let elapsed = Rc::new(Cell::new(Duration::ZERO));
    let start = Instant::now();
    let clock = elapsed.clone();
    let options = Options {
        warnings: setup.warnings,
        companion: setup
            .companion
            .then(|| Arc::new(companion.clone()) as Arc<dyn Companion>),
        dialogs: Box::new(dialogs.clone()),
        instant: Box::new(move || start + clock.get()),
    };
    let app = GuiApp::new(ctx, options).expect("app");
    (
        app,
        Fakes {
            rec,
            companion,
            dialogs,
            elapsed,
        },
    )
}

/// 以 kittest Harness 執行 App（不需要 GPU；中文字型不一定存在，允許缺字）。
pub fn harness(app: GuiApp) -> Harness<'static, GuiApp> {
    let mut h = Harness::builder()
        .with_size([1400.0, 900.0])
        // 每步 0.05 秒：兩次點選落在按兩下的 0.3 秒內
        .with_step_dt(0.05)
        // 新增的表格列（Grid）第一次排版需要多繪製幾幀
        .with_max_steps(8)
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut GuiApp| app.show(ui), app);
    h.run();
    h
}

pub fn gui(world: World) -> (Harness<'static, GuiApp>, Fakes) {
    gui_with(world, Setup::default())
}

pub fn gui_with(world: World, setup: Setup) -> (Harness<'static, GuiApp>, Fakes) {
    let (app, fakes) = app(world, setup);
    (harness(app), fakes)
}

/// PA-460（PAN-OS 11.1.2，四筆命中，其一列入 KEV 並有原廠公告）、名稱很長的核心防火牆
/// （[`OLD_CVE`] 一筆命中）、
/// PVE-01 與一筆僅關鍵字相符的等待區 CVE、兩筆同步紀錄（cvelist 成功、osv 失敗）、一筆未辨識產品。
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
    w.cve(
        OLD_CVE,
        common::matching::panos(serde_json::json!([
            {"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}
        ])),
        serde_json::json!([]),
    );
    w.cvss(OLD_CVE, 5.0);
    vendor_advisory(&w);
    w.kev(KEV_CVE, "2026-10-20");
    w.epss(KEV_CVE, 0.945);
    keyword_only_pending(&w);
    w.run_at("2026-11-01T00:00:00+00:00");
    sync_run(
        &w.conn,
        "cvelist",
        "2026-11-06T22:00:00+00:00",
        "success",
        None,
    );
    sync_run(
        &w.conn,
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

/// [`KEV_CVE`] 的 Palo Alto 原廠公告（影響 11.1.0–11.1.3 前）與修正版本連結。
fn vendor_advisory(w: &World) {
    w.csaf(
        KEV_CVE,
        "paloalto-panos",
        "PAN-SA-2026-0001",
        T0,
        &[("vers:generic/>=11.1.0|<11.1.3", "range", "affected")],
    );
    w.conn
        .execute(
            "INSERT INTO csaf_fix (provider, tracking_id, cve_id, catalog_product_id, fixed_version, url)
             VALUES ('paloalto', 'PAN-SA-2026-0001', ?1,
                     (SELECT id FROM catalog_product WHERE key = 'paloalto-panos'), '11.1.3', ?2)",
            params![KEV_CVE, VENDOR_URL],
        )
        .expect("fix");
}

/// 已結束的同步紀錄。
pub fn sync_run(
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

/// 命中的（狀態、忽略原因、理由）。
pub fn finding_status(
    app: &GuiApp,
    cve: &str,
    asset: &str,
) -> (String, Option<String>, Option<String>) {
    app.ws
        .conn
        .query_row(
            "SELECT f.status, f.ignore_reason, f.justification FROM finding f
             JOIN component c ON c.id = f.component_id JOIN asset a ON a.id = c.asset_id
             WHERE f.cve_id = ?1 AND a.asset_tag = ?2",
            params![cve, asset],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("finding")
}

pub fn finding_id(app: &GuiApp, cve: &str, asset: &str) -> i64 {
    app.ws
        .data
        .findings
        .iter()
        .find(|f| f.cve_id == cve && f.asset_tag == asset)
        .map(|f| f.id)
        .expect("finding id")
}

/// 命中表格中 CVE 欄的顯示順序（只看表格列）。
pub fn table_cves(app: &GuiApp) -> Vec<(String, String)> {
    app.ws
        .findings
        .visible
        .iter()
        .map(|i| {
            let f = &app.ws.data.findings[*i];
            (f.cve_id.clone(), f.asset_tag.clone())
        })
        .collect()
}

/// 有任何節點的標籤包含 `text`。
pub fn shows(h: &Harness<'_, GuiApp>, text: &str) -> bool {
    h.query_all_by_label_contains(text).next().is_some()
}

/// 按 Ctrl+鍵。
pub fn ctrl(h: &mut Harness<'_, GuiApp>, key: egui::Key) {
    h.key_press_modifiers(egui::Modifiers::COMMAND, key);
    h.run();
}

/// 點選標籤完全相符的節點。
pub fn click(h: &mut Harness<'_, GuiApp>, label: &str) {
    h.get_by_label(label).click();
    h.run();
}

/// 依序點選命中表格中的列（第一列單選，其餘以 Ctrl 加選）。
pub fn select_rows(h: &mut Harness<'_, GuiApp>, rows: &[(&str, &str)]) {
    for (i, (cve, asset)) in rows.iter().enumerate() {
        let mods = if i == 0 {
            egui::Modifiers::NONE
        } else {
            egui::Modifiers::COMMAND
        };
        click_finding(h, cve, asset, mods);
    }
}

/// 點選命中表格中 (CVE, 資產) 那一列的 CVE 儲存格。
pub fn click_finding(h: &mut Harness<'_, GuiApp>, cve: &str, asset: &str, mods: egui::Modifiers) {
    let id = finding_id(h.state(), cve, asset);
    let ids = h.state().ws.visible_ids();
    let pos = ids.iter().position(|x| *x == id).expect("visible");
    // 同一個 CVE 在前面出現的次數：同名儲存格中的第幾個
    let earlier = ids[..pos]
        .iter()
        .filter(|x| {
            h.state()
                .ws
                .data
                .findings
                .iter()
                .any(|f| f.id == **x && f.cve_id == cve)
        })
        .count();
    h.get_all_by_label(cve)
        .nth(earlier)
        .expect("row cell")
        .click_modifiers(mods);
    h.run();
}

pub fn is_disabled(h: &Harness<'_, GuiApp>, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().is_disabled()
}

/// 命中時間軸最後一筆事件的（新狀態、備註）。
pub fn last_event(app: &GuiApp, finding: i64) -> (String, String) {
    app.ws
        .conn
        .query_row(
            "SELECT to_status, note FROM finding_event WHERE finding_id = ?1 ORDER BY id DESC LIMIT 1",
            [finding],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("event")
}

/// 在搜尋框輸入文字（先取得焦點）。
pub fn search(h: &mut Harness<'_, GuiApp>, text: &str) {
    h.get_by_label("搜尋").focus();
    h.run();
    h.get_by_label("搜尋").type_text(text);
    h.run();
}

/// 點選下拉選單中的選項（同名的表格儲存格是 Label，選項不是）。
pub fn click_option(h: &mut Harness<'_, GuiApp>, label: &str) {
    h.get_all_by_label(label)
        .find(|n| n.accesskit_node().role() != egui::accesskit::Role::Label)
        .expect("option")
        .click();
    h.run();
}

/// 點選同名節點中最後繪製者（對話框在畫面之後繪製，例如確認對話框的「儲存」「取消」）。
pub fn click_last(h: &mut Harness<'_, GuiApp>, label: &str) {
    h.get_all_by_label(label).last().expect(label).click();
    h.run();
}

/// 排入的背景工作。
pub type Jobs = Arc<Mutex<Vec<Box<dyn FnOnce() + Send>>>>;

/// 背景工作延後執行的 spawner（模擬子程序尚未回應；`run_all` 時才執行）。
#[derive(Clone, Default)]
pub struct DeferredSpawner {
    pub jobs: Jobs,
}

impl cvewatch_frontend::settings::Spawner for DeferredSpawner {
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) {
        self.jobs.lock().expect("jobs").push(job);
    }
}

impl DeferredSpawner {
    /// 執行目前排入的工作，回傳執行數。
    pub fn run_all(&self) -> usize {
        let jobs: Vec<_> = std::mem::take(&mut *self.jobs.lock().expect("jobs"));
        let n = jobs.len();
        for job in jobs {
            job();
        }
        n
    }

    pub fn pending(&self) -> usize {
        self.jobs.lock().expect("jobs").len()
    }
}

/// 狀態列目前的訊息。
pub fn status(h: &Harness<'_, GuiApp>) -> String {
    h.state()
        .status
        .as_ref()
        .map(|r| r.message.clone())
        .unwrap_or_default()
}

/// 取代文字欄位的內容（取得焦點 → 全選 → 輸入）。
pub fn replace_text(h: &mut Harness<'_, GuiApp>, label: &str, text: &str) {
    h.get_by_label(label).focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_label(label).type_text(text);
    h.run();
}

/// 進入資產畫面並選取資產。
pub fn select_asset(h: &mut Harness<'_, GuiApp>, tag: &str) {
    ctrl(h, egui::Key::Num3);
    h.state_mut().assets_selected = Some(tag.to_string());
    h.run();
}

/// 以編輯對話框修改元件版本並儲存（`product` 為產品顯示名稱）。
pub fn edit_version(h: &mut Harness<'_, GuiApp>, tag: &str, product: &str, version: &str) {
    select_asset(h, tag);
    click(h, "編輯");
    replace_text(h, &format!("{product} 版本"), version);
    click(h, "儲存");
}

/// 資產清單（左側表格）中標籤完全相符的儲存格：位於「未處理」欄位標題右緣之左
/// （清單先於詳情繪製；詳情的命中狀態也可能是「未處理」，取第一個）。
pub fn list_cells<'a>(h: &'a Harness<'_, GuiApp>, label: &'a str) -> Vec<egui_kittest::Node<'a>> {
    let Some(edge) = h
        .query_all_by_label("未處理")
        .next()
        .map(|n| n.rect().right())
    else {
        return Vec::new();
    };
    h.query_all_by_label(label)
        .filter(|n| n.rect().left() < edge)
        .collect()
}

/// 資產清單中有標籤為 `label` 的儲存格。
pub fn listed(h: &Harness<'_, GuiApp>, label: &str) -> bool {
    !list_cells(h, label).is_empty()
}

/// 點選資產清單中該資產的編號儲存格。
pub fn pick_asset(h: &mut Harness<'_, GuiApp>, tag: &str) {
    list_cells(h, tag).first().expect("listed").click();
    h.run();
}

/// fake companion 收到的 `match` 子命令次數。
pub fn match_calls(f: &Fakes) -> usize {
    f.companion
        .calls()
        .iter()
        .filter(|c| c.last().map(String::as_str) == Some("match"))
        .count()
}
