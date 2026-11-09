//! 17 任務 4.1：`cvewatch asset isms-draft` 的說明、產生草稿（結束碼 0 與摘要）、目的檔已存在、
//! 不是 ISMS 清冊。
//!
//! 測試不存取 Credential Manager：資料目錄沒有資料庫，草稿不參考既有資產（沿用既有資產的
//! 情境由 core 的 `isms_draft` 測試在記憶體資料庫上驗證）。

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

/// 暫存設定檔與資料目錄（含測試專用別名的 catalog.d）。
fn config(dir: &Path) -> PathBuf {
    let cfg = dir.join("cvewatch.toml");
    let data = dir.join("data");
    let catalog_dir = data.join("catalog.d");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::copy(
        fixture("isms_catalog.d/isms-test.toml"),
        catalog_dir.join("isms-test.toml"),
    )
    .unwrap();
    std::fs::write(&cfg, format!("data_dir = '{}'\n", data.display())).unwrap();
    cfg
}

fn isms_draft(cfg: &Path, workbook: &Path, out: &Path) -> assert_cmd::assert::Assert {
    cvewatch()
        .arg("--config")
        .arg(cfg)
        .args(["asset", "isms-draft"])
        .arg(workbook)
        .arg(out)
        .assert()
}

#[test]
fn isms_draft_help_is_in_chinese() {
    cvewatch()
        .args(["asset", "isms-draft", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("由 ISMS 資訊資產清冊產生"))
        .stdout(predicate::str::contains("<WORKBOOK>"))
        .stdout(predicate::str::contains("<OUTPUT>"))
        .stdout(predicate::str::contains("不可已存在"));
}

#[test]
fn isms_draft_generating_a_draft() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let out = dir.path().join("draft.xlsx");
    isms_draft(&cfg, &fixture("isms_workbook.xlsx"), &out)
        .code(0)
        .stdout(predicate::str::contains("已產生 ISMS 草稿"))
        .stdout(predicate::str::contains(
            "轉換 6 個資產（沿用既有 0）、略過 5 列、待補版本 7 個元件",
        ))
        .stdout(predicate::str::contains("略過 PE 1、IF 2、SE 1"))
        .stdout(predicate::str::contains("第 15 列"))
        .stdout(predicate::str::contains("不要使用 --prune"))
        .stdout(predicate::str::contains("找不到資料庫"));
    assert!(out.exists());
}

#[test]
fn isms_draft_target_exists() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let out = dir.path().join("draft.xlsx");
    std::fs::write(&out, b"keep me").unwrap();
    isms_draft(&cfg, &fixture("isms_workbook.xlsx"), &out)
        .code(1)
        .stderr(predicate::str::contains("目的檔已存在，不覆蓋"));
    assert_eq!(std::fs::read(&out).unwrap(), b"keep me");
}

#[test]
fn isms_draft_not_an_isms_workbook() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let out = dir.path().join("draft.xlsx");
    isms_draft(&cfg, &fixture("isms_not_workbook.xlsx"), &out)
        .failure()
        .stderr(predicate::str::contains("找不到 ISMS 資訊資產清冊的表頭"));
    assert!(!out.exists(), "不產生草稿");
    isms_draft(&cfg, &fixture("isms_missing_column.xlsx"), &out)
        .failure()
        .stderr(predicate::str::contains("缺少欄位：可用性"));
}
