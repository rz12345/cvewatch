//! 21 任務 6.2：`asset catalog list` 的角色欄與 `asset catalog show` 的「角色：中文名稱（值）」。
//! 只使用內建種子與暫存目錄的 `catalog.d`，不開啟資料庫、不存取 Credential Manager。

use assert_cmd::Command;

/// 指向暫存資料目錄的設定檔；`catalog_d` 有值時寫入 `catalog.d\site.toml`。
fn setup(catalog_d: Option<&str>) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cvewatch.toml");
    let data = dir.path().join("data");
    std::fs::write(&path, format!("data_dir = '{}'\n", data.display())).unwrap();
    if let Some(text) = catalog_d {
        let catalog_dir = data.join("catalog.d");
        std::fs::create_dir_all(&catalog_dir).unwrap();
        std::fs::write(catalog_dir.join("site.toml"), text).unwrap();
    }
    (dir, path)
}

fn run(cfg: &std::path::Path, args: &[&str]) -> String {
    let mut cmd = Command::cargo_bin("cvewatch").unwrap();
    cmd.env_remove("CVEWATCH_CONFIG");
    let out = cmd.arg("--config").arg(cfg).args(args).assert().success();
    String::from_utf8(out.get_output().stdout.clone()).unwrap()
}

/// `key` 那一列以空白切開後的欄位。
fn row<'a>(out: &'a str, key: &str) -> Vec<&'a str> {
    out.lines()
        .find(|l| l.split_whitespace().next() == Some(key))
        .unwrap_or_else(|| panic!("清單缺少 {key}：{out}"))
        .split_whitespace()
        .collect()
}

#[test]
fn role_in_the_product_list() {
    let (_dir, cfg) = setup(None);
    for args in [&["asset", "catalog", "list"][..], &["asset", "catalog"][..]] {
        let out = run(&cfg, args);
        let header: Vec<_> = out.lines().next().unwrap().split_whitespace().collect();
        let role_col = header.iter().position(|h| *h == "role").expect("role 欄");
        assert_eq!(header[role_col - 1], "scheme", "role 在 scheme 之後");
        // display_name 與 vendor 可能含空白：從列尾往回數固定的數字欄
        let from_end = header.len() - role_col;
        for (key, role) in [("paloalto-panos", "firmware"), ("mysql", "dbms")] {
            let cells = row(&out, key);
            assert_eq!(cells[cells.len() - from_end], role, "{key}");
        }
    }
}

#[test]
fn role_in_product_details() {
    let (_dir, cfg) = setup(None);
    let out = run(&cfg, &["asset", "catalog", "show", "dell-idrac9"]);
    assert!(out.contains("角色：BMC（bmc）"), "{out}");
}

#[test]
fn local_product_without_a_role_in_details() {
    let (_dir, cfg) = setup(Some(
        "[[product]]\nkey = \"example-fw\"\ndisplay_name = \"Example FW\"\n\
vendor_display = \"Example\"\nversion_scheme = \"dotted\"\n",
    ));
    let out = run(&cfg, &["asset", "catalog", "show", "example-fw"]);
    assert!(out.contains("角色：其他（other）"), "{out}");
}

/// 21 code review：`catalog.d` 的 role 寫錯時 `asset show` 在輸出任何內容前就失敗
/// （目錄在開啟資料庫前載入，因此不需要資料庫或 Credential Manager）。
#[test]
fn asset_show_fails_before_output_on_bad_role() {
    let (_dir, cfg) = setup(Some(
        "[[product]]\nkey = \"freeradius\"\nrole = \"database\"\n",
    ));
    let mut cmd = Command::cargo_bin("cvewatch").unwrap();
    cmd.env_remove("CVEWATCH_CONFIG");
    let out = cmd
        .arg("--config")
        .arg(&cfg)
        .args(["asset", "show", "SRV-01"])
        .assert()
        .code(2);
    let output = out.get_output();
    assert!(output.stdout.is_empty(), "不留下半份輸出");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("site.toml") && stderr.contains("database"),
        "{stderr}"
    );
}
