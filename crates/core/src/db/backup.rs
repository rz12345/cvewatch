//! 線上備份（`sqlcipher_export`）與還原。

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use zeroize::Zeroizing;

use super::{
    is_wrong_key, load_db_key, open_encrypted, open_keyed, open_working_db, rw_create_flags,
    Cipher, DbKey,
};
use crate::error::CoreError;
use crate::paths::DataPaths;
use crate::secrets::SecretStore;

const PLAIN_SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
const ATTACH_ALIAS: &str = "cw_export";
pub const BEFORE_RESTORE_SUFFIX: &str = ".before-restore";
const RESTORE_TMP_SUFFIX: &str = ".restore-tmp";
const SOURCE_COPY_SUFFIX: &str = ".source";
const SIDE_FILE_SUFFIXES: [&str; 2] = ["-wal", "-shm"];

/// 備份檔的加密方式。
pub enum BackupCipher {
    /// 預設：與資料庫相同的金鑰。
    DatabaseKey,
    /// 可攜備份：以口令加密，與本機憑證庫無關。
    Passphrase(Zeroizing<String>),
    /// 明文；呼叫端須先取得使用者 `PLAIN` 確認。
    Plain,
}

/// 產出一致的備份副本；目的檔已存在時拒絕。失敗時刪除不完整的檔案。
pub fn backup(
    paths: &DataPaths,
    store: &dyn SecretStore,
    dest: &Path,
    cipher: &BackupCipher,
) -> Result<(), CoreError> {
    if dest.exists() {
        return Err(CoreError::Invalid(format!(
            "目的檔已存在，不覆蓋：{}",
            dest.display()
        )));
    }
    let conn = open_working_db(paths, store)?;
    let key = load_db_key(store)?;
    let target = match cipher {
        BackupCipher::DatabaseKey => Cipher::Key(&key),
        BackupCipher::Passphrase(p) => Cipher::Passphrase(p.as_str()),
        BackupCipher::Plain => Cipher::Plain,
    };
    let result = export_to(&conn, dest, &target);
    if result.is_err() {
        remove_quietly(dest);
    }
    result?;
    tracing::info!(
        encrypted = !matches!(cipher, BackupCipher::Plain),
        "資料庫備份完成"
    );
    Ok(())
}

/// 以 `ATTACH ... KEY` + `sqlcipher_export` 將 `conn` 的 main 匯出到新檔。
fn export_to(conn: &Connection, dest: &Path, cipher: &Cipher<'_>) -> Result<(), CoreError> {
    let dest_str = dest
        .to_str()
        .ok_or_else(|| CoreError::Invalid("備份路徑含無法處理的字元".to_string()))?;
    conn.execute(
        &format!("ATTACH DATABASE ?1 AS {ATTACH_ALIAS} KEY ?2"),
        params![dest_str, cipher.key_string().as_str()],
    )?;
    // 在單一讀取交易內匯出，與並行的 sync 寫入隔離，取得一致的快照。
    let exported = conn.execute_batch("BEGIN").and_then(|()| {
        let result = conn.query_row(
            &format!("SELECT sqlcipher_export('{ATTACH_ALIAS}')"),
            [],
            |_| Ok(()),
        );
        let end = if result.is_ok() { "COMMIT" } else { "ROLLBACK" };
        let ended = conn.execute_batch(end);
        result.and(ended)
    });
    let detached = conn.execute(&format!("DETACH DATABASE {ATTACH_ALIAS}"), []);
    exported?;
    detached?;
    Ok(())
}

