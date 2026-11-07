//! 6.7：加密、金鑰錯誤、rekey、三種備份與還原往返。

mod common;

use std::path::Path;

use chrono::Utc;
use cvewatch_core::db::backup::{backup, restore, BackupCipher, RestoreKey};
use cvewatch_core::db::keyops::{
    ensure_database, export_recovery_key, import_recovery_key, key_exported_at, rekey,
};
use cvewatch_core::db::{load_db_key, open_encrypted, open_working_db, DbKey};
use cvewatch_core::secrets::{SecretKey, SecretStore};
use cvewatch_core::{CoreError, ExitCode};
use rusqlite::Connection;
use zeroize::Zeroizing;

const ASSET_TAG: &str = "ASSET-TAG-0042";
const CVE_ID: &str = "CVE-2024-99999";
const PASSPHRASE: &str = "correct horse battery staple";

fn insert_marker(env: &common::Env) {
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    conn.execute(
        "INSERT INTO sync_run (started_at, source, status, error) VALUES ('t', ?1, 'failed', ?2)",
        [ASSET_TAG, CVE_ID],
    )
    .unwrap();
}

fn marker_present(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT count(*) FROM sync_run WHERE source = ?1 AND error = ?2",
        [ASSET_TAG, CVE_ID],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        == 1
}

fn opens_without_key(path: &Path) -> bool {
    let conn = Connection::open(path).unwrap();
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .is_ok()
}

#[test]
fn database_cannot_be_read_without_key() {
    let env = common::initialized();
    insert_marker(&env);
    assert!(!opens_without_key(&env.paths.db_path));
}

#[test]
fn plaintext_markers_never_appear_in_files() {
    let env = common::initialized();
    insert_marker(&env);
    // 連線仍開啟時檢查 WAL，關閉後再檢查主檔。
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    assert!(!common::files_contain(&env.paths.db_path, ASSET_TAG));
    assert!(!common::files_contain(&env.paths.db_path, CVE_ID));
    drop(conn);
    assert!(!common::files_contain(&env.paths.db_path, ASSET_TAG));
}

#[test]
fn wrong_key_fails_with_exit_5_and_leaves_file_untouched() {
    let env = common::initialized();
    insert_marker(&env);
    let before = std::fs::read(&env.paths.db_path).unwrap();
    let wrong = DbKey::generate().unwrap();
    let err = open_encrypted(&env.paths.db_path, &wrong, false).unwrap_err();
    assert!(matches!(err, CoreError::DbKeyInvalid), "{err}");
    assert_eq!(err.exit_code(), ExitCode::DbKeyError);
    assert_eq!(std::fs::read(&env.paths.db_path).unwrap(), before);
}

#[test]
fn missing_key_fails_with_exit_5() {
    let env = common::initialized();
    env.store.delete(SecretKey::DbKey).unwrap();
    let err = open_working_db(&env.paths, &env.store).unwrap_err();
    assert!(matches!(err, CoreError::DbKeyMissing));
    assert_eq!(err.exit_code(), ExitCode::DbKeyError);
    // init 也不得在金鑰遺失時覆蓋既有資料庫。
    assert!(matches!(
        ensure_database(&env.paths, &env.store),
        Err(CoreError::DbKeyMissing)
    ));
}

#[test]
fn ensure_database_is_idempotent() {
    let env = common::empty();
    let first = ensure_database(&env.paths, &env.store).unwrap();
    assert!(first.db_created && first.key_created);
    let key_hex = env.store.get(SecretKey::DbKey).unwrap().unwrap();
    insert_marker(&env);
    let second = ensure_database(&env.paths, &env.store).unwrap();
    assert!(!second.db_created && !second.key_created);
    assert_eq!(env.store.get(SecretKey::DbKey).unwrap().unwrap(), key_hex);
    assert!(marker_present(
        &open_working_db(&env.paths, &env.store).unwrap()
    ));
}

