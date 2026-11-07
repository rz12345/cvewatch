//! 1.4：`--help`、未知子命令與設定錯誤的結束碼。不存取真實的 Credential Manager。

use assert_cmd::Command;
use predicates::prelude::*;

const SUBCOMMANDS: [&str; 6] = ["init", "sync", "tui", "asset", "config", "db"];

fn cvewatch() -> Command {
    let mut cmd = Command::cargo_bin("cvewatch").unwrap();
    cmd.env_remove("CVEWATCH_CONFIG");
    cmd
}

/// 指向臨時資料目錄的設定檔，避免寫入使用者的 %LOCALAPPDATA%。
fn config_with(extra: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cvewatch.toml");
    let data = dir.path().join("data");
    std::fs::write(&path, format!("data_dir = '{}'\n{extra}", data.display())).unwrap();
    (dir, path)
}

#[test]
fn help_lists_all_subcommands() {
    let assert = cvewatch().arg("--help").assert().success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for sub in SUBCOMMANDS {
        assert!(out.contains(sub), "--help 缺少 {sub}：\n{out}");
    }
}

#[test]
fn every_subcommand_supports_help() {
    for sub in SUBCOMMANDS {
        cvewatch().args([sub, "--help"]).assert().success();
    }
    for sub in ["export-key", "import-key", "rekey", "backup", "restore"] {
        cvewatch().args(["db", sub, "--help"]).assert().success();
    }
    for sub in ["set-secret", "check"] {
        cvewatch()
            .args(["config", sub, "--help"])
            .assert()
            .success();
    }
}

#[test]
fn unknown_subcommand_exits_1() {
    cvewatch().arg("frobnicate").assert().code(1);
}

#[test]
fn missing_subcommand_exits_1() {
    cvewatch().assert().code(1);
}

#[test]
fn unknown_secret_key_is_rejected_by_parser() {
    cvewatch()
        .args(["config", "set-secret", "db_key"])
        .assert()
        .code(1);
}

#[test]
fn backup_flags_conflict() {
    cvewatch()
        .args(["db", "backup", "x.db", "--plain", "--passphrase"])
        .assert()
        .code(1);
}

#[test]
fn not_implemented_commands_exit_1() {
    let (_dir, cfg) = config_with("");
    for sub in ["sync", "tui"] {
        cvewatch()
            .arg("--config")
            .arg(&cfg)
            .arg(sub)
            .assert()
            .code(1)
            .stderr(predicate::str::contains("not implemented"));
    }
}

#[test]
fn type_error_in_config_exits_2_with_field_and_line() {
    let (_dir, cfg) = config_with("[http]\ntimeout_secs = \"abc\"\n");
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .arg("sync")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("http.timeout_secs"))
        .stderr(predicate::str::contains("第 3 行"));
}

#[test]
fn missing_explicit_config_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    cvewatch()
        .arg("--config")
        .arg(dir.path().join("none.toml"))
        .arg("sync")
        .assert()
        .code(2);
}

#[test]
fn unknown_key_warns_and_continues() {
    let (_dir, cfg) = config_with("foo = 1\n");
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .arg("sync")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("`foo`"))
        .stderr(predicate::str::contains("not implemented"));
}

#[test]
fn cli_config_overrides_environment() {
    let (_bad_dir, bad) = config_with("[http]\ntimeout_secs = \"abc\"\n");
    let (_good_dir, good) = config_with("");
    // 只有環境變數：讀到錯誤設定 → 2
    cvewatch()
        .env("CVEWATCH_CONFIG", &bad)
        .arg("sync")
        .assert()
        .code(2);
    // --config 優先 → 讀到正確設定，進到 sync（尚未實作）→ 1
    cvewatch()
        .env("CVEWATCH_CONFIG", &bad)
        .arg("--config")
        .arg(&good)
        .arg("sync")
        .assert()
        .code(1);
}

#[test]
fn writes_daily_log_file_in_data_dir() {
    let (dir, cfg) = config_with("");
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .arg("sync")
        .assert()
        .code(1);
    let logs: Vec<_> = std::fs::read_dir(dir.path().join("data").join("logs"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(logs.len(), 1, "{logs:?}");
    assert!(logs[0].starts_with("cvewatch.") && logs[0].ends_with(".log"));
}

#[test]
fn restore_key_flags_conflict() {
    cvewatch()
        .args(["db", "restore", "x.db", "--passphrase", "--recovery-key"])
        .assert()
        .code(1);
}

#[test]
fn asset_subcommands_support_help() {
    for sub in [
        "import",
        "import-packages",
        "list",
        "show",
        "template",
        "catalog",
    ] {
        cvewatch().args(["asset", sub, "--help"]).assert().success();
    }
    cvewatch()
        .args(["asset", "catalog", "show", "--help"])
        .assert()
        .success();
}

#[test]
fn asset_without_subcommand_exits_1() {
    cvewatch().arg("asset").assert().code(1);
}

#[test]
fn invalid_package_format_is_rejected_by_parser() {
    cvewatch()
        .args([
            "asset",
            "import-packages",
            "WEB-01",
            "x.json",
            "--format",
            "bogus",
        ])
        .assert()
        .code(1);
}

#[test]
fn template_writes_xlsx_and_refuses_overwrite() {
    let (dir, cfg) = config_with("");
    let out = dir.path().join("inv.xlsx");
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["asset", "template"])
        .arg(&out)
        .assert()
        .success();
    assert!(out.exists());
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["asset", "template"])
        .arg(&out)
        .assert()
        .code(1);
}

#[test]
fn catalog_list_and_show_use_seed_and_user_files() {
    let (dir, cfg) = config_with("");
    let catalog_dir = dir.path().join("data").join("catalog.d");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("local.toml"),
        "[[product]]
key = \"vmware-esxi\"
aliases = [\"r6515 esxi\"]
",
    )
    .unwrap();
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["asset", "catalog", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("paloalto-panos"));
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["asset", "catalog", "show", "vmware-esxi"])
        .assert()
        .success()
        .stdout(predicate::str::contains("r6515 esxi"));
}

#[test]
fn conflicting_user_catalog_exits_2() {
    let (dir, cfg) = config_with("");
    let catalog_dir = dir.path().join("data").join("catalog.d");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("bad.toml"),
        "[[product]]
key = \"vmware-vcenter\"
aliases = [\"esxi\"]
",
    )
    .unwrap();
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["asset", "catalog", "list"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("vmware-esxi"));
}

#[test]
fn invalid_import_column_target_exits_2() {
    let (_dir, cfg) = config_with(
        "[import.columns]
\"序號\" = \"serial\"
",
    );
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["asset", "catalog", "list"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("import.columns"));
}
