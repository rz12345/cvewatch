//! 22 任務 7.1：每日摘要的「生命週期提醒」（email-alerts 的 Lifecycle reminders in the daily
//! digest）——期間內進入提醒天數、先前已提醒、期間內到期，以及沒有項目時省略段落。

mod common;

use chrono::FixedOffset;
use common::matching::{at, world, World};
use cvewatch_core::notify::content::{self, Zone};
use cvewatch_core::notify::digest::{self, DigestData, LifecycleInput};

fn zone() -> Zone {
    Zone::Fixed(FixedOffset::east_opt(8 * 3600).unwrap())
}

/// 摘要涵蓋 2027-01-09 至 2027-01-10（UTC+8 的日曆日）。
fn collect(w: &World) -> DigestData {
    digest::collect(
        &w.conn,
        at("2027-01-09T00:00:00+08:00"),
        at("2027-01-10T08:00:00+08:00"),
        14,
        Some(LifecycleInput {
            catalog: &w.catalog,
            window: 90,
            zone: zone(),
        }),
    )
    .unwrap()
}

fn body(w: &World, data: &DigestData) -> (String, String) {
    let mail = content::digest_mail(data, &content::product_names(&w.conn).unwrap(), zone());
    (mail.subject, mail.body)
}

fn set(w: &World, tag: &str, column: &str, value: &str) {
    w.conn
        .execute(
            &format!("UPDATE asset SET {column} = ?2 WHERE asset_tag = ?1"),
            [tag, value],
        )
        .unwrap();
}

/// Scenario: A support contract enters the reminder window
#[test]
fn a_support_contract_enters_the_reminder_window() {
    let w = world();
    w.asset("SRV-01", "internal", &[("vmware-esxi", "")]);
    set(&w, "SRV-01", "support_end", "2027-04-10");
    let data = collect(&w);
    assert!(data.lifecycle.news);
    assert!(!data.is_empty(), "算新事項");
    let (subject, body) = body(&w, &data);
    assert!(!subject.ends_with("無新事項"), "{subject}");
    assert!(body.contains("■ 生命週期提醒（提前 90 天）"), "{body}");
    assert!(
        body.contains("SRV-01 維護合約到期 2027-04-10（剩 90 天）"),
        "{body}"
    );
}

/// Scenario: Reminder already sent before
#[test]
fn reminder_already_sent_before() {
    let w = world();
    w.asset("FW-EDGE", "internal", &[("paloalto-panos", "10.2.9")]);
    w.conn
        .execute_batch(
            "INSERT INTO eol_product (slug, status, fetched_at)
             VALUES ('panos', 'ok', '2027-01-01T00:00:00+00:00');
             INSERT INTO eol_release (slug, name, is_eol, eol_from, position)
             VALUES ('panos', '10.2', 0, '2027-03-01', 0);",
        )
        .unwrap();
    let data = collect(&w);
    assert!(!data.lifecycle.news);
    assert!(data.is_empty(), "其他生命週期內容不算新事項");
    let (subject, body) = body(&w, &data);
    assert!(subject.ends_with("無新事項"), "{subject}");
    assert!(body.contains("無新事項。"), "{body}");
    assert!(
        body.contains("FW-EDGE PAN-OS 10.2.9（分支 10.2）EoL 2027-03-01（剩 50 天）"),
        "{body}"
    );
}

/// Scenario: Hardware reached end of life during the period
#[test]
fn hardware_reached_end_of_life_during_the_period() {
    let w = world();
    w.asset("SW-CORE", "internal", &[("hpe-aoscx", "")]);
    set(&w, "SW-CORE", "hw_eol_override", "2027-01-10");
    set(&w, "SW-CORE", "hw_eol_source", "原廠公告");
    let data = collect(&w);
    assert!(data.lifecycle.news);
    let (_, body) = body(&w, &data);
    assert!(
        body.contains("SW-CORE 硬體 EoL 已於 2027-01-10 到期"),
        "{body}"
    );
    assert!(
        body.contains("目前已 EoL 1 個資產、已到期 0 個資產"),
        "{body}"
    );
}

#[test]
fn section_is_omitted_without_items() {
    let w = world();
    w.asset("SRV-01", "internal", &[("vmware-esxi", "")]);
    let data = collect(&w);
    assert!(data.lifecycle.is_empty());
    let (_, body) = body(&w, &data);
    assert!(!body.contains("生命週期提醒"), "{body}");
}
