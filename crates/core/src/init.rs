//! `cvewatch init`：建立目錄、範例設定檔、加密資料庫與金鑰。重跑不覆蓋既有檔案。

use std::io::Write;
use std::path::PathBuf;

use crate::config::example_config;
use crate::db::keyops::{ensure_database, key_exported_at};
use crate::db::open_working_db;
use crate::error::CoreError;
use crate::paths::{ConfigLocation, DataPaths};
use crate::secrets::SecretStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitReport {
    pub config_path: PathBuf,
    pub config_created: bool,
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub log_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub db_created: bool,
    pub key_created: bool,
    pub cloud_sync_warning: bool,
    pub recovery_key_exported: bool,
}

/// 設定檔不存在時寫出範例；回傳是否新建。既有檔案絕不覆蓋。
pub fn ensure_config_file(location: &ConfigLocation) -> Result<bool, CoreError> {
    let path = &location.path;
    if path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
    }
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => return Err(CoreError::io(path, e)),
    };
    file.write_all(example_config().as_bytes())
        .map_err(|e| CoreError::io(path, e))?;
    Ok(true)
}

/// 在設定檔已就緒（見 [`ensure_config_file`]）後建立資料目錄與資料庫。
pub fn run(
    location: &ConfigLocation,
    paths: &DataPaths,
    store: &dyn SecretStore,
    config_created: bool,
) -> Result<InitReport, CoreError> {
    for dir in [
        &paths.data_dir,
        &paths.log_dir,
        &paths.cvelist_cache_dir,
        &paths.catalog_dir,
    ] {
        std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
    }
    let cloud_sync_warning = paths.is_in_cloud_sync_folder();
    if cloud_sync_warning {
        tracing::warn!("資料目錄位於雲端同步資料夾內");
    }
    let setup = ensure_database(paths, store)?;
    let conn = open_working_db(paths, store)?;
    let recovery_key_exported = key_exported_at(&conn)?.is_some();
    Ok(InitReport {
        config_path: location.path.clone(),
        config_created,
        data_dir: paths.data_dir.clone(),
        db_path: paths.db_path.clone(),
        log_dir: paths.log_dir.clone(),
        cache_dir: paths.cvelist_cache_dir.clone(),
        db_created: setup.db_created,
        key_created: setup.key_created,
        cloud_sync_warning,
        recovery_key_exported,
    })
}
