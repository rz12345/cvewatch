//! 1.4：`--help`、未知子命令與設定錯誤的結束碼。不存取真實的 Credential Manager。

use assert_cmd::Command;
use predicates::prelude::*;

const SUBCOMMANDS: [&str; 12] = [
    "init", "sync", "tui", "asset", "config", "db", "cve", "match", "finding", "pending", "notify",
    "schedule",
];

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
    for sub in ["list", "show", "set", "conflicts"] {
        cvewatch()
            .args(["finding", sub, "--help"])
            .assert()
            .success();
    }
    for sub in ["list", "resolve"] {
        cvewatch()
            .args(["pending", sub, "--help"])
            .assert()
            .success();
    }
    for sub in ["urgent", "digest", "test-email", "test-redmine", "outbox"] {
        cvewatch()
            .args(["notify", sub, "--help"])
            .assert()
            .success();
    }
    for sub in ["install", "status", "uninstall"] {
        cvewatch()
            .args(["schedule", sub, "--help"])
            .assert()
            .success();
    }
}

#[test]
fn insecure_smtp_without_exception_is_a_config_error() {
    let (_dir, path) = config_with(
        "[notify.email]
security = \"none\"
",
    );
    cvewatch()
        .arg("--config")
        .arg(&path)
        .args(["notify", "outbox"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("allow_insecure_transport"));
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
fn tui_without_database_exits_before_touching_terminal() {
    let (_dir, cfg) = config_with("");
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .arg("tui")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("cvewatch init"));
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
        // 警告後繼續執行，直到資料庫檢查才停止
        .stderr(predicate::str::contains("cvewatch init"));
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

#[test]
fn sync_and_cve_help_list_options() {
    let out = cvewatch().args(["sync", "--help"]).assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    for opt in ["--source", "--rescan", "--since", "--history"] {
        assert!(
            text.contains(opt),
            "sync --help 缺少 {opt}：
{text}"
        );
    }
    for sub in ["show", "search"] {
        cvewatch().args(["cve", sub, "--help"]).assert().success();
    }
    let show = cvewatch()
        .args(["cve", "show", "--help"])
        .assert()
        .success();
    assert!(String::from_utf8_lossy(&show.get_output().stdout).contains("--fetch"));
}

#[test]
fn sync_argument_errors_exit_1() {
    cvewatch()
        .args(["sync", "--since", "2026-13-01"])
        .assert()
        .code(1);
    cvewatch()
        .args(["sync", "--history", "--rescan"])
        .assert()
        .code(1);
    let (_dir, cfg) = config_with("");
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["sync", "--source", "bogus"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("未知的來源"));
}

#[test]
fn sync_without_database_asks_for_init() {
    let (_dir, cfg) = config_with("");
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["sync", "--source", "kev"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("cvewatch init"));
}

#[test]
fn cve_show_validates_id() {
    let (_dir, cfg) = config_with("");
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["cve", "show", "CVE-24-1"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("不是有效的 CVE ID"));
}

#[test]
fn invalid_csaf_source_config_exits_2() {
    let (_dir, cfg) = config_with(
        "[[sources.csaf]]
id = \"v\"
discovery = \"rss\"
url = \"https://v.example/rss.xml\"
",
    );
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["sync", "--source", "kev"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("csaf_url_template"));
}

/// finding-triage「Ignoring without a reason」：參數驗證在開啟資料庫之前，不需憑證庫。
#[test]
fn ignoring_without_reason_fails_and_lists_reasons() {
    let (_dir, cfg) = config_with("");
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args(["finding", "set", "42", "ignored", "--note", "x"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--reason"))
        .stderr(predicate::str::contains("not_affected"))
        .stderr(predicate::str::contains("risk_accepted"))
        .stderr(predicate::str::contains("false_positive"));
}

#[test]
fn not_affected_without_justification_lists_values() {
    let (_dir, cfg) = config_with("");
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .args([
            "finding",
            "set",
            "42",
            "ignored",
            "--reason",
            "not_affected",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("requires_configuration"))
        .stderr(predicate::str::contains("protected_at_perimeter"));
}

#[test]
fn pending_resolve_requires_apply_or_dismiss() {
    cvewatch()
        .args(["pending", "resolve", "CVE-2024-3400"])
        .assert()
        .code(1);
    cvewatch()
        .args([
            "pending",
            "resolve",
            "CVE-2024-3400",
            "--apply",
            "FW-01",
            "--dismiss",
        ])
        .assert()
        .code(1);
}

#[test]
fn invalid_scoring_config_exits_2() {
    let (_dir, cfg) = config_with(
        "[scoring]
epss_high = 2
",
    );
    cvewatch()
        .arg("--config")
        .arg(&cfg)
        .arg("match")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("scoring"));
}

/// code review #7：說明文字不應在句中出現續行留下的連續空白。
#[test]
fn finding_set_help_has_no_stray_indentation() {
    let assert = cvewatch()
        .args(["finding", "set", "--help"])
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("protected_by_mitigating_control"), "{out}");
    let description: String = out
        .lines()
        .take_while(|l| !l.starts_with("Usage"))
        .collect();
    assert!(!description.contains("    "), "說明含連續空白：\n{out}");
}
