//! 20 任務 2.2、2.3：畫面編修的財產編號唯一、序號可重複、新增資產時的 IP、取代 IP，
//! 以及無效 IP 讓整批不寫入（asset-inventory 的 Unique property number、Creating an asset、
//! Editing asset fields）。

mod common;

use common::matching::{at, world, World};
use cvewatch_core::assets::edit::{self, EditOp, NewComponent};
use cvewatch_core::assets::validate::RecordInput;
use cvewatch_core::assets::{AssetIpRepo, AssetRepo, IpInput};
use cvewatch_core::matching::run::rematch_state;
use cvewatch_core::CoreError;

const NOW: &str = "2026-11-09T00:00:00+00:00";

fn now() -> chrono::DateTime<chrono::Utc> {
    at(NOW)
}

fn message(r: Result<impl std::fmt::Debug, CoreError>) -> String {
    match r {
        Err(CoreError::Invalid(m)) => m,
        other => panic!("應以 Invalid 拒絕：{other:?}"),
    }
}

fn fields<'a>(name: &'a str) -> RecordInput<'a> {
    RecordInput {
        name,
        category: "HW05",
        ..RecordInput::default()
    }
}

fn ip<'a>(address: &'a str, purpose: &'a str) -> IpInput<'a> {
    IpInput { address, purpose }
}

fn ips_of(w: &World, tag: &str) -> Vec<String> {
    let id = AssetRepo::new(&w.conn)
        .find_by_tag(tag)
        .unwrap()
        .unwrap()
        .id;
    AssetIpRepo::new(&w.conn)
        .for_asset(id)
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn set_property(w: &World, tag: &str, n: &str) {
    w.conn
        .execute(
            "UPDATE asset SET property_number = ?2 WHERE asset_tag = ?1",
            [tag, n],
        )
        .unwrap();
}

fn stale_needed(w: &World) -> bool {
    rematch_state(&w.conn, &w.catalog, &w.settings)
        .unwrap()
        .needed()
}

/// Scenario: Property number used by a retired asset
#[test]
fn property_number_used_by_a_retired_asset() {
    let mut w = world();
    w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.2-72806")]);
    w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.2-72806")]);
    set_property(&w, "NAS-02", "P-0002");
    edit::retire_asset(&mut w.conn, "NAS-02", now()).unwrap();
    let m = message(edit::update_asset(
        &mut w.conn,
        &w.classes,
        "NAS-01",
        &RecordInput {
            property_number: "p-0002",
            ..fields("nas01")
        },
        now(),
    ));
    assert_eq!(m, "財產編號 p-0002 已被 NAS-02 使用（該資產已退役）");
    let nas1 = AssetRepo::new(&w.conn)
        .find_by_tag("NAS-01")
        .unwrap()
        .unwrap();
    assert_eq!(nas1.record.property_number, None, "NAS-01 不變");
    assert_eq!(nas1.record.name, "NAS-01");
    // 自身的財產編號只改大小寫可通過
    set_property(&w, "NAS-01", "P-0001");
    edit::update_asset(
        &mut w.conn,
        &w.classes,
        "NAS-01",
        &RecordInput {
            property_number: "p-0001",
            ..fields("nas01")
        },
        now(),
    )
    .unwrap();
    // 新增資產也檢查
    let m = message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &RecordInput {
            asset_tag: "NAS-03",
            property_number: "P-0002",
            ..fields("nas03")
        },
        &[NewComponent {
            product_key: "synology-dsm".into(),
            version_raw: "7.2.2-72806".into(),
        }],
        &[],
        now(),
    ));
    assert!(m.contains("已被 NAS-02 使用"), "{m}");
    assert!(AssetRepo::new(&w.conn)
        .find_by_tag("NAS-03")
        .unwrap()
        .is_none());
}