/// 還原時用來解開備份的金鑰（明文備份依檔頭自動判斷，不需指定）。
pub enum RestoreKey<'a> {
    /// 本機憑證庫中的資料庫金鑰（同一台電腦、未 rekey 過的備份）。
    LocalKey,
    /// 可攜備份的口令（`--passphrase`）。
    Passphrase(&'a str),
    /// 以舊資料庫金鑰加密的備份：輸入當時的復原金鑰（`--recovery-key`）。
    RecoveryKey(&'a DbKey),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreOutcome {
    /// 原資料庫保留的位置（原本沒有資料庫時為 `None`）。
    pub previous_saved_as: Option<PathBuf>,
}

/// 驗證備份可開啟後，以本機資料庫金鑰重新加密並取代現有資料庫。
///
/// 備份的加密方式：明文（依檔頭自動判斷）或 [`RestoreKey`] 指定的金鑰。
/// 取代前會先完整驗證（含 schema 版本與 migration）；任何失敗都不動現有資料庫。
pub fn restore(
    paths: &DataPaths,
    store: &dyn SecretStore,
    src: &Path,
    key: &RestoreKey<'_>,
) -> Result<RestoreOutcome, CoreError> {
    if !src.is_file() {
        return Err(CoreError::Invalid(format!(
            "備份檔不存在：{}",
            src.display()
        )));
    }
    let local_key = load_db_key(store)?;
    let tmp = suffixed(&paths.db_path, RESTORE_TMP_SUFFIX);
    remove_with_side_files(&tmp)?;

    if let Err(err) = prepare_restore(src, &tmp, key, &local_key) {
        discard(&tmp);
        return Err(err);
    }
    let previous_saved_as = swap_in(paths, &tmp, &local_key).inspect_err(|_| discard(&tmp))?;
    open_encrypted(&paths.db_path, &local_key, false)?;
    tracing::info!("資料庫已自備份還原");
    Ok(RestoreOutcome { previous_saved_as })
}

/// 複製備份 → 以本機金鑰匯出到暫存檔 → 以正式流程開啟暫存檔（含版本檢查與 migration）。
fn prepare_restore(
    src: &Path,
    tmp: &Path,
    key: &RestoreKey<'_>,
    local_key: &DbKey,
) -> Result<(), CoreError> {
    // 只讀取備份的副本，避免 SQLite 對備份檔本身回放或寫入旁邊的 journal/WAL。
    let src_copy = suffixed(tmp, SOURCE_COPY_SUFFIX);
    std::fs::copy(src, &src_copy).map_err(|e| CoreError::io(src, e))?;
    let result = reencrypt_and_verify(&src_copy, tmp, key, local_key);
    let cleanup = remove_with_side_files(&src_copy);
    result?;
    cleanup
}

fn reencrypt_and_verify(
    src_copy: &Path,
    tmp: &Path,
    key: &RestoreKey<'_>,
    local_key: &DbKey,
) -> Result<(), CoreError> {
    let source_cipher = if is_plain_sqlite(src_copy)? {
        Cipher::Plain
    } else {
        match key {
            RestoreKey::LocalKey => Cipher::Key(local_key),
            RestoreKey::Passphrase(p) => Cipher::Passphrase(p),
            RestoreKey::RecoveryKey(k) => Cipher::Key(k),
        }
    };
    reencrypt_backup(src_copy, tmp, &source_cipher, &Cipher::Key(local_key))?;
    // 版本過新或 migration 失敗會在此回報，現有資料庫尚未被動到。
    drop(open_encrypted(tmp, local_key, false)?);
    ensure_no_side_files(tmp)
}

/// 開啟備份並驗證是 cvewatch 資料庫，再以本機金鑰匯出到暫存檔。
fn reencrypt_backup(
    src: &Path,
    tmp: &Path,
    source: &Cipher<'_>,
    target: &Cipher<'_>,
) -> Result<(), CoreError> {
    // 需要建立旗標才能 ATTACH 新檔；src 是呼叫端建立的副本。
    let conn = open_keyed(src, source, rw_create_flags()).map_err(|e| {
        if is_wrong_key(&e) {
            CoreError::BackupKeyInvalid
        } else {
            CoreError::Sqlite(e)
        }
    })?;
    let version = super::migrate::current_version(&conn)?;
    if version == 0 {
        return Err(CoreError::Invalid(
            "備份檔不是 cvewatch 資料庫（缺少 schema_version）".to_string(),
        ));
    }
    let supported = super::migrate::latest_version();
    if version > supported {
        return Err(CoreError::DbTooNew {
            found: version,
            supported,
        });
    }
    export_to(&conn, tmp, target)
}

/// 將現有資料庫移到一旁，再把暫存檔改名為正式資料庫；第二步失敗時移回原處。
fn swap_in(paths: &DataPaths, tmp: &Path, key: &DbKey) -> Result<Option<PathBuf>, CoreError> {
    let saved = set_aside_current(paths, key)?;
    if let Err(e) = std::fs::rename(tmp, &paths.db_path) {
        if let Some(saved) = &saved {
            move_back(saved, &paths.db_path);
        }
        return Err(CoreError::io(&paths.db_path, e));
    }
    Ok(saved)
}

/// 將現有資料庫（WAL 先合併）改名為 `cvewatch.db.before-restore`；
/// 已有同名檔時加上時間戳記，不覆蓋先前保留的副本。
fn set_aside_current(paths: &DataPaths, key: &DbKey) -> Result<Option<PathBuf>, CoreError> {
    let db = &paths.db_path;
    if !db.exists() {
        return Ok(None);
    }
    let checkpointed = open_encrypted(db, key, false)
        .and_then(|conn| {
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
            Ok(())
        })
        .is_ok();
    let saved = unique_before_restore_path(db);
    // 其他程序開著資料庫時，Windows 上的改名會失敗，此時尚未變更任何檔案。
    std::fs::rename(db, &saved).map_err(|e| CoreError::io(db, e))?;
    if let Err(err) = move_side_files(db, &saved, checkpointed) {
        move_back(&saved, db);
        return Err(err);
    }
    Ok(Some(saved))
}

/// 已合併的 WAL/SHM 直接刪除；未能合併時跟著主檔一起保留。
/// 失敗即中止，以免舊 WAL 被套用到還原後的資料庫。
fn move_side_files(db: &Path, saved: &Path, checkpointed: bool) -> Result<(), CoreError> {
    for side in SIDE_FILE_SUFFIXES {
        let file = suffixed(db, side);
        if !file.exists() {
            continue;
        }
        if checkpointed {
            std::fs::remove_file(&file).map_err(|e| CoreError::io(&file, e))?;
        } else {
            let keep = suffixed(saved, side);
            std::fs::rename(&file, &keep).map_err(|e| CoreError::io(&file, e))?;
        }
    }
    Ok(())
}

fn move_back(saved: &Path, db: &Path) {
    if let Err(e) = std::fs::rename(saved, db) {
        tracing::error!(error_kind = ?e.kind(), "無法將原資料庫移回");
    }
}

/// `cvewatch.db.before-restore`，若已被占用則加上時間戳記（與序號）。
fn unique_before_restore_path(db: &Path) -> PathBuf {
    let taken = |p: &Path| p.exists() || SIDE_FILE_SUFFIXES.iter().any(|s| suffixed(p, s).exists());
    let plain = suffixed(db, BEFORE_RESTORE_SUFFIX);
    if !taken(&plain) {
        return plain;
    }
    let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
    (0u32..)
        .map(|n| match n {
            0 => suffixed(db, &format!("{BEFORE_RESTORE_SUFFIX}-{stamp}")),
            n => suffixed(db, &format!("{BEFORE_RESTORE_SUFFIX}-{stamp}-{n}")),
        })
        .find(|p| !taken(p))
        .expect("序號空間不會用盡")
}

fn ensure_no_side_files(path: &Path) -> Result<(), CoreError> {
    let leftover = SIDE_FILE_SUFFIXES
        .iter()
        .map(|s| suffixed(path, s))
        .find(|p| p.exists());
    match leftover {
        Some(p) => Err(CoreError::Invalid(format!(
            "暫存檔仍有未合併的 {}，中止還原",
            p.display()
        ))),
        None => Ok(()),
    }
}

/// 刪除檔案及其 `-wal`、`-shm`；不存在視為成功。
fn remove_with_side_files(path: &Path) -> Result<(), CoreError> {
    let all = std::iter::once(path.to_path_buf())
        .chain(SIDE_FILE_SUFFIXES.iter().map(|s| suffixed(path, s)));
    for p in all {
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(CoreError::io(&p, e)),
        }
    }
    Ok(())
}

/// 清理失敗流程留下的暫存檔；清不掉只記錄，不覆蓋原本的錯誤。
fn discard(path: &Path) {
    if let Err(e) = remove_with_side_files(path) {
        tracing::warn!(error = %e, "無法刪除暫存檔");
    }
}

fn is_plain_sqlite(path: &Path) -> Result<bool, CoreError> {
    use std::io::Read;
    let mut header = [0u8; 16];
    let mut file = std::fs::File::open(path).map_err(|e| CoreError::io(path, e))?;
    match file.read_exact(&mut header) {
        Ok(()) => Ok(&header == PLAIN_SQLITE_HEADER),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(CoreError::io(path, e)),
    }
}

pub(crate) fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn remove_quietly(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(error_kind = ?e.kind(), "無法刪除暫存檔");
        }
    }
}
