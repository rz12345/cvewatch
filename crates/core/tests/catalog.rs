//! 2.7：別名命中、衝突、覆寫、相似建議、各 scheme 的可解析/不可解析樣本。

mod common;

use cvewatch_core::catalog::scheme::scheme_by_name;
use cvewatch_core::catalog::store::sync_catalog;
use cvewatch_core::catalog::{Catalog, CatalogSource, SEED_SOURCE_NAME, SEED_TOML};
use cvewatch_core::db::open_working_db;
use cvewatch_core::{CoreError, ExitCode};

fn with_user(files: &[(&str, &str)]) -> Result<Catalog, CoreError> {
    let mut sources = vec![CatalogSource {
        name: SEED_SOURCE_NAME.into(),
        text: SEED_TOML.into(),
    }];
    sources.extend(files.iter().map(|(n, t)| CatalogSource {
        name: (*n).into(),
        text: (*t).into(),
    }));
    Catalog::from_sources(&sources)
}

#[test]
fn seed_contains_required_products() {
    let catalog = Catalog::seed().unwrap();
    for key in [
        "paloalto-panos",
        "vmware-esxi",
        "vmware-vcenter",
        "proxmox-ve",
        "dell-idrac9",
        "dell-poweredge-bios",
        "synology-dsm",
        "debian",
        "ubuntu",
        "rhel",
        "windows-server",
    ] {
        assert!(catalog.get(key).is_some(), "種子缺少 {key}");
    }
}

#[test]
fn different_spellings_resolve_to_same_product() {
    let catalog = Catalog::seed().unwrap();
    for name in [
        "PAN-OS",
        "Palo Alto PA-460",
        "panos",
        "ＰＡＮ－ＯＳ",
        "pa460",
    ] {
        assert_eq!(
            catalog.resolve(name).map(|p| p.key.as_str()),
            Some("paloalto-panos"),
            "{name}"
        );
    }
    assert!(catalog.resolve("Synolgy DSM").is_none());
}

#[test]
fn typo_gets_suggestion() {
    let catalog = Catalog::seed().unwrap();
    let suggestions = catalog.suggest("Synolgy DSM");
    assert_eq!(suggestions.first(), Some(&"synology-dsm"));
    assert!(suggestions.len() <= 3);
    assert!(catalog.suggest("completely unrelated thing").is_empty());
}

#[test]
fn user_file_adds_alias_and_keeps_seed_fields() {
    let catalog = with_user(&[(
        "local.toml",
        "[[product]]\nkey = \"vmware-esxi\"\naliases = [\"r6515 esxi\"]\n",
    )])
    .unwrap();
    let esxi = catalog.resolve("R6515 ESXi").unwrap();
    assert_eq!(esxi.key, "vmware-esxi");
    assert_eq!(esxi.display_name, "ESXi", "未寫的欄位沿用種子");
    assert!(catalog.resolve("vmware esxi").is_some(), "種子別名保留");
}

#[test]
fn user_file_overrides_fields_and_adds_products() {
    let catalog = with_user(&[(
        "local.toml",
        r#"
[[product]]
key = "synology-dsm"
display_name = "Synology DSM"
cpe = []

[[product]]
key = "fortinet-fortios"
display_name = "FortiOS"
vendor_display = "Fortinet"
version_scheme = "dotted"
aliases = ["fortigate"]
cna = [{ vendor = "Fortinet", product = "FortiOS" }]
"#,
    )])
    .unwrap();
    let dsm = catalog.get("synology-dsm").unwrap();
    assert_eq!(dsm.display_name, "Synology DSM");
    assert!(dsm.cpe.is_empty());
    let forti = catalog.resolve("FortiGate").unwrap();
    assert_eq!(forti.key, "fortinet-fortios");
    assert!(
        forti.cpe.is_empty() && forti.cna.len() == 1,
        "只有 CNA 識別也可載入"
    );
}

