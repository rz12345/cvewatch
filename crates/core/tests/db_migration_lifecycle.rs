//! 22 任務 2.1：0009 版本的資料庫套用 0010_lifecycle——既有資產（含 retired）的生命週期日期
//! 與覆寫為未填、命中數不變；endoflife.date 資料表可寫入並隨產品刪除。

use cvewatch_core::assets::{AssetRepo, AssetStatus, ComponentRepo};
use cvewatch_core::db::migrate::{
    current_version, latest_version, migrate, migrate_with, MIGRATIONS,
};
use rusqlite::{params, Connection};

const T0: &str = "2026-11-01T00:00:00+00:00";

fn v9_db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    let v9: Vec<_> = MIGRATIONS
        .iter()
        .copied()
        .filter(|m| m.version <= 9)
        .collect();
    assert_eq!(migrate_with(&mut conn, &v9).unwrap(), 9);
    conn.execute(
        "INSERT INTO catalog_product (key, display_name, vendor_display, version_scheme)
         VALUES ('paloalto-panos', 'PAN-OS', 'Palo Alto Networks', 'panos')",
        [],
    )
    .unwrap();
    conn
}

/// 以 0009 的欄位新增資產與一個產品元件，回傳元件 id。
fn old_asset(conn: &Connection, tag: &str, status: &str) -> i64 {
    conn.execute(
        "INSERT INTO asset (asset_tag, name, category, exposure, status, first_seen, last_seen,
                            brand, model)
         VALUES (?1, ?1, 'HW03', 'internal', ?2, ?3, ?3, 'Aruba', 'XYZ-100')",
        params![tag, status, T0],
    )
    .unwrap();
    let asset_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO component (asset_id, kind, product_id, version_raw, source, updated_at)
         VALUES (?1, 'product', (SELECT id FROM catalog_product), '10.2.9', 'excel', ?2)",
        params![asset_id, T0],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn upgraded() -> Connection {
    let mut conn = v9_db();
    let fw = old_asset(&conn, "FW-EDGE", "active");
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
fn upgrading_keeps_assets_unchanged() {
    let conn = upgraded();
    assert_eq!(current_version(&conn).unwrap(), latest_version());
    let repo = AssetRepo::new(&conn);
    for tag in ["FW-EDGE", "NAS-02"] {
        let a = repo.find_by_tag(tag).unwrap().unwrap();
        let r = &a.record;
        assert_eq!(
            (r.warranty_end, r.support_end, r.license_end),
            (None, None, None),
            "{tag}"
        );
        assert_eq!(r.hw_eol_override, None, "{tag}");
        assert_eq!(r.brand.as_deref(), Some("Aruba"), "{tag}：既有欄位不變");
        let components = ComponentRepo::new(&conn).for_asset(a.id).unwrap();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].eol_override, None);
        assert_eq!(components[0].version_raw, "10.2.9");
    }
    assert_eq!(
        repo.find_by_tag("NAS-02").unwrap().unwrap().status,
        AssetStatus::Retired
    );
    assert_eq!(
        count(&conn, "SELECT count(*) FROM finding"),
        2,
        "命中數不變"
    );
    assert_eq!(count(&conn, "SELECT count(*) FROM eol_product"), 0);
}

#[test]
fn eol_tables_accept_rows_and_cascade() {
    let conn = upgraded();
    conn.execute(
        "INSERT INTO eol_product (slug, label, status, fetched_at)
         VALUES ('panos', 'PAN-OS', 'ok', ?1)",
        [T0],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO eol_release (slug, name, is_eol, eol_from, position)
         VALUES ('panos', '10.2', 0, '2027-03-01', 0)",
        [],
    )
    .unwrap();
    let bad = conn.execute(
        "INSERT INTO eol_product (slug, status, fetched_at) VALUES ('x', 'gone', ?1)",
        [T0],
    );
    assert!(bad.is_err(), "status 只允許 ok／not_found");
    conn.execute("DELETE FROM eol_product WHERE slug = 'panos'", [])
        .unwrap();
    assert_eq!(count(&conn, "SELECT count(*) FROM eol_release"), 0);
}