#[test]
fn export_records_timestamp_and_import_restores_access() {
    let env = common::initialized();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    assert!(key_exported_at(&conn).unwrap().is_none());
    drop(conn);

    let recovery = export_recovery_key(&env.paths, &env.store, Utc::now()).unwrap();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    assert!(key_exported_at(&conn).unwrap().is_some());
    drop(conn);

    // 模擬 Windows 重灌：憑證庫遺失。
    env.store.delete(SecretKey::DbKey).unwrap();
    import_recovery_key(&env.paths, &env.store, &recovery).unwrap();
    assert!(open_working_db(&env.paths, &env.store).is_ok());
}

#[test]
fn importing_wrong_key_keeps_store_unchanged() {
    let env = common::initialized();
    let original = env.store.get(SecretKey::DbKey).unwrap().unwrap();
    let wrong = DbKey::generate().unwrap().to_recovery_string();
    let err = import_recovery_key(&env.paths, &env.store, &wrong).unwrap_err();
    assert!(matches!(err, CoreError::DbKeyInvalid), "{err}");
    assert_eq!(env.store.get(SecretKey::DbKey).unwrap().unwrap(), original);
}

#[test]
fn rekey_invalidates_old_key_and_clears_export_flag() {
    let env = common::initialized();
    insert_marker(&env);
    export_recovery_key(&env.paths, &env.store, Utc::now()).unwrap();
    let old = load_db_key(&env.store).unwrap();

    rekey(&env.paths, &env.store).unwrap();

    let new = load_db_key(&env.store).unwrap();
    assert_ne!(old, new);
    assert!(matches!(
        open_encrypted(&env.paths.db_path, &old, false),
        Err(CoreError::DbKeyInvalid)
    ));
    let conn = open_encrypted(&env.paths.db_path, &new, false).unwrap();
    assert!(marker_present(&conn));
    assert!(key_exported_at(&conn).unwrap().is_none());
}

#[test]
fn backup_with_database_key_roundtrips() {
    let env = common::initialized();
    insert_marker(&env);
    let dest = env.dir.path().join("bak.db");
    backup(&env.paths, &env.store, &dest, &BackupCipher::DatabaseKey).unwrap();

    assert!(!opens_without_key(&dest));
    assert!(!common::files_contain(&dest, ASSET_TAG));
    let key = load_db_key(&env.store).unwrap();
    assert!(marker_present(&open_encrypted(&dest, &key, false).unwrap()));

    // 還原會先保留現有資料庫。
    let outcome = restore(&env.paths, &env.store, &dest, &RestoreKey::LocalKey).unwrap();
    let saved = outcome.previous_saved_as.unwrap();
    assert!(saved.ends_with("cvewatch.db.before-restore"));
    assert!(saved.exists());
    assert!(marker_present(
        &open_working_db(&env.paths, &env.store).unwrap()
    ));
}

#[test]
fn passphrase_backup_restores_on_new_laptop() {
    let env = common::initialized();
    insert_marker(&env);
    let dest = env.dir.path().join("portable.db");
    let cipher = BackupCipher::Passphrase(Zeroizing::new(PASSPHRASE.to_string()));
    backup(&env.paths, &env.store, &dest, &cipher).unwrap();

    // 只能以口令開啟，與本機金鑰無關。
    let key = load_db_key(&env.store).unwrap();
    assert!(open_encrypted(&dest, &key, false).is_err());

    let laptop = common::initialized();
    let err = restore(
        &laptop.paths,
        &laptop.store,
        &dest,
        &RestoreKey::Passphrase("wrong"),
    )
    .unwrap_err();
    assert!(matches!(err, CoreError::BackupKeyInvalid), "{err}");
    assert_eq!(err.exit_code(), ExitCode::DbKeyError);

    let outcome = restore(
        &laptop.paths,
        &laptop.store,
        &dest,
        &RestoreKey::Passphrase(PASSPHRASE),
    )
    .unwrap();
    assert!(outcome.previous_saved_as.is_some());
    let conn = open_working_db(&laptop.paths, &laptop.store).unwrap();
    assert!(marker_present(&conn), "資料以新電腦的金鑰重新加密");
    assert!(!opens_without_key(&laptop.paths.db_path));
}