#[test]
fn conflicting_alias_is_rejected() {
    let err = with_user(&[(
        "local.toml",
        "[[product]]\nkey = \"vmware-vcenter\"\naliases = [\"ESXi\"]\n",
    )])
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("ESXi") && msg.contains("vmware-esxi") && msg.contains("vmware-vcenter"),
        "{msg}"
    );
    assert_eq!(err.exit_code(), ExitCode::ConfigError);
}

#[test]
fn new_product_requires_fields_and_valid_scheme() {
    let missing = with_user(&[("a.toml", "[[product]]\nkey = \"x\"\n")]).unwrap_err();
    assert!(missing.to_string().contains("version_scheme"), "{missing}");
    let bad_scheme = with_user(&[(
        "a.toml",
        "[[product]]\nkey = \"x\"\ndisplay_name = \"X\"\nvendor_display = \"V\"\nversion_scheme = \"semver9\"\n",
    )])
    .unwrap_err();
    assert!(bad_scheme.to_string().contains("semver9"), "{bad_scheme}");
    let unknown_field =
        with_user(&[("a.toml", "[[product]]\nkey = \"x\"\nalias = []\n")]).unwrap_err();
    assert!(
        unknown_field.to_string().contains("a.toml"),
        "{unknown_field}"
    );
}

#[test]
fn loads_user_directory_in_name_order() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("10-a.toml"),
        "[[product]]\nkey = \"synology-dsm\"\ndisplay_name = \"A\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("20-b.toml"),
        "[[product]]\nkey = \"synology-dsm\"\ndisplay_name = \"B\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();
    let catalog = Catalog::load(dir.path()).unwrap();
    assert_eq!(catalog.get("synology-dsm").unwrap().display_name, "B");
    assert!(Catalog::load(&dir.path().join("missing")).is_ok());
}

#[test]
fn version_scheme_samples() {
    let cases: &[(&str, &[&str], &[&str])] = &[
        (
            "panos",
            &["11.1.4-h7", "11.1.6", "10.2.9-H1"],
            &["11.1 latest", "11.1", "11.1.4-h"],
        ),
        (
            "dsm",
            &["7.2.1-69057", "7.2.2-72806 Update 3", "7.2-64570"],
            &["7.2.2", "DSM 7"],
        ),
        (
            "esxi",
            &[
                "8.0 U3c",
                "8.0 Update 3c",
                "8.0.3",
                "24414501",
                "8.0.3 build-24414501",
                "7.0U3 P08",
            ],
            &["eight", "8.0 U"],
        ),
        (
            "dotted",
            &["7.10.30.00", "2.16.0", "10.0.20348.2700", "12"],
            &["8.2-4", "v1.2", ""],
        ),
        (
            "ubuntu",
            &["22.04", "22.04.4 LTS", "24.04"],
            &["jammy", "22.4"],
        ),
    ];
    for (name, ok, bad) in cases {
        let scheme = scheme_by_name(name).unwrap();
        for v in *ok {
            assert!(scheme.parse(v).is_some(), "{name} 應可解析 {v}");
        }
        for v in *bad {
            assert!(scheme.parse(v).is_none(), "{name} 不應解析 {v}");
        }
    }
    assert_eq!(
        scheme_by_name("esxi")
            .unwrap()
            .parse("  8.0   U3c ")
            .unwrap()
            .normalized,
        "8.0 u3c"
    );
}

#[test]
fn catalog_syncs_to_database_idempotently() {
    let env = common::initialized();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    let catalog = Catalog::seed().unwrap();
    let first = sync_catalog(&conn, &catalog).unwrap();
    let second = sync_catalog(&conn, &catalog).unwrap();
    assert_eq!(first, second, "product_id 穩定");
    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(count("catalog_product"), catalog.products().len() as i64);
    assert!(count("catalog_alias") > 30);
    let alias_owner: String = conn
        .query_row(
            "SELECT p.key FROM catalog_alias a JOIN catalog_product p ON p.id = a.product_id
             WHERE a.alias_norm = 'paloaltopa460'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(alias_owner, "paloalto-panos");
}
