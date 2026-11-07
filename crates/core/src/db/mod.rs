//! SQLCipher 加密資料庫：連線工廠、migration、sync 鎖、金鑰管理與備份。
//!
//! 每條連線的第一個指令都是 `PRAGMA key`，驗證金鑰後才設定 WAL 等參數。

pub mod backup;
pub mod key;
pub mod keyops;
pub mod lock;
pub mod meta;
pub mod migrate;

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OpenFlags};

use crate::error::CoreError;
use crate::paths::DataPaths;
use crate::secrets::{SecretKey, SecretStore};

pub use key::DbKey;

pub const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

/// 開啟資料庫用的金鑰種類。
pub enum Cipher<'a> {
    /// 工作資料庫與預設備份：raw key（不經 KDF）。
    Key(&'a DbKey),
    /// 可攜備份：口令經 SQLCipher 內建 PBKDF2。
    Passphrase(&'a str),
    /// 明文（只用於 `--plain` 備份）。
    Plain,
}

impl Cipher<'_> {
    /// `PRAGMA key` / `ATTACH ... KEY` 使用的字串；明文為空字串。
    fn key_string(&self) -> zeroize::Zeroizing<String> {
        match self {
            Cipher::Key(k) => k.pragma_value(),
            Cipher::Passphrase(p) => zeroize::Zeroizing::new((*p).to_string()),
            Cipher::Plain => zeroize::Zeroizing::new(String::new()),
        }
    }
}

pub(crate) fn is_wrong_key(err: &rusqlite::Error) -> bool {
    err.sqlite_error_code() == Some(ErrorCode::NotADatabase)
}

/// 執行 `PRAGMA key` / `PRAGMA rekey`。rusqlite 的 `SqlInputError` 會在訊息中帶出 SQL 原文
/// （含金鑰），因此一律換成不含 SQL 的錯誤。
pub(crate) fn set_key_pragma(
    conn: &Connection,
    pragma: &str,
    value: &str,
) -> Result<(), rusqlite::Error> {
    conn.pragma_update(None, pragma, value)
        .map_err(|e| match e {
            rusqlite::Error::SqlInputError { error, .. } => {
                rusqlite::Error::SqliteFailure(error, Some(format!("PRAGMA {pragma} 失敗")))
            }
            other => other,
        })
}

/// 開啟並以金鑰驗證，不設定 WAL、不執行 migration。金鑰錯誤時檔案不會被修改。
pub(crate) fn open_keyed(
    path: &Path,
    cipher: &Cipher<'_>,
    flags: OpenFlags,
) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open_with_flags(path, flags)?;
    if !matches!(cipher, Cipher::Plain) {
        set_key_pragma(&conn, "key", cipher.key_string().as_str())?;
    }
    // 金鑰錯誤由本程式回報；關閉 SQLCipher 自己輸出到 stderr 的訊息。不存取資料庫內容。
    conn.pragma_update(None, "cipher_log_level", "NONE")?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })?;
    Ok(conn)
}

fn ensure_sqlcipher(conn: &Connection) -> Result<(), CoreError> {
    let version: Option<String> = conn
        .query_row("PRAGMA cipher_version", [], |r| r.get(0))
        .ok();
    match version {
        Some(v) if !v.is_empty() => Ok(()),
        _ => Err(CoreError::Invalid(
            "此執行檔未以 SQLCipher 建置，拒絕開啟未加密的資料庫".to_string(),
        )),
    }
}

/// 開啟加密的工作資料庫：驗證金鑰 → WAL、foreign_keys、busy_timeout → migration。
pub fn open_encrypted(path: &Path, key: &DbKey, create: bool) -> Result<Connection, CoreError> {
    if !create && !path.exists() {
        return Err(CoreError::DbNotFound(path.to_path_buf()));
    }
    // 存在與否已在上方檢查；仍帶 CREATE 是因為 ATTACH（備份）沿用主連線旗標，需要能建立新檔。
    let flags = rw_create_flags();
    let mut conn = open_keyed(path, &Cipher::Key(key), flags).map_err(|e| {
        if is_wrong_key(&e) {
            CoreError::DbKeyInvalid
        } else {
            CoreError::Sqlite(e)
        }
    })?;
    ensure_sqlcipher(&conn)?;
    configure(&conn)?;
    migrate::migrate(&mut conn)?;
    Ok(conn)
}

pub(crate) fn rw_create_flags() -> OpenFlags {
    OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
}

fn configure(conn: &Connection) -> Result<(), CoreError> {
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(CoreError::Invalid(format!(
            "無法啟用 WAL 模式（目前為 {mode}）"
        )));
    }
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    Ok(())
}

/// 從憑證庫讀取資料庫金鑰；不存在時為 [`CoreError::DbKeyMissing`]（結束碼 5）。
pub fn load_db_key(store: &dyn SecretStore) -> Result<DbKey, CoreError> {
    let hex = store
        .get(SecretKey::DbKey)?
        .ok_or(CoreError::DbKeyMissing)?;
    DbKey::from_hex(&hex)
}

/// 一般命令使用：資料庫必須已存在，金鑰取自憑證庫。
///
/// 若金鑰錯誤但憑證庫留有 `db_key_pending`（`db rekey` 中斷），改用它開啟並完成更換。
pub fn open_working_db(
    paths: &DataPaths,
    store: &dyn SecretStore,
) -> Result<Connection, CoreError> {
    if !paths.db_path.exists() {
        return Err(CoreError::DbNotFound(paths.db_path.clone()));
    }
    let key = load_db_key(store)?;
    match open_encrypted(&paths.db_path, &key, false) {
        Err(CoreError::DbKeyInvalid) => recover_pending_key(paths, store),
        other => other,
    }
}

fn recover_pending_key(
    paths: &DataPaths,
    store: &dyn SecretStore,
) -> Result<Connection, CoreError> {
    let Some(hex) = store.get(SecretKey::DbKeyPending)? else {
        return Err(CoreError::DbKeyInvalid);
    };
    let pending = DbKey::from_hex(&hex)?;
    let conn = open_encrypted(&paths.db_path, &pending, false)?;
    meta::delete(&conn, meta::KEY_DB_KEY_EXPORTED_AT)?;
    store.set(SecretKey::DbKey, &pending.to_hex())?;
    store.delete(SecretKey::DbKeyPending)?;
    tracing::warn!("偵測到中斷的金鑰更換，已改用新金鑰並更新憑證庫");
    Ok(conn)
}
