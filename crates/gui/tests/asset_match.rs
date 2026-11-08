//! 10 任務 8.3：編修後的背景比對（fake companion 延遲回應、找不到 companion、比對中再次編修）
//! 與匯入預覽的覆寫提醒。

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cvewatch_gui::app::Screen;
use cvewatch_gui::companion::Output;
use cvewatch_gui::dialogs::import::OVERWRITE_REMINDER;
use cvewatch_gui::matching::MATCHING;
use cvewatch_gui::GuiApp;
use eframe::egui;
use egui_kittest::Harness;
use support::*;

const MATCH_OUTPUT: &str = "比對完成：使用中元件 3、本機 CVE 9\n  新命中 0、自動轉為已處理 1";

/// 背景工作改為延後執行，回傳其控制。
fn deferred(h: &mut Harness<'_, GuiApp>) -> DeferredSpawner {
    let s = DeferredSpawner::default();
    h.state_mut().ws.services.spawner = Box::new(s.clone());
    s
}

fn match_output(f: &Fakes) {
    *f.companion.output.lock().unwrap() = Output {
        code: Some(0),
        stdout: MATCH_OUTPUT.into(),
        stderr: String::new(),
    };
}

fn db_version(h: &Harness<'_, GuiApp>, tag: &str) -> String {
    h.state()
        .ws
        .conn
        .query_row(
            "SELECT c.version_raw FROM component c JOIN asset a ON a.id = c.asset_id
             WHERE a.asset_tag = ?1",
            [tag],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn asset_match_runs_in_the_background() {
    let (mut h, f) = gui(seeded_world());
    match_output(&f);
    let spawner = deferred(&mut h);
    edit_version(&mut h, "PA-460", "PAN-OS", "11.1.6");
    assert_eq!(db_version(&h, "PA-460"), "11.1.6", "編修已寫入");
    assert!(status(&h).ends_with(MATCHING), "{}", status(&h));
    assert!(h.state().matching.running());
    // 比對進行中視窗仍可操作
    ctrl(&mut h, egui::Key::Num2);
    assert_eq!(h.state().screen, Screen::Findings);
    assert_eq!(spawner.run_all(), 1);
    h.run();
    assert!(!h.state().matching.running());
    let s = status(&h);
    assert!(s.contains("已儲存 PA-460") && s.contains("比對完成"), "{s}");
    assert_eq!(match_calls(&f), 1);
}

#[test]
fn asset_match_companion_missing() {
    let (mut h, f) = gui_with(
        seeded_world(),
        Setup {
            companion: false,
            ..Setup::default()
        },
    );
    edit_version(&mut h, "PA-460", "PAN-OS", "11.1.6");
    assert_eq!(db_version(&h, "PA-460"), "11.1.6", "編修已寫入");
    let r = h.state().status.clone().expect("status");
    assert_eq!(r.level, cvewatch_frontend::Level::Warning);
    assert!(
        r.message.contains("尚未比對") && r.message.contains("cvewatch match"),
        "{}",
        r.message
    );
    assert!(f.companion.calls().is_empty());
}

#[test]
fn asset_match_failure_keeps_the_edit_and_warns() {
    let (mut h, f) = gui(seeded_world());
    *f.companion.output.lock().unwrap() = Output {
        code: Some(1),
        stdout: String::new(),
        stderr: "資料庫鎖定中".into(),
    };
    edit_version(&mut h, "PA-460", "PAN-OS", "11.1.6");
    h.run();
    let r = h.state().status.clone().expect("status");
    assert_eq!(r.level, cvewatch_frontend::Level::Warning);
    assert!(
        r.message.contains("比對失敗") && r.message.contains("cvewatch match"),
        "{}",
        r.message
    );
    assert_eq!(db_version(&h, "PA-460"), "11.1.6");
}

#[test]
fn asset_match_edits_during_matching_run_once_more_afterwards() {
    let (mut h, f) = gui(seeded_world());
    match_output(&f);
    let spawner = deferred(&mut h);
    edit_version(&mut h, "PA-460", "PAN-OS", "11.1.6");
    edit_version(&mut h, "PA-460", "PAN-OS", "11.1.7");
    edit_version(&mut h, "FW-CORE", "PAN-OS", "11.1.8");
    assert_eq!(spawner.pending(), 1, "進行中不排第二個比對");
    spawner.run_all();
    h.run();
    // fake companion 不會清除「比對待補」：結束後再跑一次（只一次）
    assert_eq!(spawner.pending(), 1);
    spawner.run_all();
    h.run();
    assert_eq!(spawner.pending(), 0);
    assert_eq!(match_calls(&f), 2);
    assert!(!h.state().matching.running());
}

#[derive(Debug)]
struct Dropped(PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

#[test]
fn asset_match_previewing_after_on_screen_edits() {
    let (mut h, f) = gui(seeded_world());
    edit_version(&mut h, "PA-460", "PAN-OS", "11.1.6");
    *f.companion.output.lock().unwrap() = Output {
        code: Some(0),
        stdout: "更新 1（版本變更 1）\n  PA-460 paloalto-panos 11.1.6 → 11.1.2".into(),
        stderr: String::new(),
    };
    let calls_before = f.companion.calls().len();
    let file = f.rec.dir.path().join("old.xlsx");
    h.input_mut().dropped_files.push(Arc::new(Dropped(file)));
    h.run();
    assert!(shows(&h, OVERWRITE_REMINDER), "預覽顯示覆寫提醒");
    assert!(
        shows(&h, "PA-460 paloalto-panos 11.1.6 → 11.1.2"),
        "版本變更明細"
    );
    assert_eq!(f.companion.calls().len(), calls_before + 1, "只執行預覽");
    assert_eq!(db_version(&h, "PA-460"), "11.1.6", "按「匯入」前資料庫不變");
}

#[test]
fn asset_match_result_keeps_the_report_of_an_edit_made_meanwhile() {
    let (mut h, f) = gui(seeded_world());
    match_output(&f);
    let spawner = deferred(&mut h);
    edit_version(&mut h, "PA-460", "PAN-OS", "11.1.6");
    select_asset(&mut h, "PVE-01");
    click(&mut h, "退役…");
    click(&mut h, "標為退役");
    assert!(status(&h).ends_with(MATCHING), "{}", status(&h));
    spawner.run_all();
    h.run();
    let s = status(&h);
    assert!(
        s.contains("已將 PVE-01 標為退役") && s.contains("比對完成"),
        "{s}"
    );
}
