//! 19 任務 5.3：`cvewatch asset system recode` 的說明與參數錯誤。
//!
//! 測試不得存取真實的 Credential Manager，執行檔無法開啟加密資料庫；修改代碼的成功與失敗邏輯
//! （含 Unknown system code）由 core 的 `system_recode` 測試涵蓋（與 `asset_rename.rs` 相同作法）。

use assert_cmd::Command;
use predicates::prelude::*;

fn cvewatch() -> Command {
    let mut cmd = Command::cargo_bin("cvewatch").unwrap();
    cmd.env_remove("CVEWATCH_CONFIG");
    cmd
}

#[test]
fn recode_help_is_in_chinese() {
    cvewatch()
        .args(["asset", "system", "recode", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("修改資產說明代碼"))
        .stdout(predicate::str::contains("目前的資產說明代碼"))
        .stdout(predicate::str::contains("新的資產說明代碼"))
        .stdout(predicate::str::contains("--dry-run"));
}

#[test]
fn system_help_lists_recode() {
    cvewatch()
        .args(["asset", "system", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("recode"));
}

#[test]
fn recode_requires_both_codes() {
    cvewatch()
        .args(["asset", "system", "recode", "NET"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("NEW_CODE"));
}

fn config_without_database() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("cvewatch.toml");
    let data = dir.path().join("data");
    std::fs::write(&cfg, format!("data_dir = '{}'\n", data.display())).unwrap();
    (dir, cfg)
}

#[test]
fn recode_without_database_asks_for_init() {
    let (_dir, cfg) = config_without_database();
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["asset", "system", "recode", "NET", "NWS", "--dry-run"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("cvewatch init"));
}

#[test]
fn recode_to_the_same_code_is_rejected() {
    let (_dir, cfg) = config_without_database();
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["asset", "system", "recode", "NET", "net"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("新代碼與原代碼相同：NET"));
}
