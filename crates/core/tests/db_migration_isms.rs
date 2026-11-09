//! 13 任務 2.3：0006 版本的資料庫套用 0007_isms_class（Scenario: Upgrading a database with a
//! firewall）——類別轉為子類別代碼、C/I/A 帶入內建範本值、notes 附記舊重要性，元件與命中不變。

use cvewatch_core::assets::{AssetRepo, AssetStatus, Cia};
use cvewatch_core::db::migrate::{
    current_version, latest_version, migrate, migrate_with, MIGRATIONS,
};
use rusqlite::{params, Connection};

const T0: &str = "2026-11-01T00:00:00+00:00";

fn v6_db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    let v6: Vec<_> = MIGRATIONS
        .iter()
        .copied()
        .filter(|m| m.version <= 6)
        .collect();
    assert_eq!(migrate_with(&mut conn, &v6).unwrap(), 6);
    conn.execute(
        "INSERT INTO catalog_product (key, display_name, vendor_display, version_scheme)
         VALUES ('paloalto-panos', 'PAN-OS', 'Palo Alto Networks', 'panos')",
        [],
    )
    .unwrap();
    conn
}

/// 以 0006 的欄位新增資產與一個產品元件，回傳元件 id。
fn old_asset(
    conn: &Connection,
    tag: &str,
    category: &str,
    criticality: u8,
    notes: Option<&str>,
    status: &str,
) -> i64 {
    conn.execute(
        "INSERT INTO asset (asset_tag, name, category, exposure, criticality, notes, status,
                            first_seen, last_seen)
         VALUES (?1, ?1, ?2, 'internal', ?3, ?4, ?5, ?6, ?6)",
        params![tag, category, criticality, notes, status, T0],
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

fn finding(conn: &Connection, component_id: i64, cve: &str) {
    conn.execute(
        "INSERT INTO finding (component_id, cve_id, confidence, decided_by, priority,
                              first_seen, last_evaluated)
         VALUES (?1, ?2, 'confirmed', 'cna', 'P2', ?3, ?3)",
        params![component_id, cve, T0],
    )
    .unwrap();
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn upgrading_a_database_with_a_firewall() {
    let mut conn = v6_db();
    let fw = old_asset(
        &conn,
        "FW-01",
        "security",
        3,
        Some("機房 A 主防火牆"),
        "active",
    );
    finding(&conn, fw, "CVE-2024-3400");
    finding(&conn, fw, "CVE-2025-0108");
    old_asset(&conn, "NAS-01", "storage", 2, Some("  "), "active");
    old_asset(&conn, "OLD-01", "network", 1, None, "retired");
    let before = (
        count(&conn, "SELECT count(*) FROM finding"),
        count(&conn, "SELECT count(*) FROM component"),
    );

    migrate(&mut conn).unwrap();
    assert_eq!(current_version(&conn).unwrap(), latest_version());

    let repo = AssetRepo::new(&conn);
    let fw = repo.find_by_tag("FW-01").unwrap().unwrap();
    assert_eq!(fw.record.category.as_str(), "HW03");
    assert_eq!(fw.record.cia, Cia::new(2, 3, 4));
    assert_eq!(
        fw.record.notes.as_deref(),
        Some("機房 A 主防火牆；舊重要性：3")
    );
    assert_eq!(fw.record.isms_id, None);
    let nas = repo.find_by_tag("NAS-01").unwrap().unwrap();
    assert_eq!(nas.record.category.as_str(), "HW05");
    assert_eq!(nas.record.notes.as_deref(), Some("舊重要性：2"));
    let old = repo.find_by_tag("OLD-01").unwrap().unwrap();
    assert_eq!(old.status, AssetStatus::Retired, "retired 資產一併轉換");
    assert_eq!(old.record.category.as_str(), "HW02");
    assert_eq!(old.record.notes.as_deref(), Some("舊重要性：1"));
    assert_eq!(
        (
            count(&conn, "SELECT count(*) FROM finding"),
            count(&conn, "SELECT count(*) FROM component"),
        ),
        before,
        "命中與元件不變"
    );
    let values: Vec<(String, i64)> = conn
        .prepare("SELECT asset_tag, asset_value FROM active_component ORDER BY asset_tag")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        values,
        vec![("FW-01".to_string(), 4), ("NAS-01".to_string(), 4)]
    );
}

#[test]
fn isms_id_index_is_case_insensitive() {
    let mut conn = v6_db();
    old_asset(&conn, "FW-01", "security", 2, None, "active");
    old_asset(&conn, "FW-02", "security", 2, None, "active");
    migrate(&mut conn).unwrap();
    conn.execute(
        "UPDATE asset SET isms_id = 'SYS-HW-001' WHERE asset_tag = 'FW-01'",
        [],
    )
    .unwrap();
    let dup = conn.execute(
        "UPDATE asset SET isms_id = 'sys-hw-001' WHERE asset_tag = 'FW-02'",
        [],
    );
    assert!(dup.is_err(), "唯一索引為最後防線");
    let out_of_range = conn.execute(
        "UPDATE asset SET availability = 5 WHERE asset_tag = 'FW-02'",
        [],
    );
    assert!(out_of_range.is_err(), "C/I/A 限 1–4");
}
