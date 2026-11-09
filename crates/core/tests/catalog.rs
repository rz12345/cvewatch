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
key = "example-fw"
display_name = "ExampleOS"
vendor_display = "Example Networks"
version_scheme = "dotted"
aliases = ["examplegate"]
cna = [{ vendor = "Example Networks", product = "ExampleOS" }]
"#,
    )])
    .unwrap();
    let dsm = catalog.get("synology-dsm").unwrap();
    assert_eq!(dsm.display_name, "Synology DSM");
    assert!(dsm.cpe.is_empty());
    let forti = catalog.resolve("ExampleGate").unwrap();
    assert_eq!(forti.key, "example-fw");
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

#[test]
fn keywords_are_inherited_or_overridden() {
    // 03：product-catalog「Product keywords」
    let seed = Catalog::seed().unwrap();
    let proxmox = seed.get("proxmox-ve").unwrap();
    assert!(proxmox
        .keywords
        .iter()
        .any(|k| k == "Proxmox Virtual Environment"));

    let inherited = with_user(&[(
        "a.toml",
        "[[product]]\nkey = \"proxmox-ve\"\naliases = [\"pve8\"]\n",
    )])
    .unwrap();
    assert_eq!(
        inherited.get("proxmox-ve").unwrap().keywords,
        proxmox.keywords
    );

    let overridden = with_user(&[(
        "a.toml",
        "[[product]]\nkey = \"proxmox-ve\"\nkeywords = [\" Proxmox Backup \", \"\"]\n",
    )])
    .unwrap();
    assert_eq!(
        overridden.get("proxmox-ve").unwrap().keywords,
        vec!["Proxmox Backup".to_string()]
    );
}

// ── 14：種子目錄擴充 ─────────────────────────────────────────

/// spec 情境的產品名稱 → 產品 key（名稱為顯示名稱或「廠牌＋產品」別名）。
fn assert_listed(catalog: &Catalog, expected: &[(&str, &str)]) {
    for (name, key) in expected {
        let found = catalog.resolve(name).map(|p| p.key.as_str());
        assert_eq!(found, Some(*key), "目錄應包含 {name}");
    }
}

/// Scenario: Network firmware and appliances
#[test]
fn seed_lists_network_firmware_and_appliances() {
    assert_listed(
        &Catalog::seed().unwrap(),
        &[
            ("PAN-OS", "paloalto-panos"),
            ("AOS-CX", "hpe-aoscx"),
            ("HPE ArubaOS", "hpe-arubaos"),
            ("HPE AOS-Switch", "hpe-aos-switch"),
            ("Cisco IOS", "cisco-ios"),
            ("Cisco IOS XE", "cisco-iosxe"),
            ("Cisco NX-OS", "cisco-nxos"),
            ("Cisco ASA", "cisco-asa"),
            ("Cisco Secure Firewall Threat Defense", "cisco-ftd"),
            ("Juniper Junos OS", "juniper-junos"),
            ("Arista EOS", "arista-eos"),
            ("MikroTik RouterOS", "mikrotik-routeros"),
            ("Fortinet FortiOS", "fortinet-fortios"),
            ("FortiClient", "fortinet-forticlient"),
            ("SonicWall SonicOS", "sonicwall-sonicos"),
            ("Sophos Firewall", "sophos-firewall"),
            ("WatchGuard Fireware", "watchguard-fireware"),
            ("Check Point Gaia", "checkpoint-gaia"),
            ("Ivanti Connect Secure", "ivanti-connect-secure"),
            ("F5 BIG-IP", "f5-bigip"),
            ("NetScaler ADC", "citrix-netscaler"),
            ("RUCKUS Unleashed", "ruckus-unleashed"),
            ("RUCKUS SmartZone", "ruckus-smartzone"),
            ("UniFi Network", "ubiquiti-unifi-network"),
            ("PIXIS", "pixis"),
        ],
    );
}

/// Scenario: Storage and backup products
#[test]
fn seed_lists_storage_and_backup_products() {
    assert_listed(
        &Catalog::seed().unwrap(),
        &[
            ("DiskStation Manager", "synology-dsm"),
            ("QNAP QTS", "qnap-qts"),
            ("QNAP QuTS hero", "qnap-quts-hero"),
            ("ASUSTOR ADM", "asustor-adm"),
            ("TrueNAS", "truenas"),
            ("NetApp ONTAP", "netapp-ontap"),
            ("QSAN SANOS", "qsan-sanos"),
            ("Veeam Backup & Replication", "veeam-backup-replication"),
            ("NAKIVO Backup & Replication", "nakivo-backup-replication"),
            ("Acronis Cyber Protect", "acronis-cyber-protect"),
            ("Commvault", "commvault"),
            ("Veritas NetBackup", "veritas-netbackup"),
            ("Veritas Backup Exec", "veritas-backup-exec"),
        ],
    );
}