#[test]
fn plain_backup_roundtrips() {
    let env = common::initialized();
    insert_marker(&env);
    let dest = env.dir.path().join("out.plain.db");
    backup(&env.paths, &env.store, &dest, &BackupCipher::Plain).unwrap();
    assert!(opens_without_key(&dest));

    let other = common::initialized();
    restore(&other.paths, &other.store, &dest, &RestoreKey::LocalKey).unwrap();
    assert!(marker_present(
        &open_working_db(&other.paths, &other.store).unwrap()
    ));
    assert!(!opens_without_key(&other.paths.db_path), "還原後重新加密");
}

#[test]
fn backup_refuses_to_overwrite() {
    let env = common::initialized();
    let dest = env.dir.path().join("exists.db");
    std::fs::write(&dest, b"keep").unwrap();
    assert!(backup(&env.paths, &env.store, &dest, &BackupCipher::DatabaseKey).is_err());
    assert_eq!(std::fs::read(&dest).unwrap(), b"keep");
}

#[test]
fn restore_rejects_non_cvewatch_database() {
    let env = common::initialized();
    let foreign = env.dir.path().join("foreign.db");
    Connection::open(&foreign)
        .unwrap()
        .execute_batch("CREATE TABLE x (id INTEGER);")
        .unwrap();
    let err = restore(&env.paths, &env.store, &foreign, &RestoreKey::LocalKey).unwrap_err();
    assert!(matches!(err, CoreError::Invalid(_)), "{err}");
    assert!(
        open_working_db(&env.paths, &env.store).is_ok(),
        "現有資料庫不受影響"
    );
}

/// 只在寫入 `db_key` 時失敗的憑證庫，模擬 rekey 中途失敗。
struct FailOnDbKeySet<'a>(&'a cvewatch_core::secrets::MemoryStore);

impl SecretStore for FailOnDbKeySet<'_> {
    fn get(
        &self,
        key: SecretKey,
    ) -> Result<Option<Zeroizing<String>>, cvewatch_core::secrets::SecretError> {
        self.0.get(key)
    }
    fn set(&self, key: SecretKey, value: &str) -> Result<(), cvewatch_core::secrets::SecretError> {
        if key == SecretKey::DbKey {
            return Err(cvewatch_core::secrets::SecretError::Backend {
                key,
                reason: "simulated".into(),
            });
        }
        self.0.set(key, value)
    }
    fn delete(&self, key: SecretKey) -> Result<(), cvewatch_core::secrets::SecretError> {
        self.0.delete(key)
    }
}

#[test]
fn interrupted_rekey_is_recovered_from_pending_key() {
    let env = common::initialized();
    insert_marker(&env);
    let old = load_db_key(&env.store).unwrap();

    assert!(rekey(&env.paths, &FailOnDbKeySet(&env.store)).is_err());
    // 資料庫已換成新金鑰，但 db_key 仍是舊的；pending 保有新金鑰。
    assert!(env.store.get(SecretKey::DbKeyPending).unwrap().is_some());

    let conn = open_working_db(&env.paths, &env.store).unwrap();
    assert!(marker_present(&conn));
    assert_ne!(load_db_key(&env.store).unwrap(), old, "已改用新金鑰");
    assert!(env.store.get(SecretKey::DbKeyPending).unwrap().is_none());
}

#[test]
fn successful_rekey_leaves_no_pending_key() {
    let env = common::initialized();
    rekey(&env.paths, &env.store).unwrap();
    assert!(env.store.get(SecretKey::DbKeyPending).unwrap().is_none());
}

