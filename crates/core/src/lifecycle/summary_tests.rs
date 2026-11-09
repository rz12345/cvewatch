//! [`super`] 的單元測試：retired 資產不提醒、期間內進入提醒天數與期間內到期的邊界。

use chrono::{NaiveDate, Utc};
use rusqlite::Connection;

use super::*;
use crate::assets::validate::{validate_import_record, RecordInput};
use crate::assets::{AssetClasses, AssetRecord, EolOverride};
use crate::db::migrate::migrate;

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn record(tag: &str) -> AssetRecord {
    validate_import_record(
        &RecordInput {
            asset_tag: tag,
            name: tag,
            category: "HW01",
            ..RecordInput::default()
        },
        &AssetClasses::builtin(),
    )
    .unwrap()
    .record
}

fn db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    migrate(&mut conn).unwrap();
    conn
}

fn hw(date: NaiveDate) -> Option<EolOverride> {
    Some(EolOverride {
        date,
        source: "x".into(),
    })
}

/// Scenario: Retired asset is not reminded
#[test]
fn retired_asset_is_not_reminded() {
    let conn = db();
    let repo = AssetRepo::new(&conn);
    let past = ymd(2026, 1, 1);
    let nas = AssetRecord {
        hw_eol_override: hw(past),
        ..record("NAS-02")
    };
    let id = repo.insert(&nas, Utc::now()).unwrap();
    repo.retire(id, Utc::now()).unwrap();
    let sw = AssetRecord {
        hw_eol_override: hw(past),
        ..record("SW-CORE")
    };
    repo.insert(&sw, Utc::now()).unwrap();
    let catalog = Catalog::seed().unwrap();
    let counts = summary(&conn, &catalog, ymd(2027, 1, 10), 90).unwrap();
    assert_eq!(
        counts,
        LifecycleCounts {
            eol: 1,
            eol_soon: 0,
            expiry: 0,
            window: 90
        }
    );
    let assets = evaluate_active(&conn, &catalog, ymd(2027, 1, 10), 90).unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].0.record.asset_tag, "SW-CORE");
}

fn assets_with(
    records: Vec<AssetRecord>,
    today: NaiveDate,
    window: u32,
) -> Vec<(Asset, AssetLifecycle)> {
    let conn = db();
    let repo = AssetRepo::new(&conn);
    for r in &records {
        repo.insert(r, Utc::now()).unwrap();
    }
    evaluate_active(&conn, &Catalog::seed().unwrap(), today, window).unwrap()
}

/// Scenario: A support contract enters the reminder window（剛好第 90 天）
#[test]
fn entering_the_window_on_the_last_day_of_the_period() {
    let srv = AssetRecord {
        support_end: Some(ymd(2027, 4, 10)),
        ..record("SRV-01")
    };
    let until = ymd(2027, 1, 10);
    let assets = assets_with(vec![srv], until, 90);
    let d = digest(&assets, ymd(2027, 1, 9), until, 90);
    assert!(d.news);
    assert_eq!(d.upcoming.len(), 1);
    assert_eq!(d.upcoming[0].days_left, 90);
    assert_eq!(d.upcoming[0].subject, "維護合約到期");
    assert!(!d.upcoming[0].is_eol);
    // 期間起點當天進入提醒天數不算本期（期間為 (since, until]）
    let d = digest(&assets, ymd(2027, 1, 10), ymd(2027, 1, 10), 90);
    assert!(!d.news, "已在先前的摘要提醒過");
    assert_eq!(d.upcoming.len(), 1, "仍列出");
}

/// Scenario: Reminder already sent before
#[test]
fn reminder_already_sent_before_is_not_news() {
    let fw = AssetRecord {
        hw_eol_override: hw(ymd(2027, 3, 1)),
        ..record("FW-EDGE")
    };
    let until = ymd(2027, 1, 10);
    let assets = assets_with(vec![fw], until, 90);
    let d = digest(&assets, ymd(2027, 1, 9), until, 90);
    assert!(!d.news);
    assert_eq!(d.upcoming.len(), 1);
    assert_eq!(d.upcoming[0].days_left, 50);
}

/// Scenario: Hardware reached end of life during the period（期間終點當天到期）
#[test]
fn reached_during_the_period() {
    let sw = AssetRecord {
        hw_eol_override: hw(ymd(2027, 1, 10)),
        ..record("SW-CORE")
    };
    let old = AssetRecord {
        warranty_end: Some(ymd(2027, 1, 9)),
        ..record("NAS-01")
    };
    let until = ymd(2027, 1, 10);
    let assets = assets_with(vec![sw, old], until, 90);
    let d = digest(&assets, ymd(2027, 1, 9), until, 90);
    assert!(d.news);
    assert_eq!(d.reached.len(), 1, "期間起點當天到期的不算本期");
    assert_eq!(d.reached[0].asset_tag, "SW-CORE");
    assert!(d.reached[0].is_eol);
    assert_eq!(d.eol_assets, 1);
    assert_eq!(d.expired_assets, 1);
    assert!(!d.is_empty());
}

#[test]
fn nothing_to_report() {
    let assets = assets_with(vec![record("SRV-01")], ymd(2027, 1, 10), 90);
    let d = digest(&assets, ymd(2027, 1, 9), ymd(2027, 1, 10), 90);
    assert!(d.is_empty());
    assert!(!d.news);
}
