//! 21-component-roles：目錄的元件角色宣告、`catalog.d` 覆寫與種子角色。

use std::collections::BTreeMap;

use cvewatch_core::catalog::{Catalog, CatalogSource, ComponentRole, SEED_SOURCE_NAME, SEED_TOML};
use cvewatch_core::CoreError;

const LOCAL_FW: &str = "[[product]]\nkey = \"example-fw\"\ndisplay_name = \"Example FW\"\n\
vendor_display = \"Example\"\nversion_scheme = \"dotted\"\n";

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

fn role_of(catalog: &Catalog, key: &str) -> ComponentRole {
    catalog
        .get(key)
        .unwrap_or_else(|| panic!("目錄缺少 {key}"))
        .role
}

#[test]
fn product_without_a_role_is_other() {
    let catalog = with_user(&[("site.toml", LOCAL_FW)]).unwrap();
    assert_eq!(role_of(&catalog, "example-fw"), ComponentRole::Other);
    assert_eq!(role_of(&catalog, "example-fw").label(), "其他");
}

#[test]
fn unknown_role_value_names_file_key_value_and_choices() {
    let text = format!("{LOCAL_FW}role = \"database\"\n");
    let err = with_user(&[("site.toml", &text)]).unwrap_err().to_string();
    for needle in [
        "site.toml",
        "example-fw",
        "database",
        "firmware、bios、bmc、os、dbms、web、tool、security、suite、app、other",
    ] {
        assert!(err.contains(needle), "缺少「{needle}」：{err}");
    }
}

#[test]
fn unknown_role_on_seed_override_points_to_user_file() {
    let err = with_user(&[(
        "site.toml",
        "[[product]]\nkey = \"freeradius\"\nrole = \"Security\"\n",
    )])
    .unwrap_err()
    .to_string();
    assert!(err.contains("site.toml"), "{err}");
    assert!(!err.contains(SEED_SOURCE_NAME), "{err}");
    assert!(
        err.contains("freeradius") && err.contains("Security"),
        "{err}"
    );
}

#[test]
fn overriding_a_seed_role_keeps_other_fields() {
    let seed = Catalog::seed().unwrap();
    let catalog = with_user(&[(
        "site.toml",
        "[[product]]\nkey = \"freeradius\"\nrole = \"security\"\n",
    )])
    .unwrap();
    let before = seed.get("freeradius").unwrap();
    let after = catalog.get("freeradius").unwrap();
    assert_eq!(before.role, ComponentRole::Tool);
    assert_eq!(after.role, ComponentRole::Security);
    assert_eq!(after.role.label(), "資通安全系統");
    assert_eq!(after.aliases, before.aliases);
    assert_eq!(after.cpe, before.cpe);
    assert_eq!(after.cna, before.cna);
}

#[test]
fn override_without_a_role_keeps_the_seed_role() {
    let catalog = with_user(&[(
        "site.toml",
        "[[product]]\nkey = \"vmware-esxi\"\naliases = [\"example esxi\"]\n",
    )])
    .unwrap();
    assert_eq!(role_of(&catalog, "vmware-esxi"), ComponentRole::Os);
    assert_eq!(role_of(&catalog, "vmware-esxi").label(), "作業系統");
}

#[test]
fn role_declared_for_a_local_product() {
    let text = "[[product]]\nkey = \"example-bmc\"\ndisplay_name = \"Example BMC\"\n\
vendor_display = \"Example\"\nversion_scheme = \"dotted\"\nrole = \"bmc\"\n";
    let catalog = with_user(&[("site.toml", text)]).unwrap();
    assert_eq!(role_of(&catalog, "example-bmc"), ComponentRole::Bmc);
    assert_eq!(role_of(&catalog, "example-bmc").label(), "BMC");
}

#[test]
fn later_user_file_wins_in_name_order() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("10-a.toml"),
        "[[product]]\nkey = \"nginx\"\nrole = \"tool\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("20-b.toml"),
        "[[product]]\nkey = \"nginx\"\nrole = \"security\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("30-c.toml"),
        "[[product]]\nkey = \"nginx\"\naliases = [\"example nginx\"]\n",
    )
    .unwrap();
    let catalog = Catalog::load(dir.path()).unwrap();
    assert_eq!(role_of(&catalog, "nginx"), ComponentRole::Security);
}

#[test]
fn explicit_other_resets_a_seed_role() {
    let catalog = with_user(&[(
        "site.toml",
        "[[product]]\nkey = \"mysql\"\nrole = \"other\"\n",
    )])
    .unwrap();
    assert_eq!(role_of(&catalog, "mysql"), ComponentRole::Other);
}

#[test]
fn role_change_does_not_change_fingerprint() {
    let seed = Catalog::seed().unwrap();
    let catalog = with_user(&[(
        "site.toml",
        "[[product]]\nkey = \"freeradius\"\nrole = \"security\"\n",
    )])
    .unwrap();
    assert_eq!(catalog.fingerprint(), seed.fingerprint());
}

