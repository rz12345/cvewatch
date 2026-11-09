//! 16 任務 1.1：0007 版本的資料庫套用 0008_isms_admin（Scenario: Upgrading a database with
//! ISMS IDs）——owner 轉為保管人、依 ISMS 編號的系統代碼建立資產說明並歸屬，元件與命中不變。

use cvewatch_core::assets::systems::{SystemDirectory, SystemRepo};
use cvewatch_core::assets::{AssetRepo, AssetStatus};
use cvewatch_core::db::migrate::{
    current_version, latest_version, migrate, migrate_with, MIGRATIONS,
};
use rusqlite::{params, Connection};

const T0: &str = "2026-11-01T00:00:00+00:00";

fn v7_db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    let v7: Vec<_> = MIGRATIONS
        .iter()
        .copied()
        .filter(|m| m.version <= 7)
        .collect();
    assert_eq!(migrate_with(&mut conn, &v7).unwrap(), 7);
    conn.execute(
        "INSERT INTO catalog_product (key, display_name, vendor_display, version_scheme)
         VALUES ('paloalto-panos', 'PAN-OS', 'Palo Alto Networks', 'panos')",
        [],
    )
    .unwrap();
    conn
}

/// 以 0007 的欄位新增資產與一個產品元件，回傳元件 id。
fn old_asset(
    conn: &Connection,
    tag: &str,
    owner: Option<&str>,
    isms_id: Option<&str>,
    status: &str,
) -> i64 {
    conn.execute(
        "INSERT INTO asset (asset_tag, name, category, exposure, owner, isms_id, status,
                            first_seen, last_seen)
         VALUES (?1, ?1, 'HW03', 'internal', ?2, ?3, ?4, ?5, ?5)",
        params![tag, owner, isms_id, status, T0],
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

#[test]
fn upgrading_a_database_with_isms_ids() {
    let mut conn = v7_db();
    let fw = old_asset(&conn, "FW-01", Some("王小明"), Some("SYS-HW-003"), "active");
    for cve in ["CVE-2024-3400", "CVE-2025-0108"] {
        conn.execute(
            "INSERT INTO finding (component_id, cve_id, confidence, decided_by, priority,
                                  first_seen, last_evaluated)
             VALUES (?1, ?2, 'confirmed', 'cna', 'P2', ?3, ?3)",
            params![fw, cve, T0],
        )
        .unwrap();
    }
    old_asset(&conn, "NAS-02", None, Some("sys-hw-005"), "retired");
    old_asset(&conn, "NAS-01", Some("李小華"), None, "active");
    let before = (
        count(&conn, "SELECT count(*) FROM finding"),
        count(&conn, "SELECT count(*) FROM component"),
    );

    migrate(&mut conn).unwrap();
    assert_eq!(current_version(&conn).unwrap(), latest_version());

    let dir = SystemDirectory::load(&conn).unwrap();
    assert_eq!(dir.all().len(), 1, "大小寫不同的系統代碼合併為一個");
    assert_eq!(dir.label("SYS"), "SYS SYS", "名稱暫時等於代碼");
    let repo = AssetRepo::new(&conn);
    let fw = repo.find_by_tag("FW-01").unwrap().unwrap();
    assert_eq!(fw.record.system_code.as_deref(), Some("SYS"));
    assert_eq!(fw.record.custodian.as_deref(), Some("王小明"));
    assert_eq!(fw.record.responsible_unit, None);
    assert_eq!(fw.record.user_unit, None);
    assert_eq!(fw.record.compliance, None);
    assert!(!fw.record.personal_data);
    let nas2 = repo.find_by_tag("NAS-02").unwrap().unwrap();
    assert_eq!(nas2.status, AssetStatus::Retired);
    assert_eq!(
        nas2.record.system_code.as_deref(),
        Some("SYS"),
        "retired 資產一併歸屬"
    );
    let nas1 = repo.find_by_tag("NAS-01").unwrap().unwrap();
    assert_eq!(nas1.record.system_code, None);
    assert_eq!(nas1.record.custodian.as_deref(), Some("李小華"));
    assert_eq!(
        (
            count(&conn, "SELECT count(*) FROM finding"),
            count(&conn, "SELECT count(*) FROM component"),
        ),
        before,
        "命中與元件不變"
    );
    let usage = SystemRepo::new(&conn).usage_counts().unwrap();
    assert_eq!(usage[0].1, 2);
}

#[test]
fn system_constraints_are_enforced_by_the_schema() {
    let mut conn = v7_db();
    old_asset(&conn, "FW-01", None, None, "active");
    migrate(&mut conn).unwrap();
    conn.execute(
        "INSERT INTO asset_system (code, name) VALUES ('NET', '網路服務')",
        [],
    )
    .unwrap();
    let dup_name = conn.execute(
        "INSERT INTO asset_system (code, name) VALUES ('LAB', '網路服務')",
        [],
    );
    assert!(dup_name.is_err(), "名稱唯一");
    let unknown = conn.execute(
        "UPDATE asset SET system_code = 'LAB' WHERE asset_tag = 'FW-01'",
        [],
    );
    assert!(unknown.is_err(), "外鍵為最後防線");
    let compliance = conn.execute(
        "UPDATE asset SET compliance = 5 WHERE asset_tag = 'FW-01'",
        [],
    );
    assert!(compliance.is_err(), "法規遵循性限 1–4");
}
