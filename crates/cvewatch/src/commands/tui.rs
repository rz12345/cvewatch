//! `cvewatch tui`：開啟文字介面（實作在 `cvewatch-tui`）。

use std::sync::Arc;

use cvewatch_core::db::open_working_db;
use cvewatch_core::secrets::KeyringStore;
use cvewatch_core::{CoreError, ExitCode};
use cvewatch_tui::data::Settings;
use cvewatch_tui::services::{DisplayTz, ProcessLauncher, Services, SystemClipboard, SystemOpener};
use cvewatch_tui::settings::jobs::LiveBackend;
use cvewatch_tui::settings::ThreadSpawner;
use cvewatch_tui::{glyphs, Context};

use crate::app::App;

pub fn run(app: &App) -> Result<ExitCode, CoreError> {
    let catalog = app.load_catalog()?;
    let conn = open_working_db(&app.paths, &app.store)?;
    let exe = std::env::current_exe()
        .map_err(|e| CoreError::Invalid(format!("無法取得執行檔路徑：{e}")))?;
    // 明確指定的設定檔傳給背景 sync 與排程安裝；環境變數 CVEWATCH_CONFIG 會自動繼承
    // 相對路徑轉為絕對路徑：重新安裝排程時寫入排程動作（其工作目錄為執行檔所在資料夾）
    let config = app
        .location()
        .is_explicit()
        .then(|| crate::schedule::absolute(&app.location().path))
        .transpose()?;
    let config_values = &app.loaded.config;
    let ctx = Context {
        conn,
        settings: Settings {
            catalog,
            scoring: config_values.scoring.clone(),
            stale_days: config_values.pending.stale_days,
        },
        config: config_values.clone(),
        config_path: app.location().path.clone(),
        services: Services {
            opener: Box::new(SystemOpener),
            clipboard: Box::new(SystemClipboard),
            launcher: Box::new(ProcessLauncher {
                exe: exe.clone(),
                config: config.clone(),
            }),
            clock: Box::new(chrono::Utc::now),
            tz: DisplayTz::Local,
            secrets: Box::new(KeyringStore::default()),
            backend: Arc::new(LiveBackend { exe, config }),
            spawner: Box::new(ThreadSpawner),
            documents_dir: cvewatch_core::paths::documents_dir(),
        },
        glyphs: glyphs::detect(),
    };
    cvewatch_tui::run(ctx)?;
    Ok(ExitCode::Success)
}
