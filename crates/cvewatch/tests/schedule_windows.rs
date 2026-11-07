//! 06 任務 5.4：實際建立／查詢／移除 Windows 排程工作與無視窗包裝程式。
//!
//! 會修改工作排程器，預設略過；在使用者機器上手動執行：
//! `cargo test -p cvewatch --test schedule_windows -- --ignored --test-threads=1`
#![cfg(windows)]

use assert_cmd::Command;

fn cvewatch(config: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("cvewatch").unwrap();
    cmd.env_remove("CVEWATCH_CONFIG")
        .arg("--config")
        .arg(config);
    cmd
}

fn config() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cvewatch.toml");
    let data = dir.path().join("data");
    std::fs::write(
        &path,
        format!(
            "data_dir = '{}'\n[schedule]\nsync_every_hours = 2\ndigest_at = \"07:45\"\n",
            data.display()
        ),
    )
    .unwrap();
    (dir, path)
}

#[test]
#[ignore = "會建立真實的 Windows 排程工作；手動執行"]
fn install_status_reinstall_uninstall() {
    let (_dir, cfg) = config();
    cvewatch(&cfg)
        .args(["schedule", "install"])
        .assert()
        .success();
    let out = cvewatch(&cfg)
        .args(["schedule", "status"])
        .assert()
        .success();
    let text = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(
        text.contains(r"cvewatch\sync") && text.contains(r"cvewatch\digest"),
        "{text}"
    );
    // 重複安裝以新設定取代，不產生重複工作
    cvewatch(&cfg)
        .args(["schedule", "install"])
        .assert()
        .success();
    cvewatch(&cfg)
        .args(["schedule", "uninstall"])
        .assert()
        .success();
    let out = cvewatch(&cfg)
        .args(["schedule", "status"])
        .assert()
        .success();
    let text = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(text.contains("尚未安裝"), "{text}");
}

#[test]
#[ignore = "需要已建置的 cvewatchw.exe；手動執行"]
fn wrapper_returns_exit_code_of_cvewatch() {
    let wrapper = assert_cmd::cargo::cargo_bin("cvewatchw");
    // 未知子命令：cvewatch 以 1 結束，包裝程式原樣傳回
    let status = std::process::Command::new(&wrapper)
        .arg("frobnicate")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
    let status = std::process::Command::new(&wrapper)
        .arg("--version")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(0));
}
