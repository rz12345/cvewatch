//! 22 任務 2.3：生命週期日期與 EoL 覆寫的讀寫往返；元件移除時覆寫清除、修改版本時保留。

mod common;

use chrono::{NaiveDate, Utc};
use cvewatch_core::assets::{
    AssetRecord, AssetRepo, Cia, ClassCode, ComponentRepo, EolOverride, Exposure,
};
use cvewatch_core::db::open_working_db;
use rusqlite::Connection;

fn product(conn: &Connection, key: &str) -> i64 {
    conn.execute(
        "INSERT INTO catalog_product (key, display_name, vendor_display, version_scheme)
         VALUES (?1, ?1, 'v', 'dotted')",
        [key],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn record(tag: &str) -> AssetRecord {
    AssetRecord {
        asset_tag: tag.to_string(),
        name: format!("{tag}-host"),
        category: ClassCode::parse("HW05").unwrap(),
        location: None,
        exposure: Exposure::Internal,
        cia: Cia::new(2, 3, 4),
        isms_id: None,
        system_code: None,
        responsible_unit: None,
        custodian: None,
        user_unit: None,
        compliance: None,
        personal_data: false,
        brand: None,
        model: None,
        serial_number: None,
        property_number: None,
        hostname: None,
        management_url: None,
        warranty_end: None,
        support_end: None,
        license_end: None,
        hw_eol_override: None,
        notes: None,
    }
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn eol(date: NaiveDate, source: &str) -> EolOverride {
    EolOverride {
        date,
        source: source.into(),
    }
}

fn pair(conn: &Connection, sql: &str, id: i64) -> (Option<String>, Option<String>) {
    conn.query_row(sql, [id], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
}

#[test]
fn dates_and_overrides_round_trip() {
    let env = common::initialized();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    let assets = AssetRepo::new(&conn);
    let r = AssetRecord {
        warranty_end: Some(ymd(2027, 12, 31)),
        support_end: Some(ymd(2027, 6, 30)),
        license_end: None,
        hw_eol_override: Some(eol(ymd(2027, 6, 30), "原廠延長支援函")),
        ..record("SW-CORE")
    };
    let id = assets.insert(&r, Utc::now()).unwrap();
    assert_eq!(assets.find_by_tag("SW-CORE").unwrap().unwrap().record, r);
    let sql = "SELECT hw_eol_override, hw_eol_source FROM asset WHERE id = ?1";
    assert_eq!(
        pair(&conn, sql, id),
        (Some("2027-06-30".into()), Some("原廠延長支援函".into()))
    );
    let warranty: String = conn
        .query_row(
            "SELECT warranty_end FROM asset WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(warranty, "2027-12-31");

    let cleared = AssetRecord {
        hw_eol_override: None,
        support_end: None,
        ..r
    };
    assets.update_record(id, &cleared, Utc::now()).unwrap();
    assert_eq!(
        assets.find_by_tag("SW-CORE").unwrap().unwrap().record,
        cleared
    );
    assert_eq!(pair(&conn, sql, id), (None, None), "覆寫兩欄一起清除");
}

#[test]
fn component_override_follows_version_but_not_removal() {
    let env = common::initialized();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    let esxi = product(&conn, "vmware-esxi");
    let id = AssetRepo::new(&conn)
        .insert(&record("SRV-01"), Utc::now())
        .unwrap();
    let components = ComponentRepo::new(&conn);
    components
        .upsert_product(id, esxi, "8.0 U2", None, Utc::now())
        .unwrap();
    let c = components.for_asset(id).unwrap().remove(0);
    let value = eol(ymd(2028, 12, 31), "延長支援合約");
    components.set_eol_override(c.id, Some(&value)).unwrap();
    components
        .upsert_product(id, esxi, "8.0 U3", None, Utc::now())
        .unwrap();
    let c = components.for_asset(id).unwrap().remove(0);
    assert_eq!(c.version_raw, "8.0 U3");
    assert_eq!(c.eol_override, Some(value.clone()), "版本修改保留覆寫");

    components.remove(c.id, Utc::now()).unwrap();
    let sql = "SELECT eol_override, eol_override_source FROM component WHERE id = ?1";
    assert_eq!(
        pair(&conn, sql, c.id),
        (None, None),
        "元件移除時覆寫一併移除"
    );
    components
        .upsert_product(id, esxi, "8.0 U3", None, Utc::now())
        .unwrap();
    let restored = components.for_asset(id).unwrap().remove(0);
    assert_eq!(restored.eol_override, None, "恢復的元件沒有覆寫");

    components
        .set_product_eol_override(id, esxi, Some(&value))
        .unwrap();
    assert_eq!(
        components.for_asset(id).unwrap()[0].eol_override,
        Some(value)
    );
}
