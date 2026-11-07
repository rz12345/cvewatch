//! 資料庫金鑰生命週期：建立、匯出/匯入復原金鑰、更換金鑰。

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use zeroize::Zeroizing;

use super::{load_db_key, meta, open_encrypted, open_working_db, DbKey};
use crate::error::CoreError;
use crate::paths::DataPaths;
use crate::secrets::{SecretKey, SecretStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbSetup {
    pub db_created: bool,
    pub key_created: bool,
}

/// `init` 使用：必要時產生金鑰並建立資料庫；既有資料庫只做驗證與 migration。
pub fn ensure_database(paths: &DataPaths, store: &dyn SecretStore) -> Result<DbSetup, CoreError> {
    let existing = match store.get(SecretKey::DbKey)? {
        Some(hex) => Some(DbKey::from_hex(&hex)?),
        None => None,
    };
    let db_exists = paths.db_path.exists();
    match (db_exists, existing) {
        (true, Some(key)) => {
            open_encrypted(&paths.db_path, &key, false)?;
            Ok(DbSetup {
                db_created: false,
                key_created: false,
            })
        }
        // 資料庫在但金鑰不在：絕不覆蓋，請使用者匯入復原金鑰。
        (true, None) => Err(CoreError::DbKeyMissing),
        (false, Some(key)) => {
            open_encrypted(&paths.db_path, &key, true)?;
            Ok(DbSetup {
                db_created: true,
                key_created: false,
            })
        }
        (false, None) => {
            let key = DbKey::generate()?;
            store.set(SecretKey::DbKey, &key.to_hex())?;
            open_encrypted(&paths.db_path, &key, true)?;
            tracing::info!("已建立新的資料庫金鑰與加密資料庫");
            Ok(DbSetup {
                db_created: true,
                key_created: true,
            })
        }
    }
}

/// 最後一次匯出復原金鑰的時間；從未匯出為 `None`。
pub fn key_exported_at(conn: &Connection) -> Result<Option<String>, CoreError> {
    meta::get(conn, meta::KEY_DB_KEY_EXPORTED_AT)
}

/// 回傳復原金鑰字串並記錄匯出時間。不寫入任何檔案。
pub fn export_recovery_key(
    paths: &DataPaths,
    store: &dyn SecretStore,
    now: DateTime<Utc>,
) -> Result<Zeroizing<String>, CoreError> {
    let conn = open_working_db(paths, store)?;
    let key = load_db_key(store)?;
    meta::set(&conn, meta::KEY_DB_KEY_EXPORTED_AT, &now.to_rfc3339())?;
    tracing::info!("已匯出資料庫復原金鑰");
    Ok(key.to_recovery_string())
}

/// 只有在復原金鑰能開啟資料庫時才寫入憑證庫；否則憑證庫維持原狀。
pub fn import_recovery_key(
    paths: &DataPaths,
    store: &dyn SecretStore,
    recovery: &str,
) -> Result<(), CoreError> {
    let key = DbKey::from_recovery_string(recovery)?;
    if !paths.db_path.exists() {
        return Err(CoreError::DbNotFound(paths.db_path.clone()));
    }
    let conn = open_encrypted(&paths.db_path, &key, false)?;
    // 匯入的金鑰就是使用者手上的復原金鑰，視為已匯出。
    meta::set(
        &conn,
        meta::KEY_DB_KEY_EXPORTED_AT,
        &Utc::now().to_rfc3339(),
    )?;
    drop(conn);
    store.set(SecretKey::DbKey, &key.to_hex())?;
    tracing::info!("已匯入資料庫復原金鑰");
    Ok(())
}

/// 以新金鑰重新加密資料庫，更新憑證庫，並將復原金鑰標為未匯出。
///
/// 任何時間點中斷，憑證庫都至少保有一把能開啟資料庫的金鑰：
/// 1. 新金鑰先存入 `db_key_pending`
/// 2. `PRAGMA rekey`
/// 3. 以新連線驗證新金鑰
/// 4. 新金鑰寫入 `db_key`，刪除 `db_key_pending`
///
/// 2 之後中斷時，[`super::open_working_db`] 會以 `db_key_pending` 完成更換。
pub fn rekey(paths: &DataPaths, store: &dyn SecretStore) -> Result<(), CoreError> {
    let old = load_db_key(store)?;
    let new = DbKey::generate()?;
    store.set(SecretKey::DbKeyPending, &new.to_hex())?;
    if let Err(err) = apply_rekey(paths, &old, &new) {
        abandon_pending_if_unused(paths, store, &old);
        return Err(err);
    }
    let conn = open_encrypted(&paths.db_path, &new, false)?;
    meta::delete(&conn, meta::KEY_DB_KEY_EXPORTED_AT)?;
    store.set(SecretKey::DbKey, &new.to_hex())?;
    if let Err(e) = store.delete(SecretKey::DbKeyPending) {
        tracing::warn!(error = %e, "無法刪除暫存的新金鑰");
    }
    tracing::info!("資料庫金鑰已更換");
    Ok(())
}

fn apply_rekey(paths: &DataPaths, old: &DbKey, new: &DbKey) -> Result<(), CoreError> {
    let conn = open_encrypted(&paths.db_path, old, false)?;
    set_journal_mode(&conn, "delete")?;
    super::set_key_pragma(&conn, "rekey", new.pragma_value().as_str())?;
    set_journal_mode(&conn, "wal")
}

/// rekey 失敗時：若資料庫仍能以舊金鑰開啟，暫存的新金鑰沒有用處，予以刪除；
/// 否則保留，讓下次開啟時恢復。
fn abandon_pending_if_unused(paths: &DataPaths, store: &dyn SecretStore, old: &DbKey) {
    if open_encrypted(&paths.db_path, old, false).is_ok() {
        if let Err(e) = store.delete(SecretKey::DbKeyPending) {
            tracing::warn!(error = %e, "無法刪除暫存的新金鑰");
        }
    } else {
        tracing::warn!("金鑰更換中斷，保留暫存的新金鑰供下次開啟時恢復");
    }
}

fn set_journal_mode(conn: &Connection, mode: &str) -> Result<(), CoreError> {
    let actual: String =
        conn.query_row(&format!("PRAGMA journal_mode = {mode}"), [], |r| r.get(0))?;
    if !actual.eq_ignore_ascii_case(mode) {
        return Err(CoreError::Invalid(
            "無法切換 journal 模式；請先關閉其他正在使用資料庫的 cvewatch（TUI 或 sync）"
                .to_string(),
        ));
    }
    Ok(())
}
