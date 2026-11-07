//! 4.5：migration 成功/失敗回復/版本過新；兩個連線同時讀寫；鎖過期取代。

mod common;

use chrono::{Duration, Utc};
use cvewatch_core::db::lock::{acquire_sync_lock, release_sync_lock, LockOutcome};
use cvewatch_core::db::migrate::{current_version, migrate_with, Migration};
use cvewatch_core::db::{open_encrypted, open_working_db, DbKey};
use cvewatch_core::{CoreError, ExitCode};
use rusqlite::Connection;

const M1: Migration = Migration {
    version: 1,
    name: "0001",
    sql: "CREATE TABLE schema_version (version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at TEXT NOT NULL);",
};
const M2: Migration = Migration {
    version: 2,
    name: "0002",
    sql: "CREATE TABLE t2 (id INTEGER);",
};
const M3: Migration = Migration {
    version: 3,
    name: "0003",
    sql: "CREATE TABLE t3 (id INTEGER);",
};
const M4: Migration = Migration {
    version: 4,
    name: "0004",
    sql: "CREATE TABLE t4 (id INTEGER);",
};
const M5: Migration = Migration {
    version: 5,
    name: "0005",
    sql: "CREATE TABLE t5 (id INTEGER);",
};
const M5_BROKEN: Migration = Migration {
    version: 5,
    name: "0005_broken",
    sql: "CREATE TABLE t5 (id INTEGER); THIS IS NOT SQL;",
};

fn keyed_db() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("m.db");
    let key = DbKey::generate().unwrap();
    // open_encrypted 會套用內建 migration；這裡改用獨立 SQLCipher 連線測試自訂清單。
    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "key", key.pragma_value().as_str())
        .unwrap();
    (dir, conn)
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        == 1
}

#[test]
fn upgrades_from_3_to_5_in_order() {
    let (_d, mut conn) = keyed_db();
    assert_eq!(migrate_with(&mut conn, &[M1, M2, M3]).unwrap(), 3);
    assert_eq!(migrate_with(&mut conn, &[M1, M2, M3, M4, M5]).unwrap(), 5);
    assert_eq!(current_version(&conn).unwrap(), 5);
    assert!(table_exists(&conn, "t4") && table_exists(&conn, "t5"));
}

#[test]
fn failed_migration_rolls_back_whole_batch() {
    let (_d, mut conn) = keyed_db();
    migrate_with(&mut conn, &[M1, M2, M3]).unwrap();
    let err = migrate_with(&mut conn, &[M1, M2, M3, M4, M5_BROKEN]).unwrap_err();
    assert!(
        matches!(err, CoreError::Migration { version: 5, .. }),
        "{err}"
    );
    assert_ne!(err.exit_code(), ExitCode::Success);
    assert_eq!(current_version(&conn).unwrap(), 3);
    assert!(!table_exists(&conn, "t4"), "4 應一併回復");
}

#[test]
fn refuses_database_newer_than_binary() {
    let (_d, mut conn) = keyed_db();
    migrate_with(&mut conn, &[M1, M2, M3]).unwrap();
    let err = migrate_with(&mut conn, &[M1, M2]).unwrap_err();
    assert!(matches!(
        err,
        CoreError::DbTooNew {
            found: 3,
            supported: 2
        }
    ));
    assert_eq!(current_version(&conn).unwrap(), 3);
}

#[test]
fn builtin_migrations_apply_on_open() {
    let env = common::initialized();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    assert_eq!(
        current_version(&conn).unwrap(),
        cvewatch_core::db::migrate::latest_version()
    );
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
    let fk: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fk, 1);
}

#[test]
fn reader_sees_last_commit_while_writer_holds_transaction() {
    let env = common::initialized();
    let mut writer = open_working_db(&env.paths, &env.store).unwrap();
    let reader = open_working_db(&env.paths, &env.store).unwrap();
    let insert = "INSERT INTO sync_run (started_at, source, status) VALUES ('t', 'nvd', 'running')";
    writer.execute(insert, []).unwrap();

    let tx = writer.transaction().unwrap();
    tx.execute(insert, []).unwrap();
    let count = |c: &Connection| -> i64 {
        c.query_row("SELECT count(*) FROM sync_run", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(
        count(&reader),
        1,
        "讀取端看到最後一次提交的資料，且不被鎖住"
    );
    tx.commit().unwrap();
    assert_eq!(count(&reader), 2);
}

#[test]
fn sync_lock_blocks_second_holder() {
    let env = common::initialized();
    let mut a = open_working_db(&env.paths, &env.store).unwrap();
    let mut b = open_working_db(&env.paths, &env.store).unwrap();
    let now = Utc::now();
    assert_eq!(
        acquire_sync_lock(&mut a, 100, now).unwrap(),
        LockOutcome::Acquired
    );
    let err = acquire_sync_lock(&mut b, 200, now + Duration::minutes(30)).unwrap_err();
    assert!(matches!(err, CoreError::SyncLocked { pid: 100, .. }));
    assert_eq!(err.exit_code(), ExitCode::SyncLocked);

    assert!(!release_sync_lock(&b, 200).unwrap(), "不能釋放別人的鎖");
    assert!(release_sync_lock(&a, 100).unwrap());
    assert_eq!(
        acquire_sync_lock(&mut b, 200, now).unwrap(),
        LockOutcome::Acquired
    );
}

#[test]
fn stale_lock_is_replaced() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let three_hours_ago = Utc::now() - Duration::hours(3);
    acquire_sync_lock(&mut conn, 100, three_hours_ago).unwrap();
    let outcome = acquire_sync_lock(&mut conn, 200, Utc::now()).unwrap();
    assert!(matches!(
        outcome,
        LockOutcome::ReplacedStale {
            previous_pid: 100,
            ..
        }
    ));
    let holder: u32 = conn
        .query_row("SELECT pid FROM sync_lock", [], |r| r.get(0))
        .unwrap();
    assert_eq!(holder, 200);
}

#[test]
fn open_without_create_requires_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let key = DbKey::generate().unwrap();
    let err = open_encrypted(&dir.path().join("none.db"), &key, false).unwrap_err();
    assert!(matches!(err, CoreError::DbNotFound(_)));
}
