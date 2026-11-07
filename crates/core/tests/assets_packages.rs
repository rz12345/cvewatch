//! 4.5：四種格式 fixture、缺 distro、未知資產、整批取代與歷史。

mod common;

use std::path::PathBuf;

use chrono::Utc;
use cvewatch_core::assets::packages::import::import_packages;
use cvewatch_core::assets::packages::{detect, parse_packages, Distro, PackageFormat};
use cvewatch_core::assets::{
    AssetRecord, AssetRepo, Category, ComponentKind, ComponentRepo, Exposure,
};
use cvewatch_core::db::open_working_db;
use rusqlite::Connection;

fn read(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/packages")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

fn debian() -> Distro {
    Distro::parse("debian-12").unwrap()
}

fn add_asset(conn: &Connection, tag: &str) -> i64 {
    AssetRepo::new(conn)
        .insert(
            &AssetRecord {
                asset_tag: tag.into(),
                name: tag.into(),
                category: Category::Host,
                location: None,
                owner: None,
                exposure: Exposure::Internal,
                criticality: 2,
                notes: None,
            },
            Utc::now(),
        )
        .unwrap()
}

fn package_purls(conn: &Connection, asset_id: i64) -> Vec<(String, String)> {
    ComponentRepo::new(conn)
        .for_asset(asset_id)
        .unwrap()
        .into_iter()
        .filter_map(|c| match c.kind {
            ComponentKind::Package { purl_full, .. } => Some((purl_full, c.version_raw)),
            ComponentKind::Product { .. } => None,
        })
        .collect()
}

#[test]
fn detects_all_supported_formats() {
    assert_eq!(
        detect(&read("web01.cdx.json")).unwrap(),
        PackageFormat::CycloneDx
    );
    assert_eq!(
        detect(&read("web01.spdx.json")).unwrap(),
        PackageFormat::Spdx
    );
    assert_eq!(detect(&read("dpkg-v1.txt")).unwrap(), PackageFormat::Dpkg);
    assert_eq!(detect(&read("rpm.txt")).unwrap(), PackageFormat::Rpm);
}

#[test]
fn unknown_format_lists_supported_formats() {
    let err = parse_packages(&read("unknown.txt"), None, Some(&debian())).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("CycloneDX")
            && msg.contains("SPDX")
            && msg.contains("dpkg")
            && msg.contains("rpm"),
        "{msg}"
    );
    assert!(parse_packages("{\"foo\": 1}", None, None).is_err());
}

#[test]
fn cyclonedx_takes_every_component_with_purl() {
    let parsed = parse_packages(&read("web01.cdx.json"), None, None).unwrap();
    let names: Vec<&str> = parsed.entries.iter().map(|e| e.purl.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "pkg:deb/debian/openssl?arch=amd64&distro=debian-12",
            "pkg:deb/debian/libssl3?arch=amd64&distro=debian-12",
            "pkg:deb/debian/zlib1g?arch=amd64&distro=debian-12",
        ]
    );
    assert_eq!(parsed.skipped_without_purl, 1);
    assert_eq!(parsed.duplicates, 1);
    let zlib = &parsed.entries[2];
    assert_eq!(zlib.version, "1:1.2.13.dfsg-1");
    assert!(
        zlib.purl_full.contains("upstream=zlib"),
        "完整 purl 保留 upstream"
    );
}

#[test]
fn spdx_takes_purl_external_refs() {
    let parsed = parse_packages(&read("web01.spdx.json"), None, None).unwrap();
    assert_eq!(parsed.entries.len(), 2);
    assert_eq!(parsed.skipped_without_purl, 1);
    assert_eq!(parsed.entries[1].version, "7.88.1-10+deb12u8");
}

#[test]
fn dpkg_requires_distro() {
    let err = parse_packages(&read("dpkg-v1.txt"), None, None).unwrap_err();
    assert!(err.to_string().contains("--distro"), "{err}");
    assert!(err.to_string().contains("debian-12"), "{err}");
    assert!(Distro::parse("debian").is_err());
}

