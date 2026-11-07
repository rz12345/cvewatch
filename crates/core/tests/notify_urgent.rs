//! 06 任務 2.3：緊急事件判定（spec email-alerts「Urgent findings definition」「No duplicate
//! notifications」與 redmine-integration「Non-urgent finding」），含 probable 命中。

mod common;

use common::matching::{affected, at, panos, world, World};
use cvewatch_core::matching::triage::{self, ChangeRequest};
use cvewatch_core::notify::outbox::Channel;
use cvewatch_core::notify::urgent::{self, EventKind};
use serde_json::json;

const CRITICAL: &str = "CVE-2026-1001";
const HIGH: &str = "CVE-2026-1002";
const UNKNOWN: &str = "CVE-2026-1003";
const THRESHOLD: f64 = 9.0;

fn affected_panos(w: &World, cve: &str) {
    w.cve(
        cve,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
}

fn events(w: &World, channel: Channel) -> Vec<(EventKind, String, String)> {
    urgent::detect(&w.conn, channel, THRESHOLD)
        .unwrap()
        .into_iter()
        .map(|e| (e.kind, e.finding.cve_id, e.finding.asset_tag))
        .collect()
}

fn mark_notified(w: &World, channel: Channel, now: &str) {
    let found = urgent::detect(&w.conn, channel, THRESHOLD).unwrap();
    urgent::record(&w.conn, channel, &found, None, at(now)).unwrap();
}

#[test]
fn new_critical_finding_is_urgent() {
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")]);
    affected_panos(&w, CRITICAL);
    w.cvss(CRITICAL, 9.8);
    w.run_at("2026-11-07T00:00:00+00:00");
    assert_eq!(
        events(&w, Channel::Email),
        vec![(EventKind::NewUrgent, CRITICAL.into(), "FW-01".into())]
    );
}

#[test]
fn non_urgent_finding_produces_no_event() {
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")]);
    affected_panos(&w, HIGH);
    w.cvss(HIGH, 7.5);
    w.run();
    assert!(events(&w, Channel::Email).is_empty());
    assert!(events(&w, Channel::Redmine).is_empty());
}

#[test]
fn existing_finding_becoming_kev_is_an_urgent_event() {
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")]);
    affected_panos(&w, HIGH);
    w.cvss(HIGH, 7.5);
    w.run_at("2026-10-24T00:00:00+00:00");
    assert!(events(&w, Channel::Email).is_empty(), "兩週前不緊急");
    w.kev(HIGH, "2026-11-07");
    w.run_at("2026-11-07T00:00:00+00:00");
    assert_eq!(
        events(&w, Channel::Email),
        vec![(EventKind::BecameKev, HIGH.into(), "FW-01".into())]
    );
}

#[test]
fn notified_critical_finding_later_listed_in_kev_notifies_once_more() {
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")]);
    affected_panos(&w, CRITICAL);
    w.cvss(CRITICAL, 9.8);
    w.run_at("2026-11-01T00:00:00+00:00");
    mark_notified(&w, Channel::Email, "2026-11-01T00:00:00+00:00");
    assert!(events(&w, Channel::Email).is_empty());
    w.kev(CRITICAL, "2026-11-07");
    w.run_at("2026-11-07T00:00:00+00:00");
    assert_eq!(
        events(&w, Channel::Email),
        vec![(EventKind::BecameKev, CRITICAL.into(), "FW-01".into())]
    );
    mark_notified(&w, Channel::Email, "2026-11-07T00:00:00+00:00");
    assert!(events(&w, Channel::Email).is_empty(), "KEV 事件只一次");
}

#[test]
fn next_sync_without_changes_sends_nothing_and_channels_are_independent() {
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")]);
    w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.3")]);
    affected_panos(&w, CRITICAL);
    w.cvss(CRITICAL, 9.8);
    w.run();
    assert_eq!(
        events(&w, Channel::Email).len(),
        2,
        "一次 sync 兩個緊急事件"
    );
    mark_notified(&w, Channel::Email, "2026-11-01T00:00:00+00:00");
    w.run_at("2026-11-01T04:00:00+00:00");
    assert!(events(&w, Channel::Email).is_empty());
    assert_eq!(events(&w, Channel::Redmine).len(), 2, "Redmine 另行去重");
}

#[test]
fn probable_finding_is_included_and_flagged() {
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.4")]);
    w.cve(
        UNKNOWN,
        json!([affected(
            "Palo Alto Networks",
            "PAN-OS",
            None,
            json!([{"version": "10.2.0", "lessThan": "10.2.9", "status": "affected"}])
        )]),
        json!([]),
    );
    w.kev(UNKNOWN, "2026-10-01");
    w.run();
    let found = urgent::detect(&w.conn, Channel::Email, THRESHOLD).unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].is_probable());
    assert_eq!(found[0].kind, EventKind::NewUrgent, "KEV 早於命中建立");
    assert_eq!(found[0].kev.as_ref().unwrap().date_added, "2026-10-01");
}

#[test]
fn handled_findings_and_retired_assets_are_not_urgent() {
    let mut w = world();
    let c = w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")])[0];
    w.asset("FW-OLD", "internal", &[("paloalto-panos", "11.1.2")]);
    affected_panos(&w, CRITICAL);
    w.cvss(CRITICAL, 9.8);
    w.run();
    w.retire("FW-OLD");
    let id = w.finding(c, CRITICAL).unwrap().id;
    let change = triage::validate(&ChangeRequest {
        status: "resolved".into(),
        ..ChangeRequest::default()
    })
    .unwrap();
    triage::set_status(&w.conn, id, &change, at("2026-11-01T01:00:00+00:00")).unwrap();
    assert!(events(&w, Channel::Email).is_empty());
}

#[test]
fn threshold_is_configurable() {
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")]);
    affected_panos(&w, HIGH);
    w.cvss(HIGH, 7.5);
    w.run();
    assert_eq!(
        urgent::detect(&w.conn, Channel::Email, 7.0).unwrap().len(),
        1
    );
}
