//! 20 任務 1.1、1.3：0008 版本的資料庫套用 0009_asset_identity——既有資產的識別欄位為空、
//! 沒有 IP、命中數不變；財產編號不分大小寫唯一；`asset_ip` 的寫入順序與永久刪除時的連帶刪除。

use std::str::FromStr;

use chrono::{TimeZone, Utc};
use cvewatch_core::assets::edit::purge_asset;
use cvewatch_core::assets::{AssetIp, AssetIpRepo, AssetRepo, AssetStatus};
use cvewatch_core::db::migrate::{
    current_version, latest_version, migrate, migrate_with, MIGRATIONS,
};
use rusqlite::{params, Connection};

const T0: &str = "2026-11-01T00:00:00+00:00";

fn v8_db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    let v8: Vec<_> = MIGRATIONS
        .iter()
        .copied()
        .filter(|m| m.version <= 8)
        .collect();
    assert_eq!(migrate_with(&mut conn, &v8).unwrap(), 8);
    conn.execute(
        "INSERT INTO catalog_product (key, display_name, vendor_display, version_scheme)
         VALUES ('paloalto-panos', 'PAN-OS', 'Palo Alto Networks', 'panos')",
        [],
    )
    .unwrap();
    conn
}

/// 以 0008 的欄位新增資產與一個產品元件，回傳元件 id。
fn old_asset(conn: &Connection, tag: &str, status: &str) -> i64 {
    conn.execute(
        "INSERT INTO asset (asset_tag, name, category, exposure, status, first_seen, last_seen)
         VALUES (?1, ?1, 'HW03', 'internal', ?2, ?3, ?3)",
        params![tag, status, T0],
    )
    .unwrap();
    let asset_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO component (asset_id, kind, product_id, version_raw, source, updated_at)
         VALUES (?1, 'product', (SELECT id FROM catalog_product), '11.1.4', 'excel', ?2)",
        params![asset_id, T0],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn upgraded() -> Connection {
    let mut conn = v8_db();
    let fw = old_asset(&conn, "FW-01", "active");
    for cve in ["CVE-2024-3400", "CVE-2025-0108"] {
        conn.execute(
            "INSERT INTO finding (component_id, cve_id, confidence, decided_by, priority,
                                  first_seen, last_evaluated)
             VALUES (?1, ?2, 'confirmed', 'cna', 'P2', ?3, ?3)",
            params![fw, cve, T0],
        )
        .unwrap();
    }
    old_asset(&conn, "NAS-02", "retired");
    migrate(&mut conn).unwrap();
    conn
}

#[test]
fn upgrading_keeps_assets_without_identity_data() {
    let conn = upgraded();
    assert_eq!(current_version(&conn).unwrap(), latest_version());
    let repo = AssetRepo::new(&conn);
    for tag in ["FW-01", "NAS-02"] {
        let a = repo.find_by_tag(tag).unwrap().unwrap();
        let r = &a.record;
        assert_eq!(
            (
                &r.brand,
                &r.model,
                &r.serial_number,
                &r.property_number,
                &r.hostname,
                &r.management_url
            ),
            (&None, &None, &None, &None, &None, &None),
            "{tag}"
        );
    }
    assert_eq!(
        repo.find_by_tag("NAS-02").unwrap().unwrap().status,
        AssetStatus::Retired
    );
    assert_eq!(count(&conn, "SELECT count(*) FROM asset_ip"), 0);
    assert_eq!(
        count(&conn, "SELECT count(*) FROM finding"),
        2,
        "命中數不變"
    );
}

#[test]
fn property_number_is_unique_ignoring_case() {
    let conn = upgraded();
    conn.execute(
        "UPDATE asset SET property_number = 'P-0001' WHERE asset_tag = 'FW-01'",
        [],
    )
    .unwrap();
    let dup = conn.execute(
        "UPDATE asset SET property_number = 'p-0001' WHERE asset_tag = 'NAS-02'",
        [],
    );
    assert!(dup.is_err(), "唯一索引為最後防線");
    // 序號不檢查重複
    conn.execute("UPDATE asset SET serial_number = 'SN0001'", [])
        .unwrap();
}

fn ip(text: &str, purpose: Option<&str>) -> AssetIp {
    AssetIp {
        address: std::net::IpAddr::from_str(text).unwrap(),
        purpose: purpose.map(str::to_string),
    }
}

#[test]
fn replacing_addresses_keeps_input_order() {
    let conn = upgraded();
    let repo = AssetRepo::new(&conn);
    let fw = repo.find_by_tag("FW-01").unwrap().unwrap().id;
    let nas = repo.find_by_tag("NAS-02").unwrap().unwrap().id;
    let ips = AssetIpRepo::new(&conn);
    ips.replace(fw, &[ip("192.0.2.1", None), ip("192.0.2.2", Some("HA"))])
        .unwrap();
    ips.replace(
        fw,
        &[ip("2001:db8::10", None), ip("192.0.2.1", Some("管理"))],
    )
    .unwrap();
    assert_eq!(
        ips.for_asset(fw).unwrap(),
        vec![ip("2001:db8::10", None), ip("192.0.2.1", Some("管理"))]
    );
    ips.replace(nas, &[ip("192.0.2.1", None)])
        .expect("不同資產可有相同位址");
    let all = ips.all_by_asset().unwrap();
    assert_eq!(all[&fw].len(), 2);
    assert_eq!(all[&nas], vec![ip("192.0.2.1", None)]);
    let stored: String = conn
        .query_row(
            "SELECT address FROM asset_ip WHERE asset_id = ?1 ORDER BY id LIMIT 1",
            params![fw],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, "2001:db8::10", "以標準寫法儲存");
}

#[test]
fn purging_an_asset_removes_its_addresses() {
    let mut conn = upgraded();
    let nas = AssetRepo::new(&conn)
        .find_by_tag("NAS-02")
        .unwrap()
        .unwrap()
        .id;
    let fw = AssetRepo::new(&conn)
        .find_by_tag("FW-01")
        .unwrap()
        .unwrap()
        .id;
    AssetIpRepo::new(&conn)
        .replace(nas, &[ip("192.0.2.20", None)])
        .unwrap();
    AssetIpRepo::new(&conn)
        .replace(fw, &[ip("192.0.2.1", None)])
        .unwrap();
    let now = Utc.with_ymd_and_hms(2026, 11, 9, 0, 0, 0).unwrap();
    purge_asset(&mut conn, "NAS-02", now).unwrap();
    assert_eq!(
        count(
            &conn,
            &format!("SELECT count(*) FROM asset_ip WHERE asset_id = {nas}")
        ),
        0
    );
    assert_eq!(
        count(&conn, "SELECT count(*) FROM asset_ip"),
        1,
        "其他資產不受影響"
    );
}