#[test]
fn dpkg_with_distro_builds_spec_purl() {
    let parsed = parse_packages(&read("dpkg-v1.txt"), None, Some(&debian())).unwrap();
    assert_eq!(parsed.entries.len(), 3, "版本為空的已移除套件略過");
    assert_eq!(
        parsed.entries[0].purl_full,
        "pkg:deb/debian/openssl@3.0.15-1~deb12u1?distro=debian-12"
    );
}

#[test]
fn rpm_keeps_epoch_in_version() {
    let rhel = Distro::parse("rhel-9").unwrap();
    let parsed = parse_packages(&read("rpm.txt"), None, Some(&rhel)).unwrap();
    assert_eq!(parsed.format, PackageFormat::Rpm);
    assert_eq!(parsed.entries[0].version, "1:3.0.7-27.el9");
    assert_eq!(
        parsed.entries[0].purl,
        "pkg:rpm/redhat/openssl?distro=rhel-9"
    );
    assert_eq!(
        parsed.entries[0].purl_full,
        "pkg:rpm/redhat/openssl@3.0.7-27.el9?distro=rhel-9&epoch=1"
    );
    assert_eq!(parsed.entries[1].version, "5.1.8-9.el9");
}

#[test]
fn import_replaces_snapshot_and_records_history() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let id = add_asset(&conn, "WEB-01");

    let v1 = parse_packages(&read("dpkg-v1.txt"), None, Some(&debian())).unwrap();
    let first = import_packages(&mut conn, "WEB-01", &v1, Utc::now()).unwrap();
    assert_eq!(first.added, 3);

    let v2 = parse_packages(&read("dpkg-v2.txt"), None, Some(&debian())).unwrap();
    let second = import_packages(&mut conn, "WEB-01", &v2, Utc::now()).unwrap();
    assert_eq!(
        second.removed,
        vec!["pkg:deb/debian/libfoo?distro=debian-12".to_string()]
    );
    assert_eq!(second.version_changes.len(), 1);
    assert_eq!(second.unchanged, 1);

    let packages = package_purls(&conn, id);
    assert_eq!(packages.len(), 2);
    assert!(packages.iter().any(|(_, v)| v == "3.0.15-1~deb12u2"));

    let components = ComponentRepo::new(&conn);
    let openssl = components
        .for_asset(id)
        .unwrap()
        .into_iter()
        .find(|c| c.label().contains("openssl"))
        .unwrap();
    assert_eq!(
        components.history(openssl.id).unwrap()[0].version_raw,
        "3.0.15-1~deb12u1"
    );
}

#[test]
fn import_requires_existing_active_asset() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let parsed = parse_packages(&read("web01.cdx.json"), None, None).unwrap();
    let err = import_packages(&mut conn, "NOPE-01", &parsed, Utc::now()).unwrap_err();
    assert!(err.to_string().contains("asset import"), "{err}");

    let id = add_asset(&conn, "OLD-01");
    AssetRepo::new(&conn).retire(id, Utc::now()).unwrap();
    assert!(import_packages(&mut conn, "OLD-01", &parsed, Utc::now()).is_err());
}

#[test]
fn excel_product_components_survive_package_import() {
    let env = common::initialized();
    let mut conn = open_working_db(&env.paths, &env.store).unwrap();
    let id = add_asset(&conn, "WEB-01");
    conn.execute(
        "INSERT INTO catalog_product (key, display_name, vendor_display, version_scheme)
         VALUES ('debian', 'Debian', 'Debian', 'dotted')",
        [],
    )
    .unwrap();
    let product = conn.last_insert_rowid();
    ComponentRepo::new(&conn)
        .upsert_product(id, product, "12.7", None, Utc::now())
        .unwrap();
    let parsed = parse_packages(&read("dpkg-v2.txt"), None, Some(&debian())).unwrap();
    import_packages(&mut conn, "WEB-01", &parsed, Utc::now()).unwrap();
    assert_eq!(ComponentRepo::new(&conn).for_asset(id).unwrap().len(), 3);
}
