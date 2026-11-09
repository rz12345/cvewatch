//! 18 任務 5.1：`cvewatch asset import-isms` 的說明、資料庫未初始化、`asset isms-draft` 已移除。
//!
//! 測試不存取 Credential Manager，執行檔無法開啟加密資料庫：預覽與實際匯入的輸出由
//! `commands/asset_isms.rs` 的單元測試以記憶體資料庫驗證（同一組 core 函式與輸出函式）。

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

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

/// 暫存設定檔與資料目錄（沒有資料庫）。
fn config(dir: &Path) -> PathBuf {
    let cfg = dir.join("cvewatch.toml");
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(&cfg, format!("data_dir = '{}'\n", data.display())).unwrap();
    cfg
}

#[test]
fn import_isms_help_is_in_chinese() {
    cvewatch()
        .args(["asset", "import-isms", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("從 ISMS 資訊資產清冊匯入資產"))
        .stdout(predicate::str::contains("<WORKBOOK>"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--prune").not());
}

#[test]
fn import_isms_without_database_asks_for_init() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    for extra in [&[][..], &["--dry-run"][..]] {
        cvewatch()
            .arg("--config")
            .arg(&cfg)
            .args(["asset", "import-isms"])
            .arg(fixture("isms_workbook.xlsx"))
            .args(extra)
            .assert()
            .failure()
            .stderr(predicate::str::contains("cvewatch init"));
    }
    assert!(
        !dir.path().join("data").join("cvewatch.db").exists(),
        "不建立資料庫"
    );
}

#[test]
fn isms_draft_is_removed() {
    cvewatch()
        .args(["asset", "isms-draft", "--help"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("isms-draft"));
}
