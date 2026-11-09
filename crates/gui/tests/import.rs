//! 清冊匯入：拖放或檔案對話框 → `--dry-run` 預覽 → 確認後實際匯入（fake companion）。

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cvewatch_gui::companion::Output;
use cvewatch_gui::dialogs::import::{Stage, UNSUPPORTED};
use cvewatch_gui::GuiApp;
use eframe::egui;
use egui_kittest::Harness;
use support::*;

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

fn drop_files(h: &mut Harness<'_, GuiApp>, paths: &[&Path]) {
    for p in paths {
        h.input_mut()
            .dropped_files
            .push(Arc::new(Dropped(p.to_path_buf())));
    }
    h.run();
}

fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

#[test]
fn dropped_spreadsheet_is_previewed_then_imported() {
    let cfg = PathBuf::from(r"D:\cvewatch\cvewatch.toml");
    let (mut h, f) = gui_with(
        seeded_world(),
        Setup {
            cli_config: Some(cfg.clone()),
            ..Setup::default()
        },
    );
    let file = f.rec.dir.path().join("assets.xlsx");
    drop_files(&mut h, &[&file]);
    let calls = f.companion.calls();
    assert_eq!(calls.len(), 1, "先只執行預覽");
    assert!(has(&calls[0], "--dry-run"));
    assert!(shows(&h, "新增 2、更新 1（版本變更 1）"), "顯示預覽");
    assert!(shows(&h, "資料庫尚未變更"));
    click(&mut h, "將清冊中消失的資產標為退役");
    let calls = f.companion.calls();
    assert_eq!(calls.len(), 2, "勾選後重新預覽");
    assert!(has(&calls[1], "--dry-run") && has(&calls[1], "--prune"));
    click(&mut h, "匯入");
    let calls = f.companion.calls();
    assert_eq!(calls.len(), 3);
    let last = &calls[2];
    assert!(!has(last, "--dry-run"), "{last:?}");
    assert!(has(last, "--prune"));
    assert_eq!(
        last[..5],
        [
            "--config".to_string(),
            cfg.display().to_string(),
            "asset".into(),
            "import".into(),
            file.display().to_string()
        ]
    );
    assert!(matches!(
        h.state().import.as_ref().map(|i| &i.stage),
        Some(Stage::Done(Ok(_)))
    ));
    assert!(shows(&h, "已匯入清冊並重新載入"));
    click(&mut h, "關閉");
    assert!(h.state().import.is_none());
}

#[test]
fn cancelling_after_preview_runs_nothing_more() {
    let (mut h, f) = gui(seeded_world());
    *f.dialogs.inventory.borrow_mut() = Some(f.rec.dir.path().join("assets.xlsx"));
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, "匯入清冊");
    assert_eq!(f.companion.calls().len(), 1);
    assert!(!has(&f.companion.calls()[0], "--config"), "未指定設定檔");
    click_last(&mut h, "取消");
    assert!(h.state().import.is_none());
    assert_eq!(f.companion.calls().len(), 1, "取消後不執行第二次");
}

#[test]
fn csv_can_choose_components_file() {
    let (mut h, f) = gui(seeded_world());
    let comps = f.rec.dir.path().join("components.csv");
    *f.dialogs.components.borrow_mut() = Some(comps.clone());
    drop_files(&mut h, &[&f.rec.dir.path().join("assets.csv")]);
    click(&mut h, "選擇元件檔");
    let calls = f.companion.calls();
    assert_eq!(calls.len(), 2);
    let i = calls[1]
        .iter()
        .position(|a| a == "--components")
        .expect("flag");
    assert_eq!(calls[1][i + 1], comps.display().to_string());
    assert!(has(&calls[1], "--dry-run"));
}

#[test]
fn unsupported_or_multiple_files_are_refused() {
    let (mut h, f) = gui(seeded_world());
    drop_files(&mut h, &[&f.rec.dir.path().join("report.pdf")]);
    assert!(shows(&h, UNSUPPORTED));
    assert!(h.state().import.is_none());
    let a = f.rec.dir.path().join("a.xlsx");
    let b = f.rec.dir.path().join("b.csv");
    drop_files(&mut h, &[&a, &b]);
    assert!(shows(&h, "一次只能匯入一個檔案"));
    assert!(f.companion.calls().is_empty());
}

