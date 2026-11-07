//! 執行環境：設定位置、載入結果、資料目錄、憑證庫與日誌。

use cvewatch_core::config::{self, LoadedConfig};
use cvewatch_core::logging::{self, LogGuard};
use cvewatch_core::paths::{
    default_config_path, resolve_config_location, ConfigLocation, DataPaths, CONFIG_ENV_VAR,
};
use cvewatch_core::secrets::KeyringStore;
use cvewatch_core::CoreError;

pub struct App {
    pub loaded: LoadedConfig,
    pub paths: DataPaths,
    pub store: KeyringStore,
    _log_guard: Option<LogGuard>,
}

impl App {
    pub fn location(&self) -> &ConfigLocation {
        &self.loaded.location
    }
}

pub fn resolve_location(cli_path: Option<std::path::PathBuf>) -> Result<ConfigLocation, CoreError> {
    Ok(resolve_config_location(
        cli_path,
        std::env::var_os(CONFIG_ENV_VAR),
        default_config_path(),
    )?)
}

/// 載入設定 → 解析資料目錄 → 啟動日誌與清理 → 輸出未知欄位警告。
pub fn start(location: &ConfigLocation, command: &str) -> Result<App, CoreError> {
    let loaded = config::load(location)?;
    let paths = DataPaths::resolve(loaded.config.data_dir.as_deref())?;
    let log_guard = start_logging(&paths, &loaded);
    tracing::info!(command, "cvewatch 啟動");
    if loaded.used_defaults {
        tracing::info!("使用預設設定");
    }
    for key in &loaded.unknown_keys {
        eprintln!("警告：設定檔包含未知欄位 `{key}`（已忽略）");
        tracing::warn!(key = %key, "設定檔包含未知欄位");
    }
    Ok(App {
        loaded,
        paths,
        store: KeyringStore::default(),
        _log_guard: log_guard,
    })
}

/// 日誌失敗不影響命令執行，只提示使用者。
fn start_logging(paths: &DataPaths, loaded: &LoadedConfig) -> Option<LogGuard> {
    let guard = match logging::init(&paths.log_dir, loaded.config.log.level) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("警告：無法寫入日誌檔：{e}");
            return None;
        }
    };
    let today = chrono::Local::now().date_naive();
    match logging::cleanup_old_logs(&paths.log_dir, loaded.config.log.retention_days, today) {
        Ok(removed) if !removed.is_empty() => {
            tracing::info!(removed = removed.len(), "已刪除過期日誌檔");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "清理過期日誌失敗"),
    }
    Some(guard)
}
