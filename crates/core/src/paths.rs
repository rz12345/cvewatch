//! 設定檔與資料目錄的位置解析。

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::config::ConfigError;

pub const APP_DIR_NAME: &str = "cvewatch";
pub const CONFIG_FILE_NAME: &str = "cvewatch.toml";
pub const CONFIG_ENV_VAR: &str = "CVEWATCH_CONFIG";
pub const DB_FILE_NAME: &str = "cvewatch.db";
pub const LOG_DIR_NAME: &str = "logs";
pub const CACHE_DIR_NAME: &str = "cache";
pub const CVELIST_CACHE_DIR_NAME: &str = "cvelist";
pub const CATALOG_DIR_NAME: &str = "catalog.d";

/// 設定檔路徑的來源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    CommandLine,
    Environment,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLocation {
    pub path: PathBuf,
    pub source: ConfigSource,
}

impl ConfigLocation {
    /// 使用者是否明確指定路徑（`--config` 或環境變數）。
    pub fn is_explicit(&self) -> bool {
        self.source != ConfigSource::Default
    }
}

/// 依 `--config` → `CVEWATCH_CONFIG` → 使用者設定目錄 的順序決定設定檔路徑。
pub fn resolve_config_location(
    cli: Option<PathBuf>,
    env: Option<OsString>,
    default: Option<PathBuf>,
) -> Result<ConfigLocation, ConfigError> {
    if let Some(path) = cli {
        return Ok(ConfigLocation {
            path,
            source: ConfigSource::CommandLine,
        });
    }
    if let Some(value) = env.filter(|v| !v.is_empty()) {
        return Ok(ConfigLocation {
            path: PathBuf::from(value),
            source: ConfigSource::Environment,
        });
    }
    default
        .map(|path| ConfigLocation {
            path,
            source: ConfigSource::Default,
        })
        .ok_or(ConfigError::NoHomeDir)
}

/// `%APPDATA%\cvewatch\cvewatch.toml`
pub fn default_config_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.config_dir().join(APP_DIR_NAME).join(CONFIG_FILE_NAME))
}

/// 使用者「文件」資料夾（TUI 匯出 VEX 的預設位置）；取不到時依序為家目錄、目前目錄
/// （匯出對話框仍可修改路徑）。
pub fn documents_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|d| {
            d.document_dir()
                .map_or_else(|| d.home_dir().to_path_buf(), Path::to_path_buf)
        })
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

/// `%LOCALAPPDATA%\cvewatch`
pub fn default_data_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.data_local_dir().join(APP_DIR_NAME))
}

/// 資料目錄下的各檔案位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub log_dir: PathBuf,
    pub cvelist_cache_dir: PathBuf,
    /// 使用者產品目錄擴充檔（`*.toml`）
    pub catalog_dir: PathBuf,
}

impl DataPaths {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        DataPaths {
            db_path: data_dir.join(DB_FILE_NAME),
            log_dir: data_dir.join(LOG_DIR_NAME),
            cvelist_cache_dir: data_dir.join(CACHE_DIR_NAME).join(CVELIST_CACHE_DIR_NAME),
            catalog_dir: data_dir.join(CATALOG_DIR_NAME),
            data_dir,
        }
    }

    /// 由設定的 `data_dir`（若有）或預設位置建立。
    pub fn resolve(configured: Option<&Path>) -> Result<Self, ConfigError> {
        match configured {
            Some(dir) => Ok(DataPaths::new(dir)),
            None => default_data_dir()
                .map(DataPaths::new)
                .ok_or(ConfigError::NoHomeDir),
        }
    }

    pub fn is_in_cloud_sync_folder(&self) -> bool {
        path_looks_cloud_synced(&self.data_dir)
    }
}

/// 路徑是否位於 OneDrive 之類的雲端同步資料夾（SQLite WAL 檔不應被同步）。
pub fn path_looks_cloud_synced(path: &Path) -> bool {
    path.to_string_lossy().to_lowercase().contains("onedrive")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default() -> Option<PathBuf> {
        Some(PathBuf::from("C:/default/cvewatch.toml"))
    }

    #[test]
    fn cli_path_wins_over_env_and_default() {
        let loc = resolve_config_location(
            Some(PathBuf::from("D:/x/a.toml")),
            Some(OsString::from("E:/env.toml")),
            default(),
        )
        .unwrap();
        assert_eq!(loc.path, PathBuf::from("D:/x/a.toml"));
        assert_eq!(loc.source, ConfigSource::CommandLine);
        assert!(loc.is_explicit());
    }

    #[test]
    fn env_wins_over_default() {
        let loc =
            resolve_config_location(None, Some(OsString::from("E:/env.toml")), default()).unwrap();
        assert_eq!(loc.path, PathBuf::from("E:/env.toml"));
        assert_eq!(loc.source, ConfigSource::Environment);
    }

    #[test]
    fn empty_env_is_ignored() {
        let loc = resolve_config_location(None, Some(OsString::new()), default()).unwrap();
        assert_eq!(loc.source, ConfigSource::Default);
        assert!(!loc.is_explicit());
    }

    #[test]
    fn missing_home_dir_is_an_error() {
        assert!(matches!(
            resolve_config_location(None, None, None),
            Err(ConfigError::NoHomeDir)
        ));
    }

    #[test]
    fn data_paths_layout() {
        let p = DataPaths::new("D:/data");
        assert_eq!(p.db_path, PathBuf::from("D:/data").join("cvewatch.db"));
        assert_eq!(p.log_dir, PathBuf::from("D:/data").join("logs"));
        assert_eq!(
            p.cvelist_cache_dir,
            PathBuf::from("D:/data").join("cache").join("cvelist")
        );
    }

    #[test]
    fn onedrive_detection_is_case_insensitive() {
        assert!(path_looks_cloud_synced(Path::new(
            r"C:\Users\me\OneDrive - Org\cvewatch"
        )));
        assert!(path_looks_cloud_synced(Path::new("c:/users/me/onedrive/x")));
        assert!(!path_looks_cloud_synced(Path::new(r"D:\cvewatch-data")));
    }
}