#[test]
fn failed_preview_shows_stderr_and_blocks_import() {
    let (mut h, f) = gui(seeded_world());
    *f.companion.output.lock().unwrap() = Output {
        code: Some(2),
        stdout: String::new(),
        stderr: "清冊缺少欄位 asset_tag".into(),
    };
    drop_files(&mut h, &[&f.rec.dir.path().join("assets.xlsx")]);
    assert!(shows(&h, "清冊缺少欄位 asset_tag"));
    assert!(shows(&h, "結束碼 2"));
    assert!(is_disabled(&h, "匯入"));
}

#[test]
fn import_without_companion_is_refused() {
    let (mut h, f) = gui_with(
        seeded_world(),
        Setup {
            companion: false,
            ..Setup::default()
        },
    );
    drop_files(&mut h, &[&f.rec.dir.path().join("assets.xlsx")]);
    assert!(shows(&h, "cvewatch.exe"));
    assert!(h.state().import.is_none());
}

#[test]
fn import_with_failed_rematch_still_reloads() {
    let (mut h, f) = gui(seeded_world());
    drop_files(&mut h, &[&f.rec.dir.path().join("assets.xlsx")]);
    *f.companion.output.lock().unwrap() = Output {
        code: Some(4),
        stdout: "新增 2".into(),
        stderr: "自動比對失敗：資料庫忙碌".into(),
    };
    click(&mut h, "匯入");
    assert!(shows(&h, "清冊已匯入，但自動比對失敗"));
    assert!(shows(&h, "cvewatch match"));
    let status = h.state().status.clone().expect("status");
    assert_eq!(status.level, cvewatch_frontend::Level::Warning);
}

#[test]
fn drop_while_dialog_open_is_ignored() {
    let (mut h, f) = gui(seeded_world());
    ctrl(&mut h, egui::Key::Num2);
    click_finding(&mut h, KEV_CVE, "FW-EDGE", egui::Modifiers::NONE);
    click(&mut h, "已處理");
    drop_files(&mut h, &[&f.rec.dir.path().join("assets.xlsx")]);
    assert!(h.state().import.is_none());
    assert!(f.companion.calls().is_empty());
    assert!(h.state().dialog.is_some(), "備註對話框不受影響");
}

#[test]
fn failed_import_still_reloads_database_state() {
    let (mut h, f) = gui(seeded_world());
    drop_files(&mut h, &[&f.rec.dir.path().join("assets.xlsx")]);
    // 匯入步驟失敗（例如逾時被終止）：資料庫可能已寫入，仍重新載入
    *f.companion.output.lock().unwrap() = Output {
        code: None,
        stdout: String::new(),
        stderr: "已終止".into(),
    };
    let loaded_before = h.state().ws.data.loaded_at;
    h.state()
        .ws
        .conn
        .execute(
            "UPDATE asset SET name = '改名' WHERE asset_tag = 'FW-EDGE'",
            [],
        )
        .unwrap();
    click(&mut h, "匯入");
    let status = h.state().status.clone().expect("status");
    assert_eq!(status.level, cvewatch_frontend::Level::Error);
    assert!(status.message.contains("已重新載入"), "{}", status.message);
    let app = h.state();
    assert!(app.ws.data.loaded_at >= loaded_before);
    assert_eq!(
        app.ws.data.asset("FW-EDGE").unwrap().asset.record.name,
        "改名"
    );
}

/// 13 任務 5.4：預覽顯示 `asset import --dry-run` 的輸出，舊清冊的轉換提醒列在版本警告之後。
#[test]
fn legacy_inventory_preview_shows_conversion_notice() {
    let (mut h, f) = gui(seeded_world());
    f.companion.output.lock().unwrap().stdout = "匯入預覽（--dry-run，未寫入資料庫）\n\
         版本無法解析（仍保存原始字串）：\n  第 7 列 FW-02 paloalto-panos「11.1 latest」不符合 panos 格式\n\
         提醒：\n  清冊的 criticality 欄已停用，改用 confidentiality、integrity、availability\n\
         \x20 第 2 列 FW-01：security 已轉為 HW03 資安設備"
        .into();
    let file = f.rec.dir.path().join("inventory_legacy.xlsx");
    drop_files(&mut h, &[&file]);
    assert!(has(&f.companion.calls()[0], "--dry-run"));
    assert!(
        shows(&h, "security 已轉為 HW03 資安設備"),
        "預覽列出舊值轉換"
    );
    assert!(shows(&h, "criticality 欄已停用"));
}