/// design D3 的種子角色表。
const SEED_ROLES: &[(ComponentRole, &[&str])] = &[
    (
        ComponentRole::Firmware,
        &[
            "paloalto-panos",
            "hpe-aoscx",
            "hpe-aos-switch",
            "hpe-arubaos",
            "cisco-ios",
            "cisco-iosxe",
            "cisco-nxos",
            "cisco-asa",
            "cisco-ftd",
            "juniper-junos",
            "arista-eos",
            "mikrotik-routeros",
            "fortinet-fortios",
            "sonicwall-sonicos",
            "sophos-firewall",
            "watchguard-fireware",
            "checkpoint-gaia",
            "ivanti-connect-secure",
            "f5-bigip",
            "citrix-netscaler",
            "ruckus-unleashed",
            "ruckus-smartzone",
            "synology-dsm",
            "qnap-qts",
            "qnap-quts-hero",
            "asustor-adm",
            "netapp-ontap",
            "qsan-sanos",
        ],
    ),
    (ComponentRole::Bios, &["dell-poweredge-bios"]),
    (ComponentRole::Bmc, &["dell-idrac9"]),
    (
        ComponentRole::Os,
        &[
            "vmware-esxi",
            "proxmox-ve",
            "truenas",
            "debian",
            "ubuntu",
            "rhel",
            "windows-server",
        ],
    ),
    (
        ComponentRole::Dbms,
        &[
            "mariadb",
            "mysql",
            "postgresql",
            "mssql-server",
            "mongodb",
            "redis",
        ],
    ),
    (
        ComponentRole::Web,
        &["nginx", "apache-httpd", "apache-tomcat"],
    ),
    (
        ComponentRole::Tool,
        &[
            "ubiquiti-unifi-network",
            "vmware-vcenter",
            "openssh",
            "samba",
            "bind9",
            "freeradius",
        ],
    ),
    (ComponentRole::Security, &["fortinet-forticlient", "pixis"]),
    (
        ComponentRole::Suite,
        &[
            "veeam-backup-replication",
            "nakivo-backup-replication",
            "acronis-cyber-protect",
            "commvault",
            "veritas-netbackup",
            "veritas-backup-exec",
        ],
    ),
    (
        ComponentRole::App,
        &[
            "ruby",
            "rails",
            "php",
            "python",
            "nodejs",
            "java-se",
            "django",
            "laravel",
            "wordpress",
            "drupal",
            "ckeditor4",
            "tinymce",
            "bootstrap",
            "jquery",
            "jquery-ui",
            "lodash",
            "momentjs",
            "openssl",
        ],
    ),
];

fn assert_roles(expected: ComponentRole, keys: &[&str]) {
    let catalog = Catalog::seed().unwrap();
    for key in keys {
        assert_eq!(role_of(&catalog, key), expected, "{key}");
    }
}

#[test]
fn seed_network_and_storage_firmware() {
    assert_roles(
        ComponentRole::Firmware,
        &[
            "paloalto-panos",
            "hpe-aoscx",
            "cisco-ios",
            "fortinet-fortios",
            "ruckus-unleashed",
            "synology-dsm",
        ],
    );
}

#[test]
fn seed_server_components() {
    assert_roles(
        ComponentRole::Os,
        &["vmware-esxi", "proxmox-ve", "ubuntu", "windows-server"],
    );
    assert_roles(ComponentRole::Bmc, &["dell-idrac9"]);
    assert_roles(ComponentRole::Bios, &["dell-poweredge-bios"]);
    assert_roles(ComponentRole::Tool, &["vmware-vcenter"]);
}

#[test]
fn seed_server_software() {
    assert_roles(ComponentRole::Dbms, &["mysql", "postgresql"]);
    assert_roles(ComponentRole::Web, &["nginx", "apache-tomcat"]);
    assert_roles(ComponentRole::Suite, &["veeam-backup-replication"]);
    assert_roles(ComponentRole::Security, &["fortinet-forticlient"]);
    assert_roles(ComponentRole::App, &["php", "rails", "jquery"]);
}

#[test]
fn every_seed_product_has_a_declared_role() {
    let catalog = Catalog::seed().unwrap();
    let other: Vec<_> = catalog
        .products()
        .iter()
        .filter(|p| p.role == ComponentRole::Other)
        .map(|p| p.key.as_str())
        .collect();
    assert!(other.is_empty(), "角色為其他：{other:?}");
    let declared = SEED_TOML
        .lines()
        .filter(|l| l.starts_with("role = "))
        .count();
    assert_eq!(declared, catalog.products().len());
}

#[test]
fn seed_roles_match_the_design_table() {
    let catalog = Catalog::seed().unwrap();
    let mut listed = 0;
    for (role, keys) in SEED_ROLES {
        assert_roles(*role, keys);
        listed += keys.len();
    }
    assert_eq!(listed, 78);
    assert_eq!(catalog.products().len(), 78);

    let counts: BTreeMap<&str, usize> =
        catalog
            .products()
            .iter()
            .fold(BTreeMap::new(), |mut acc, p| {
                *acc.entry(p.role.as_str()).or_default() += 1;
                acc
            });
    let expected: BTreeMap<&str, usize> = [
        ("firmware", 28),
        ("bios", 1),
        ("bmc", 1),
        ("os", 7),
        ("dbms", 6),
        ("web", 3),
        ("tool", 6),
        ("security", 2),
        ("suite", 6),
        ("app", 18),
    ]
    .into_iter()
    .collect();
    assert_eq!(counts, expected);
}

/// 21 任務 6.3：使用說明 `catalog.d` 範例（含角色覆寫）可被目錄載入。
#[test]
fn user_guide_catalog_example_loads() {
    let guide = include_str!("../../../docs/user-guide/assets.md");
    let example = guide
        .split("```toml")
        .skip(1)
        .filter_map(|block| block.split("```").next())
        .find(|block| block.contains("example-bmc"))
        .expect("使用說明的 catalog.d 範例");
    let catalog = with_user(&[("site.toml", example)]).unwrap();
    assert_eq!(role_of(&catalog, "freeradius"), ComponentRole::Security);
    assert_eq!(role_of(&catalog, "example-bmc"), ComponentRole::Bmc);
    assert_eq!(role_of(&catalog, "zyxel-switch"), ComponentRole::Firmware);
    assert_eq!(role_of(&catalog, "hpe-aoscx"), ComponentRole::Firmware);
}