#[test]
fn second_restore_keeps_first_before_restore_copy() {
    let env = common::initialized();
    let original_key = env.store.get(SecretKey::DbKey).unwrap().unwrap();
    insert_marker(&env);
    let bak = env.dir.path().join("a.db");
    backup(&env.paths, &env.store, &bak, &BackupCipher::DatabaseKey).unwrap();

    let first = restore(&env.paths, &env.store, &bak, &RestoreKey::LocalKey).unwrap();
    let second = restore(&env.paths, &env.store, &bak, &RestoreKey::LocalKey).unwrap();
    let (first, second) = (
        first.previous_saved_as.unwrap(),
        second.previous_saved_as.unwrap(),
    );
    assert_ne!(first, second, "不得覆蓋先前保留的副本");
    assert!(first.exists() && second.exists());
    let key = DbKey::from_hex(&original_key).unwrap();
    assert!(marker_present(
        &open_encrypted(&first, &key, false).unwrap()
    ));
}

#[test]
fn restore_of_newer_backup_leaves_current_database_untouched() {
    let env = common::initialized();
    let bak = env.dir.path().join("future.plain.db");
    backup(&env.paths, &env.store, &bak, &BackupCipher::Plain).unwrap();
    Connection::open(&bak)
        .unwrap()
        .execute(
            "INSERT INTO schema_version (version, name, applied_at) VALUES (999, 'future', 't')",
            [],
        )
        .unwrap();
    insert_marker(&env);

    let err = restore(&env.paths, &env.store, &bak, &RestoreKey::LocalKey).unwrap_err();
    assert!(
        matches!(err, CoreError::DbTooNew { found: 999, .. }),
        "{err}"
    );
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    assert!(marker_present(&conn), "現有資料庫維持原狀");
    let leftovers: Vec<_> = std::fs::read_dir(&env.paths.data_dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("restore"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn restore_does_not_modify_source_backup() {
    let env = common::initialized();
    insert_marker(&env);
    let bak = env.dir.path().join("src.db");
    backup(&env.paths, &env.store, &bak, &BackupCipher::DatabaseKey).unwrap();
    let before = std::fs::read(&bak).unwrap();
    restore(&env.paths, &env.store, &bak, &RestoreKey::LocalKey).unwrap();
    assert_eq!(std::fs::read(&bak).unwrap(), before);
}

#[test]
fn backup_from_before_rekey_restores_with_recovery_key() {
    let env = common::initialized();
    insert_marker(&env);
    let recovery = export_recovery_key(&env.paths, &env.store, Utc::now()).unwrap();
    let old_backup = env.dir.path().join("before-rekey.db");
    backup(
        &env.paths,
        &env.store,
        &old_backup,
        &BackupCipher::DatabaseKey,
    )
    .unwrap();
    rekey(&env.paths, &env.store).unwrap();

    // 本機金鑰已換，直接還原失敗（結束碼 5）且現有資料庫不變。
    let err = restore(&env.paths, &env.store, &old_backup, &RestoreKey::LocalKey).unwrap_err();
    assert!(matches!(err, CoreError::BackupKeyInvalid), "{err}");

    let old_key = DbKey::from_recovery_string(&recovery).unwrap();
    restore(
        &env.paths,
        &env.store,
        &old_backup,
        &RestoreKey::RecoveryKey(&old_key),
    )
    .unwrap();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    assert!(marker_present(&conn), "以目前的本機金鑰重新加密寫入");
    assert!(open_encrypted(&env.paths.db_path, &old_key, false).is_err());
}

#[test]
fn wrong_recovery_key_exits_5_and_keeps_current_database() {
    let env = common::initialized();
    insert_marker(&env);
    let bak = env.dir.path().join("bak.db");
    backup(&env.paths, &env.store, &bak, &BackupCipher::DatabaseKey).unwrap();
    let before = std::fs::read(&env.paths.db_path).unwrap();

    let wrong = DbKey::generate().unwrap();
    let err = restore(
        &env.paths,
        &env.store,
        &bak,
        &RestoreKey::RecoveryKey(&wrong),
    )
    .unwrap_err();
    assert!(matches!(err, CoreError::BackupKeyInvalid), "{err}");
    assert_eq!(err.exit_code(), ExitCode::DbKeyError);
    assert_eq!(std::fs::read(&env.paths.db_path).unwrap(), before);
    assert!(marker_present(
        &open_working_db(&env.paths, &env.store).unwrap()
    ));
}
