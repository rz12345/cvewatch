//! 執行環境啟動：載入設定 → 解析資料目錄 → 啟動日誌與清理 → 建立憑證庫。
//!
//! CLI、TUI 與 GUI 共用。啟動時的警告（未知欄位、日誌無法寫入）不直接輸出，
//! 放在 [`Runtime::warnings`] 由呼叫端決定顯示方式（CLI 印到 stderr，GUI 以對話框顯示）。

use std::path::PathBuf;

use crate::catalog::Catalog;
use crate::config::{self, LoadedConfig};
use crate::logging::{self, LogGuard};
use crate::paths::{ConfigLocation, DataPaths};
use crate::secrets::KeyringStore;
use crate::CoreError;

/// 啟動後的執行環境。持有日誌的背景寫入器：必須存活到程式結束，提早 drop 會遺失日誌。
pub struct Runtime {
    pub loaded: LoadedConfig,
    pub paths: DataPaths,
    pub store: KeyringStore,
    /// 啟動時的警告（不含「警告：」前綴），依發生順序
    pub warnings: Vec<String>,
    _log_guard: Option<LogGuard>,
}

impl Runtime {
    pub fn location(&self) -> &ConfigLocation {
        &self.loaded.location
    }

    /// 產品目錄：內建種子 + `<data_dir>\catalog.d\*.toml`。
    pub fn load_catalog(&self) -> Result<Catalog, CoreError> {
        Catalog::load(&self.paths.catalog_dir)
    }

    /// 要傳給子程序（背景 sync、排程）的設定檔：只有使用者明確指定時才有值（環境變數
    /// `CVEWATCH_CONFIG` 也算明確指定）。相對路徑轉為絕對路徑：排程的工作目錄是執行檔所在資料夾。
    pub fn explicit_config(&self) -> Result<Option<PathBuf>, CoreError> {
        let location = self.location();
        location
            .is_explicit()
            .then(|| {
                std::path::absolute(&location.path).map_err(|e| CoreError::io(&location.path, e))
            })
            .transpose()
    }
}

/// 未知欄位的警告文字。
pub fn unknown_key_warning(key: &str) -> String {
    format!("設定檔包含未知欄位 `{key}`（已忽略）")
}

/// 載入設定並啟動日誌。設定錯誤或無法決定資料目錄時失敗；日誌失敗只記為警告。
pub fn start(location: &ConfigLocation, command: &str) -> Result<Runtime, CoreError> {
    let loaded = config::load(location)?;
    let paths = DataPaths::resolve(loaded.config.data_dir.as_deref())?;
    let mut warnings = Vec::new();
    let log_guard = match start_logging(&paths, &loaded) {
        Ok(guard) => Some(guard),
        Err(e) => {
            warnings.push(format!("無法寫入日誌檔：{e}"));
            None
        }
    };
    tracing::info!(command, "cvewatch 啟動");
    if loaded.used_defaults {
        tracing::info!("使用預設設定");
    }
    for key in &loaded.unknown_keys {
        warnings.push(unknown_key_warning(key));
        tracing::warn!(key = %key, "設定檔包含未知欄位");
    }
    Ok(Runtime {
        loaded,
        paths,
        store: KeyringStore::default(),
        warnings,
        _log_guard: log_guard,
    })
}

/// 啟動日誌並清理過期日誌檔；清理失敗只寫入日誌。
fn start_logging(paths: &DataPaths, loaded: &LoadedConfig) -> Result<LogGuard, CoreError> {
    let guard = logging::init(&paths.log_dir, loaded.config.log.level)?;
    let today = chrono::Local::now().date_naive();
    match logging::cleanup_old_logs(&paths.log_dir, loaded.config.log.retention_days, today) {
        Ok(removed) if !removed.is_empty() => {
            tracing::info!(removed = removed.len(), "已刪除過期日誌檔");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "清理過期日誌失敗"),
    }
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::ConfigSource;

    /// 資料目錄下的 `logs` 是檔案：日誌目錄無法建立（不安裝全域 subscriber，測試互不影響）。
    fn setup(extra: &str) -> (tempfile::TempDir, ConfigLocation) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join(crate::paths::LOG_DIR_NAME), "not a dir").unwrap();
        let path = dir.path().join("cvewatch.toml");
        // 以 TOML 基本字串寫入（路徑含引號或反斜線時也正確跳脫）
        let data_dir = toml::Value::String(data.display().to_string());
        std::fs::write(&path, format!("data_dir = {data_dir}\n{extra}")).unwrap();
        let location = ConfigLocation {
            path,
            source: ConfigSource::CommandLine,
        };
        (dir, location)
    }

    #[test]
    fn unknown_keys_become_warnings() {
        let (_dir, location) = setup("x = 1\n");
        let rt = start(&location, "test").unwrap();
        assert!(
            rt.warnings
                .iter()
                .any(|w| w == "設定檔包含未知欄位 `x`（已忽略）"),
            "{:?}",
            rt.warnings
        );
    }

    #[test]
    fn explicit_config_only_when_user_chose_it() {
        let (_dir, location) = setup("");
        let rt = start(&location, "test").unwrap();
        assert_eq!(rt.explicit_config().unwrap(), Some(location.path.clone()));
        let default = ConfigLocation {
            source: ConfigSource::Default,
            ..location
        };
        let rt = start(&default, "test").unwrap();
        assert_eq!(rt.explicit_config().unwrap(), None);
    }

    #[test]
    fn unwritable_log_dir_is_a_warning_not_an_error() {
        let (_dir, location) = setup("");
        let rt = start(&location, "test").unwrap();
        assert_eq!(rt.warnings.len(), 1, "{:?}", rt.warnings);
        assert!(
            rt.warnings[0].starts_with("無法寫入日誌檔："),
            "{:?}",
            rt.warnings
        );
        assert!(rt.paths.data_dir.ends_with("data"));
    }
}