/// Scenario: Server software and libraries
#[test]
fn seed_lists_server_software_and_libraries() {
    assert_listed(
        &Catalog::seed().unwrap(),
        &[
            ("nginx", "nginx"),
            ("Apache HTTP Server", "apache-httpd"),
            ("Apache Tomcat", "apache-tomcat"),
            ("MariaDB", "mariadb"),
            ("MySQL", "mysql"),
            ("PostgreSQL", "postgresql"),
            ("Microsoft SQL Server", "mssql-server"),
            ("MongoDB", "mongodb"),
            ("Redis", "redis"),
            ("Ruby", "ruby"),
            ("Ruby on Rails", "rails"),
            ("PHP", "php"),
            ("Python", "python"),
            ("Node.js", "nodejs"),
            ("Java SE", "java-se"),
            ("Django", "django"),
            ("Laravel", "laravel"),
            ("WordPress", "wordpress"),
            ("Drupal", "drupal"),
            ("CKEditor 4", "ckeditor4"),
            ("TinyMCE", "tinymce"),
            ("Bootstrap", "bootstrap"),
            ("jQuery", "jquery"),
            ("jQuery UI", "jquery-ui"),
            ("Lodash", "lodash"),
            ("Moment.js", "momentjs"),
            ("OpenSSL", "openssl"),
            ("OpenSSH", "openssh"),
            ("Samba", "samba"),
            ("BIND 9", "bind9"),
            ("FreeRADIUS", "freeradius"),
        ],
    );
}

/// Scenario: Brand and product spellings
#[test]
fn brand_and_product_spellings_resolve() {
    assert_listed(
        &Catalog::seed().unwrap(),
        &[
            ("HPE Aruba Networking AOS-CX", "hpe-aoscx"),
            ("Cisco IOS XE Software", "cisco-iosxe"),
            ("Juniper Junos", "juniper-junos"),
            ("FortiGate FortiOS", "fortinet-fortios"),
            ("QNAP QTS", "qnap-qts"),
            ("Apache httpd", "apache-httpd"),
            ("Microsoft SQL Server", "mssql-server"),
            ("moment.js", "momentjs"),
            ("Ckeditor4", "ckeditor4"),
            ("Ruby on Rails", "rails"),
        ],
    );
}

/// Scenario: Product with keywords only
#[test]
fn pixis_has_keywords_only() {
    let catalog = Catalog::seed().unwrap();
    let pixis = catalog.get("pixis").unwrap();
    assert!(pixis.cpe.is_empty() && pixis.cna.is_empty());
    assert!(!pixis.keywords.is_empty());
}

#[test]
fn seed_has_78_products_each_with_an_identifier() {
    let catalog = Catalog::seed().expect("種子目錄載入（別名衝突會失敗）");
    assert_eq!(catalog.products().len(), 78);
    for p in catalog.products() {
        assert!(
            !p.cpe.is_empty() || !p.cna.is_empty() || !p.keywords.is_empty(),
            "{} 至少要有一組 CPE、CNA 或關鍵字",
            p.key
        );
    }
}

/// Scenario: Model spelling without a local alias（虛構型號）
#[test]
fn model_spelling_without_a_local_alias_is_unresolved() {
    let catalog = Catalog::seed().unwrap();
    assert!(catalog.resolve("Aruba XYZ-100").is_none());
}

/// Scenario: Model alias declared locally（虛構型號）
#[test]
fn model_alias_declared_locally() {
    let catalog = with_user(&[(
        "site.toml",
        r#"
[[product]]
key = "hpe-aoscx"
aliases = ["Aruba XYZ-100"]
"#,
    )])
    .unwrap();
    assert_eq!(catalog.resolve("Aruba XYZ-100").unwrap().key, "hpe-aoscx");
    assert_eq!(catalog.resolve("aruba xyz100").unwrap().key, "hpe-aoscx");
}

/// Scenario: Per-model product declared locally（虛構型號代碼）
#[test]
fn per_model_product_declared_locally() {
    let catalog = with_user(&[(
        "site.toml",
        r#"
[[product]]
key = "zyxel-switch"
display_name = "Zyxel 交換器韌體"
vendor_display = "Zyxel"
version_scheme = "zyxel"
aliases = ["Zyxel ABC-100", "Zyxel ABC-200"]
cpe = [
  { part = "o", vendor = "zyxel", product = "abc-100_firmware" },
  { part = "o", vendor = "zyxel", product = "abc-200_firmware" },
]
"#,
    )])
    .unwrap();
    let p = catalog.get("zyxel-switch").unwrap();
    assert_eq!(p.version_scheme, "zyxel");
    assert_eq!(p.cpe.len(), 2);
    assert_eq!(
        catalog.resolve("Zyxel ABC-200").unwrap().key,
        "zyxel-switch"
    );
    assert!(p.scheme().parse("V2.80(ABCD.1)C0").is_some());
}
