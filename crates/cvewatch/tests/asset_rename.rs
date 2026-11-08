//! 12 任務 2.2：`cvewatch asset rename` 的說明與參數錯誤。
//!
//! 測試不得存取真實的 Credential Manager，執行檔無法開啟加密資料庫；更名的成功與失敗邏輯
//! 由 core 的 `asset_rename` 測試涵蓋（與 `asset_export.rs` 相同作法）。

use assert_cmd::Command;
use predicates::prelude::*;

fn cvewatch() -> Command {
    let mut cmd = Command::cargo_bin("cvewatch").unwrap();
    cmd.env_remove("CVEWATCH_CONFIG");
    cmd
}

#[test]
fn rename_help_is_in_chinese() {
    cvewatch()
        .args(["asset", "rename", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("更改資產編號"))
        .stdout(predicate::str::contains("目前的資產編號"))
        .stdout(predicate::str::contains("新的資產編號"));
}

#[test]
fn rename_requires_both_tags() {
    cvewatch()
        .args(["asset", "rename", "NAS-01"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("NEW_TAG"));
}

#[test]
fn rename_without_database_asks_for_init() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("cvewatch.toml");
    let data = dir.path().join("data");
    std::fs::write(&cfg, format!("data_dir = '{}'\n", data.display())).unwrap();
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["asset", "rename", "NAS-01", "NAS-11"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("cvewatch init"));
}
