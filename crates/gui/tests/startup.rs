//! 啟動：錯誤畫面（資料庫不存在、金鑰遺失）與啟動警告（未知欄位）。

mod support;

use std::path::{Path, PathBuf};

use cvewatch_core::paths::{ConfigLocation, ConfigSource};
use cvewatch_core::CoreError;
use cvewatch_gui::ErrorScreen;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use support::*;

fn error_harness(screen: ErrorScreen) -> Harness<'static, (ErrorScreen, bool)> {
    let mut h = Harness::builder()
        .with_size([900.0, 500.0])
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, state: &mut (ErrorScreen, bool)| {
                if state.0.show(ui) {
                    state.1 = true;
                }
            },
            (screen, false),
        );
    h.run();
    h
}

#[test]
fn database_not_initialized_explains_init() {
    let cfg = Path::new(r"C:\Users\rz\AppData\Roaming\cvewatch\cvewatch.toml");
    let e = CoreError::DbNotFound(PathBuf::from(r"C:\data\cvewatch.db"));
    let mut h = error_harness(ErrorScreen::from_core(&e, Some(cfg)));
    assert!(h.query_by_label_contains("cvewatch init").is_some());
    assert!(h.query_by_label_contains("資料庫尚未建立").is_some());
    assert!(h.query_by_label_contains("cvewatch.toml").is_some());
    h.get_by_label("關閉").click();
    h.run();
    assert!(h.state().1, "按「關閉」後結束");
}

#[test]
fn missing_key_explains_import_key() {
    let h = error_harness(ErrorScreen::from_core(&CoreError::DbKeyMissing, None));
    assert!(h
        .query_by_label_contains("cvewatch db import-key")
        .is_some());
}

#[test]
fn unknown_config_key_is_a_warning_bar() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let path = dir.path().join("cvewatch.toml");
    let data_dir = data.display().to_string().replace('\\', "\\\\");
    std::fs::write(
        &path,
        format!("data_dir = \"{data_dir}\"\n[notify.emial]\nx = 1\n"),
    )
    .unwrap();
    let location = ConfigLocation {
        path,
        source: ConfigSource::CommandLine,
    };
    let runtime = cvewatch_core::runtime::start(&location, "gui-test").unwrap();
    let (h, _f) = gui_with(
        seeded_world(),
        Setup {
            warnings: runtime.warnings.clone(),
            ..Setup::default()
        },
    );
    assert!(
        shows(&h, "設定檔包含未知欄位 `notify.emial`（已忽略）"),
        "{:?}",
        runtime.warnings
    );
    assert!(shows(&h, "Dashboard"), "照常開啟主畫面");
}

#[test]
fn missing_database_from_real_startup_gives_error_screen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cvewatch.toml");
    let data_dir = dir
        .path()
        .join("data")
        .display()
        .to_string()
        .replace('\\', "\\\\");
    std::fs::write(&path, format!("data_dir = \"{data_dir}\"\n")).unwrap();
    let args = vec![
        std::ffi::OsString::from("--config"),
        path.clone().into_os_string(),
    ];
    let launched = cvewatch_gui::startup::launch(args, &dir.path().join("cvewatch-gui.exe"));
    match launched.root {
        cvewatch_gui::Root::Error(e) => {
            assert!(e.message.contains("cvewatch init"), "{}", e.message);
            assert_eq!(e.config_path.as_deref(), Some(path.as_path()));
        }
        cvewatch_gui::Root::App(_) => panic!("資料庫不存在時應顯示錯誤畫面"),
    }
}
