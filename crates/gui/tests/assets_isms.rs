//! 18 任務 6.4：資產畫面的「從 ISMS 清冊匯入」（檔案對話框與 companion 替身）：預覽 → 匯入、
//! 取消、不是 ISMS 清冊、找不到 cvewatch.exe 時停用。

mod support;

use std::path::PathBuf;

use cvewatch_gui::companion::Output;
use cvewatch_gui::dialogs::import::{Stage, ISMS_OVERWRITE_REMINDER, PENDING_WARNING};
use cvewatch_gui::screens::asset_list::ISMS_IMPORT_BUTTON;
use eframe::egui;
use support::*;

const PREVIEW: &str = "新增 5、更新 1（版本變更 0）、未變 0、版本待補 7 個元件、略過 10 列";

fn workbook() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join("isms_workbook.xlsx")
}

fn set_output(f: &Fakes, code: i32, stdout: &str, stderr: &str) {
    *f.companion.output.lock().unwrap() = Output {
        code: Some(code),
        stdout: stdout.into(),
        stderr: stderr.into(),
    };
}

fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

#[test]
fn assets_isms_importing_from_the_window() {
    let (mut h, f) = gui(seeded_world());
    *f.dialogs.isms.borrow_mut() = Some(workbook());
    set_output(&f, 0, PREVIEW, "");
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, ISMS_IMPORT_BUTTON);
    let calls = f.companion.calls();
    assert_eq!(
        calls,
        [vec![
            "asset".to_string(),
            "import-isms".into(),
            workbook().display().to_string(),
            "--dry-run".into()
        ]],
        "先只執行預覽"
    );
    assert!(shows(&h, PREVIEW), "顯示預覽");
    assert!(shows(&h, ISMS_OVERWRITE_REMINDER));
    assert!(shows(&h, "資料庫尚未變更"));
    assert!(
        !shows(&h, "將清冊中消失的資產標為退役"),
        "ISMS 匯入沒有退役選項"
    );
    assert!(!shows(&h, "選擇元件檔"));
    click(&mut h, "匯入");
    let calls = f.companion.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1][..2], ["asset".to_string(), "import-isms".into()]);
    assert!(!has(&calls[1], "--dry-run") && !has(&calls[1], "--prune"));
    assert!(matches!(
        h.state().import.as_ref().map(|i| &i.stage),
        Some(Stage::Done(Ok(_)))
    ));
    assert!(
        status(&h).contains("已從 ISMS 清冊匯入並重新載入"),
        "{}",
        status(&h)
    );
}

#[test]
fn assets_isms_cancelling_the_preview() {
    let (mut h, f) = gui(seeded_world());
    *f.dialogs.isms.borrow_mut() = Some(workbook());
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, ISMS_IMPORT_BUTTON);
    assert_eq!(f.companion.calls().len(), 1);
    click_last(&mut h, "取消");
    assert!(h.state().import.is_none());
    assert_eq!(f.companion.calls().len(), 1, "取消後不執行匯入");

    // 檔案對話框就取消：不執行 companion
    *f.dialogs.isms.borrow_mut() = None;
    click(&mut h, ISMS_IMPORT_BUTTON);
    assert_eq!(f.companion.calls().len(), 1);
    assert!(status(&h).contains("已取消"), "{}", status(&h));
}

#[test]
fn assets_isms_not_an_isms_workbook_in_the_window() {
    let (mut h, f) = gui(seeded_world());
    *f.dialogs.isms.borrow_mut() = Some(workbook());
    set_output(&f, 1, "", "錯誤：找不到 ISMS 資訊資產清冊的表頭");
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, ISMS_IMPORT_BUTTON);
    assert!(shows(&h, "無法預覽："));
    assert!(
        h.state()
            .import
            .as_ref()
            .is_some_and(|i| matches!(&i.stage, Stage::Preview(Ok(o)) if o.stderr.contains("找不到 ISMS 資訊資產清冊的表頭"))),
        "顯示錯誤"
    );
    assert!(is_disabled(&h, "匯入"), "不能匯入");
    assert_eq!(f.companion.calls().len(), 1);
}

#[test]
fn assets_isms_button_is_disabled_without_companion() {
    let (mut h, f) = gui_with(
        seeded_world(),
        Setup {
            companion: false,
            ..Setup::default()
        },
    );
    *f.dialogs.isms.borrow_mut() = Some(workbook());
    ctrl(&mut h, egui::Key::Num3);
    assert!(is_disabled(&h, ISMS_IMPORT_BUTTON));
    assert!(f.companion.calls().is_empty());
}

#[test]
fn assets_isms_preview_warns_about_pending_versions() {
    let (mut h, f) = gui(seeded_world());
    *f.dialogs.isms.borrow_mut() = Some(workbook());
    let reminder = cvewatch_core::assets::pending::PENDING_REMINDER;
    set_output(&f, 0, &format!("{PREVIEW}\n{reminder}"), "");
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, ISMS_IMPORT_BUTTON);
    assert!(shows(&h, PENDING_WARNING), "預覽有版本待補時警告");
    click_last(&mut h, "取消");

    // 只在字詞中提到「未納入比對」（例如備註）不算
    set_output(&f, 0, &format!("{PREVIEW}\n備註：未納入比對的說明"), "");
    click(&mut h, ISMS_IMPORT_BUTTON);
    assert!(!shows(&h, PENDING_WARNING));
}

#[test]
fn assets_isms_match_failure_uses_isms_wording() {
    let (mut h, f) = gui(seeded_world());
    *f.dialogs.isms.borrow_mut() = Some(workbook());
    set_output(&f, 0, PREVIEW, "");
    ctrl(&mut h, egui::Key::Num3);
    click(&mut h, ISMS_IMPORT_BUTTON);
    set_output(
        &f,
        cvewatch_core::ExitCode::PartialFailure as i32,
        "ISMS 匯入完成",
        "比對失敗",
    );
    click(&mut h, "匯入");
    let s = status(&h);
    assert!(
        s.contains("已從 ISMS 清冊匯入並重新載入，但匯入後的自動比對失敗"),
        "{s}"
    );
    assert!(s.contains("版本待補的元件請在編輯對話框補上版本"), "{s}");
}
