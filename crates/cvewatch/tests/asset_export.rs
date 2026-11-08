//! 10 任務 3.3～3.4：`cvewatch asset export` 的說明與「目的檔已存在」；清冊往返。
//!
//! 測試不得存取真實的 Credential Manager，執行檔無法開啟加密資料庫，因此往返以
//! `asset export` / `asset import --dry-run` 呼叫的同一組 core 函式在記憶體資料庫上驗證。

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use chrono::Utc;
use cvewatch_core::assets::columns::ColumnMap;
use cvewatch_core::assets::export::export_inventory;
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::plan::ImportPlan;
use cvewatch_core::assets::sheet::read_inventory;
use cvewatch_core::assets::validate::parse_inventory;
use cvewatch_core::catalog::Catalog;
use predicates::prelude::*;
use rusqlite::Connection;

fn cvewatch() -> Command {
    let mut cmd = Command::cargo_bin("cvewatch").unwrap();
    cmd.env_remove("CVEWATCH_CONFIG");
    cmd
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

#[test]
fn export_help_is_in_chinese() {
    cvewatch()
        .args(["asset", "export", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("匯出使用中資產為 Excel 清冊"))
        .stdout(predicate::str::contains("不可已存在"));
}

#[test]
fn export_target_exists() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("cvewatch.toml");
    let data = dir.path().join("data");
    std::fs::write(&cfg, format!("data_dir = '{}'\n", data.display())).unwrap();
    let out = dir.path().join("inv.xlsx");
    std::fs::write(&out, b"keep me").unwrap();
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["asset", "export"])
        .arg(&out)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("目的檔已存在，不覆蓋"));
    assert_eq!(std::fs::read(&out).unwrap(), b"keep me");
}

fn memory_db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    cvewatch_core::db::migrate::migrate(&mut conn).unwrap();
    conn
}

/// 與 `asset import` 相同：讀檔 → 驗證 → 計算差異（`dry_run` 時不寫入）。
fn import(conn: &mut Connection, path: &Path, dry_run: bool) -> ImportPlan {
    let book = read_inventory(path, None).unwrap();
    let inventory = parse_inventory(
        &book,
        &ColumnMap::default(),
        &cvewatch_core::assets::AssetClasses::builtin(),
    )
    .unwrap();
    let options = ImportOptions {
        dry_run,
        prune: false,
    };
    import_inventory(
        conn,
        &Catalog::seed().unwrap(),
        &inventory,
        options,
        Utc::now(),
    )
    .unwrap()
}

#[test]
fn round_trip_without_changes() {
    for name in ["inventory_basic.xlsx", "inventory_v2.xlsx"] {
        let mut conn = memory_db();
        let first = import(&mut conn, &fixture(name), false);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("inv.xlsx");
        let summary = export_inventory(&conn, &Catalog::seed().unwrap(), &out).unwrap();
        assert_eq!(summary.assets, first.added.len(), "{name}");
        let again = import(&mut conn, &out, true);
        assert!(again.added.is_empty(), "{name}: {:?}", again.added);
        assert!(again.updated.is_empty(), "{name}: {:?}", again.updated);
        assert_eq!(again.version_change_count(), 0, "{name}");
        assert!(
            again.disappeared.is_empty(),
            "{name}: {:?}",
            again.disappeared
        );
    }
}