/// Scenario: Serial number may repeat
#[test]
fn serial_number_may_repeat() {
    let mut w = world();
    w.asset("SRV-01", "internal", &[("vmware-esxi", "8.0")]);
    w.asset("SRV-02", "internal", &[("vmware-esxi", "8.0")]);
    for (tag, serial) in [("SRV-01", "SN0001"), ("SRV-02", "sn0001")] {
        edit::update_asset(
            &mut w.conn,
            &w.classes,
            tag,
            &RecordInput {
                serial_number: serial,
                ..fields(tag)
            },
            now(),
        )
        .unwrap();
    }
    let repo = AssetRepo::new(&w.conn);
    let serial = |tag| repo.find_by_tag(tag).unwrap().unwrap().record.serial_number;
    assert_eq!(serial("SRV-01").as_deref(), Some("SN0001"));
    assert_eq!(serial("SRV-02").as_deref(), Some("sn0001"));
}

/// Scenario: Adding an asset with addresses
#[test]
fn adding_an_asset_with_addresses() {
    let mut w = world();
    edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &RecordInput {
            asset_tag: "FW-EDGE",
            name: "fw-edge",
            category: "HW03",
            hostname: "fw-edge",
            ..RecordInput::default()
        },
        &[NewComponent {
            product_key: "paloalto-panos".into(),
            version_raw: "11.2.3".into(),
        }],
        &[ip("192.0.2.1", ""), ip("192.0.2.2", "")],
        now(),
    )
    .unwrap();
    let fw = AssetRepo::new(&w.conn)
        .find_by_tag("FW-EDGE")
        .unwrap()
        .unwrap();
    assert_eq!(fw.record.hostname.as_deref(), Some("fw-edge"));
    assert_eq!(ips_of(&w, "FW-EDGE"), ["192.0.2.1", "192.0.2.2"]);
    // 無效 IP：不建立資產
    let m = message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &RecordInput {
            asset_tag: "FW-EDGE-2",
            name: "fw-edge-2",
            category: "HW03",
            ..RecordInput::default()
        },
        &[NewComponent {
            product_key: "paloalto-panos".into(),
            version_raw: "11.2.3".into(),
        }],
        &[ip("192.0.2.0/24", "")],
        now(),
    ));
    assert!(m.contains("只能填單一 IPv4 或 IPv6 位址"), "{m}");
    assert!(AssetRepo::new(&w.conn)
        .find_by_tag("FW-EDGE-2")
        .unwrap()
        .is_none());
}

/// Scenario: Replacing addresses（只改 IP 不記錄「比對待補」）
#[test]
fn replacing_addresses() {
    let mut w = world();
    w.asset("SRV-01", "internal", &[("vmware-esxi", "8.0")]);
    edit::apply_edits(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        "SRV-01",
        &[EditOp::SetIps(vec![
            ip("192.0.2.10", ""),
            ip("192.0.2.11", ""),
        ])],
        now(),
    )
    .unwrap();
    w.run_at(NOW);
    assert!(!stale_needed(&w));
    edit::apply_edits(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        "SRV-01",
        &[EditOp::SetIps(vec![
            ip("192.0.2.10", ""),
            ip("2001:db8::10", ""),
        ])],
        now(),
    )
    .unwrap();
    assert_eq!(ips_of(&w, "SRV-01"), ["192.0.2.10", "2001:db8::10"]);
    assert!(!stale_needed(&w), "只改 IP 不需要重新比對");
    assert!(!EditOp::SetIps(Vec::new()).affects_matching());
}

/// Scenario: One invalid address saves nothing
#[test]
fn one_invalid_address_saves_nothing() {
    let mut w = world();
    w.asset("SRV-01", "internal", &[("vmware-esxi", "8.0")]);
    edit::apply_edits(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        "SRV-01",
        &[EditOp::SetIps(vec![ip("192.0.2.10", "管理")])],
        now(),
    )
    .unwrap();
    let m = message(edit::apply_edits(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        "SRV-01",
        &[
            EditOp::Fields(Box::new(fields("srv01"))),
            EditOp::SetIps(vec![ip("192.0.2.10", "管理"), ip("192.0.2.300", "")]),
        ],
        now(),
    ));
    assert!(
        m.contains("「192.0.2.300」不是有效的 IPv4 或 IPv6 位址"),
        "{m}"
    );
    let srv = AssetRepo::new(&w.conn)
        .find_by_tag("SRV-01")
        .unwrap()
        .unwrap();
    assert_eq!(srv.record.name, "SRV-01", "名稱不變");
    assert_eq!(ips_of(&w, "SRV-01"), ["192.0.2.10（管理）"]);
}
