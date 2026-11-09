//! 06 任務 4.5：Redmine 開案、新資產註記、狀態變更註記、502 重送、422 dead、http 網址拒絕、
//! 自簽 CA 經 `ca_file` 驗證，以及清單載入與連線測試。
//!
//! Redmine 一律走 https（http 會被拒絕），因此以本機 rustls HTTPS 替身取代 wiremock（後者只提供明文 HTTP）。

mod common;
#[path = "common/notify.rs"]
mod support;

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Duration;
use common::matching::{at, panos, world, World};
use cvewatch_core::config::HttpConfig;
use cvewatch_core::matching::triage::{self, ChangeRequest};
use cvewatch_core::notify::check;
use cvewatch_core::notify::content::Zone;
use cvewatch_core::notify::dispatch::{Endpoint, Notifier};
use cvewatch_core::notify::outbox;
use cvewatch_core::notify::redmine::{RedmineClient, RedmineSettings};
use cvewatch_core::notify::settings::{NotifyConfig, RedmineConfig};
use cvewatch_core::notify::triage_notes;
use serde_json::{json, Value};
use support::{https_server, pki, Requests, TestPki};
use zeroize::Zeroizing;

const KEV_CVE: &str = "CVE-2026-4001";
const HIGH_CVE: &str = "CVE-2026-4002";
const API_KEY: &str = "k3y-for-tests";
const T1: &str = "2026-11-07T00:00:00+00:00";

struct Env {
    pki: TestPki,
    _dir: tempfile::TempDir,
    ca: PathBuf,
}

fn env() -> Env {
    let pki = pki();
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, &pki.ca_pem).unwrap();
    Env { pki, _dir: dir, ca }
}

fn redmine_cfg(port: u16) -> RedmineConfig {
    RedmineConfig {
        enabled: true,
        base_url: format!("https://localhost:{port}"),
        project_id: "12".into(),
        tracker_id: 3,
        priority_map: [("P1".to_string(), 5)].into_iter().collect(),
        assigned_to_id: 0,
        add_note_on_triage: true,
    }
}

fn notify_cfg(port: u16, ca: Option<&PathBuf>) -> NotifyConfig {
    NotifyConfig {
        ca_file: ca.cloned(),
        redmine: redmine_cfg(port),
        ..NotifyConfig::default()
    }
}

fn client(cfg: &NotifyConfig) -> Result<RedmineClient, String> {
    RedmineClient::new(&RedmineSettings {
        redmine: &cfg.redmine,
        api_key: Some(Zeroizing::new(API_KEY.into())),
        ca_file: cfg.ca_file(),
        allow_insecure: cfg.allow_insecure_transport,
        http: &HttpConfig::default(),
    })
}

fn notifier(e: &Env, script: Vec<(u16, String)>) -> (Notifier, Requests) {
    let (port, seen) = https_server(Arc::clone(&e.pki.server), script);
    let cfg = notify_cfg(port, Some(&e.ca));
    let api = client(&cfg).unwrap();
    let n = Notifier {
        config: cfg,
        stale_days: 14,
        lifecycle: None,
        zone: Zone::Local,
        mail: Endpoint::Disabled,
        redmine: Endpoint::Ready(Box::new(api)),
    };
    (n, seen)
}

fn created(id: u64) -> (u16, String) {
    (201, json!({"issue": {"id": id}}).to_string())
}

fn kev_world() -> World {
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")]);
    w.cve(
        KEV_CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.kev(KEV_CVE, "2026-10-20");
    w.cvss(KEV_CVE, 9.8);
    w.run();
    w
}

fn body(seen: &Requests, i: usize) -> Value {
    serde_json::from_str(&seen.lock().unwrap()[i].body).unwrap()
}

fn issue_of(w: &World, cve: &str) -> Option<i64> {
    w.conn
        .query_row(
            "SELECT issue_id FROM redmine_issue WHERE cve_id = ?1",
            [cve],
            |r| r.get(0),
        )
        .ok()
}

#[test]
fn urgent_kev_cve_creates_one_issue_then_new_asset_adds_note() {
    let e = env();
    let mut w = kev_world();
    let (n, seen) = notifier(&e, vec![created(42), (204, String::new())]);
    let (queued, stats) = n.after_sync(&mut w.conn, at(T1)).unwrap();
    assert_eq!((queued.issues, stats.sent), (1, 1));
    assert_eq!(issue_of(&w, KEV_CVE), Some(42));
    {
        let req = &seen.lock().unwrap()[0];
        assert_eq!(req.line, "POST /issues.json HTTP/1.1");
        assert_eq!(req.api_key.as_deref(), Some(API_KEY));
    }
    let issue = &body(&seen, 0)["issue"];
    assert_eq!(
        issue["subject"],
        format!("[cvewatch][P1][KEV] {KEV_CVE} PAN-OS — FW-01")
    );
    assert_eq!(issue["project_id"], 12);
    assert_eq!(issue["tracker_id"], 3);
    assert_eq!(issue["priority_id"], 5);
    let desc = issue["description"].as_str().unwrap();
    assert!(desc.contains("| P1 | FW-01 | PAN-OS 11.1.2 |"), "{desc}");
    assert!(desc.contains("KEV：已列入（2026-10-20"), "{desc}");

    // 之後 FW-02 也受影響：在原議題加註記，不建立第二張
    w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.3")]);
    w.run_at("2026-11-07T04:00:00+00:00");
    let (queued, stats) = n
        .after_sync(&mut w.conn, at("2026-11-07T04:00:00+00:00"))
        .unwrap();
    assert_eq!((queued.issues, queued.notes, stats.sent), (0, 1, 1));
    assert_eq!(seen.lock().unwrap()[1].line, "PUT /issues/42.json HTTP/1.1");
    let notes = body(&seen, 1)["issue"]["notes"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(notes.contains("FW-02：PAN-OS 11.1.3"), "{notes}");
}

#[test]
fn non_urgent_finding_creates_no_issue() {
    let e = env();
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")]);
    w.cve(
        HIGH_CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.cvss(HIGH_CVE, 7.5);
    w.run();
    let (n, seen) = notifier(&e, vec![created(1)]);
    let (queued, _) = n.after_sync(&mut w.conn, at(T1)).unwrap();
    assert_eq!(queued.issues, 0);
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn bad_gateway_is_retried_and_issue_recorded_after_success() {
    let e = env();
    let mut w = kev_world();
    let none = json!({"issues": []}).to_string();
    let (n, seen) = notifier(&e, vec![(502, String::new()), (200, none), created(7)]);
    let (_, stats) = n.after_sync(&mut w.conn, at(T1)).unwrap();
    assert_eq!((stats.sent, stats.retrying), (0, 1));
    assert_eq!(issue_of(&w, KEV_CVE), None);
    // 下次執行（5 分鐘後到期）不重複排入；先確認 Redmine 沒有既有議題再重送
    let later = at(T1) + Duration::minutes(5);
    let (queued, stats) = n.after_sync(&mut w.conn, later).unwrap();
    assert_eq!((queued.issues, stats.sent), (0, 1));
    assert_eq!(issue_of(&w, KEV_CVE), Some(7));
    let lines: Vec<String> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.line.clone())
        .collect();
    assert_eq!(lines.len(), 3);
    assert!(
        lines[1].starts_with("GET /issues.json?project_id=12&status_id=*"),
        "{}",
        lines[1]
    );
    assert!(lines[1].contains(KEV_CVE));
    assert_eq!(lines[2], "POST /issues.json HTTP/1.1");
}

#[test]
fn invalid_tracker_is_dead_and_listed_with_redmine_message() {
    let e = env();
    let mut w = kev_world();
    let err = json!({"errors": ["Tracker is not included in the list"]}).to_string();
    let (n, _) = notifier(&e, vec![(422, err)]);
    let (_, stats) = n.after_sync(&mut w.conn, at(T1)).unwrap();
    assert_eq!(stats.dead, 1);
    let items = outbox::list(&w.conn).unwrap();
    assert!(items[0].is_dead());
    let last = items[0].last_error.as_deref().unwrap();
    assert!(
        last.contains("422") && last.contains("Tracker is not included"),
        "{last}"
    );
}

#[test]
fn triage_adds_note_without_changing_issue_status() {
    let e = env();
    let mut w = kev_world();
    let (n, seen) = notifier(&e, vec![created(42), (204, String::new())]);
    n.after_sync(&mut w.conn, at(T1)).unwrap();
    let id: i64 = w
        .conn
        .query_row("SELECT id FROM finding WHERE cve_id = ?1", [KEV_CVE], |r| {
            r.get(0)
        })
        .unwrap();
    let change = triage::validate(&ChangeRequest {
        status: "resolved".into(),
        note: "已升至 11.1.6".into(),
        ..ChangeRequest::default()
    })
    .unwrap();
    triage_notes::set_status_many(&mut w.conn, &n.config, &[id], &change, at(T1), true).unwrap();
    let stats = n.flush(&w.conn, at(T1)).unwrap();
    assert_eq!(stats.sent, 1);
    let req = body(&seen, 1);
    assert_eq!(
        req["issue"]["notes"],
        "cvewatch 狀態變更：\nFW-01：resolved（已升至 11.1.6）"
    );
    assert!(req["issue"].get("status_id").is_none(), "不變更議題狀態");
}

#[test]
fn triage_without_issue_or_with_notes_disabled_queues_nothing() {
    let mut w = kev_world();
    let mut cfg = notify_cfg(443, None);
    let id: i64 = w
        .conn
        .query_row("SELECT id FROM finding WHERE cve_id = ?1", [KEV_CVE], |r| {
            r.get(0)
        })
        .unwrap();
    let change = triage::validate(&ChangeRequest {
        status: "resolved".into(),
        ..ChangeRequest::default()
    })
    .unwrap();
    triage_notes::set_status_many(&mut w.conn, &cfg, &[id], &change, at(T1), true).unwrap();
    assert!(outbox::list(&w.conn).unwrap().is_empty(), "此 CVE 沒有議題");
    w.conn
        .execute(
            "INSERT INTO redmine_issue (cve_id, issue_id, created_at) VALUES (?1, 42, ?2)",
            [KEV_CVE, T1],
        )
        .unwrap();
    cfg.redmine.add_note_on_triage = false;
    let reopen = triage::validate(&ChangeRequest {
        status: "open".into(),
        ..ChangeRequest::default()
    })
    .unwrap();
    triage_notes::set_status_many(&mut w.conn, &cfg, &[id], &reopen, at(T1), true).unwrap();
    assert!(outbox::list(&w.conn).unwrap().is_empty(), "設定關閉");
}

#[test]
fn http_url_is_rejected_and_never_sends_api_key() {
    let mut cfg = notify_cfg(80, None);
    cfg.redmine.base_url = "http://redmine.local".into();
    let err = client(&cfg).err().unwrap();
    assert!(err.contains("https"), "{err}");
    cfg.allow_insecure_transport = true;
    let err = client(&cfg).err().unwrap();
    assert!(err.contains("拒絕送出"), "{err}");
}

#[test]
fn self_signed_ca_is_trusted_only_with_ca_file() {
    let e = env();
    let project = json!({"project": {"id": 12, "name": "基礎設施資安",
        "trackers": [{"id": 1, "name": "Bug"}, {"id": 3, "name": "漏洞"}]}});
    let (port, seen) = https_server(Arc::clone(&e.pki.server), vec![(200, project.to_string())]);
    let http = HttpConfig::default();
    let key = || Some(Zeroizing::new(API_KEY.to_string()));
    let report = check::test_redmine(&notify_cfg(port, Some(&e.ca)), key(), &http).unwrap();
    assert_eq!(report.project.label(), "基礎設施資安 (#12)");
    assert_eq!(report.tracker.label(), "漏洞 (#3)");
    assert_eq!(
        seen.lock().unwrap()[0].line,
        "GET /projects/12.json?include=trackers HTTP/1.1"
    );
    let err = check::test_redmine(&notify_cfg(port, None), key(), &http).unwrap_err();
    assert!(err.contains("憑證"), "{err}");
}

#[test]
fn test_reports_tracker_not_enabled_in_project() {
    let e = env();
    let project = json!({"project": {"id": 12, "name": "基礎設施資安", "trackers": [{"id": 1, "name": "Bug"}]}});
    let (port, _) = https_server(Arc::clone(&e.pki.server), vec![(200, project.to_string())]);
    let err = check::test_redmine(
        &notify_cfg(port, Some(&e.ca)),
        Some(Zeroizing::new(API_KEY.into())),
        &HttpConfig::default(),
    )
    .unwrap_err();
    assert!(
        err.contains("未啟用 tracker #3") && err.contains("Bug (#1)"),
        "{err}"
    );
}

#[test]
fn lists_follow_pagination_and_include_groups() {
    let e = env();
    let page1 =
        json!({"projects": [{"id": 1, "name": "A"}, {"id": 2, "name": "B"}], "total_count": 3});
    let page2 = json!({"projects": [{"id": 12, "name": "基礎設施資安"}], "total_count": 3});
    let members = json!({"memberships": [
        {"user": {"id": 5, "name": "Rz Chen"}},
        {"group": {"id": 9, "name": "資安小組"}}
    ], "total_count": 2});
    let priorities =
        json!({"issue_priorities": [{"id": 4, "name": "高"}, {"id": 5, "name": "緊急"}]});
    let (port, seen) = https_server(
        Arc::clone(&e.pki.server),
        vec![
            (200, page1.to_string()),
            (200, page2.to_string()),
            (200, members.to_string()),
            (200, priorities.to_string()),
        ],
    );
    let api = client(&notify_cfg(port, Some(&e.ca))).unwrap();
    let projects = api.projects().unwrap();
    assert_eq!(projects.len(), 3);
    assert!(seen.lock().unwrap()[1].line.contains("offset=2"));
    let members = api.members("12").unwrap();
    let labels: Vec<String> = members.iter().map(|m| m.label()).collect();
    assert_eq!(labels, ["Rz Chen (#5)", "資安小組 (#9)"]);
    assert_eq!(api.priorities().unwrap().len(), 2);
}

#[test]
fn deleted_issue_note_is_dead_and_next_event_opens_new_issue() {
    let e = env();
    let mut w = kev_world();
    let (n, seen) = notifier(&e, vec![created(42), (404, String::new()), created(43)]);
    n.after_sync(&mut w.conn, at(T1)).unwrap();
    // FW-02 受影響 → 註記，但議題 #42 已被刪除
    w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.3")]);
    w.run_at("2026-11-07T04:00:00+00:00");
    let (_, stats) = n
        .after_sync(&mut w.conn, at("2026-11-07T04:00:00+00:00"))
        .unwrap();
    assert_eq!(stats.dead, 1, "404 不重試");
    assert_eq!(issue_of(&w, KEV_CVE), None, "已清除對應");
    // 之後再有資產受影響：重新開案
    w.asset("FW-03", "internal", &[("paloalto-panos", "11.1.4")]);
    w.run_at("2026-11-07T08:00:00+00:00");
    let (queued, _) = n
        .after_sync(&mut w.conn, at("2026-11-07T08:00:00+00:00"))
        .unwrap();
    assert_eq!(queued.issues, 1);
    assert_eq!(issue_of(&w, KEV_CVE), Some(43));
    assert_eq!(seen.lock().unwrap()[2].line, "POST /issues.json HTTP/1.1");
}

#[test]
fn redirect_is_not_followed() {
    let e = env();
    let (port, seen) = https_server(Arc::clone(&e.pki.server), vec![(302, String::new())]);
    let err = check::test_redmine(
        &notify_cfg(port, Some(&e.ca)),
        Some(Zeroizing::new(API_KEY.into())),
        &HttpConfig::default(),
    )
    .unwrap_err();
    assert!(err.contains("302") && err.contains("轉址"), "{err}");
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn created_issue_is_not_posted_again_when_recording_fails() {
    let e = env();
    let mut w = kev_world();
    w.conn
        .execute_batch(
            "CREATE TRIGGER no_record BEFORE INSERT ON redmine_issue
             BEGIN SELECT RAISE(ABORT, 'disk I/O error'); END;",
        )
        .unwrap();
    let (n, seen) = notifier(&e, vec![created(42), created(99)]);
    let (_, stats) = n.after_sync(&mut w.conn, at(T1)).unwrap();
    assert_eq!(stats.sent, 1);
    n.flush(&w.conn, at(T1) + Duration::hours(1)).unwrap();
    assert_eq!(seen.lock().unwrap().len(), 1, "不重送 POST");
}

#[test]
fn retry_after_lost_response_reuses_issue_already_created() {
    let e = env();
    let mut w = kev_world();
    let existing = json!({"issues": [
        {"id": 42, "subject": format!("[cvewatch][P1][KEV] {KEV_CVE} PAN-OS — FW-01")}
    ]})
    .to_string();
    let (n, seen) = notifier(&e, vec![(504, String::new()), (200, existing)]);
    n.after_sync(&mut w.conn, at(T1)).unwrap();
    n.flush(&w.conn, at(T1) + Duration::minutes(5)).unwrap();
    assert_eq!(issue_of(&w, KEV_CVE), Some(42));
    assert_eq!(seen.lock().unwrap().len(), 2, "不再 POST");
}

#[test]
fn queued_create_for_deleted_issue_recreates_it() {
    let e = env();
    let mut w = kev_world();
    let (n, seen) = notifier(&e, vec![(404, String::new()), created(43)]);
    n.queue_urgent(&mut w.conn, at(T1)).unwrap();
    // 佇列中的建立請求送出前，其他途徑已記錄議題 #42（之後在 Redmine 被刪除）
    w.conn
        .execute(
            "INSERT INTO redmine_issue (cve_id, issue_id, created_at) VALUES (?1, 42, ?2)",
            [KEV_CVE, T1],
        )
        .unwrap();
    let stats = n.flush(&w.conn, at(T1)).unwrap();
    assert_eq!(stats.sent, 1);
    assert_eq!(issue_of(&w, KEV_CVE), Some(43));
    let lines: Vec<String> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.line.clone())
        .collect();
    assert_eq!(
        lines,
        ["PUT /issues/42.json HTTP/1.1", "POST /issues.json HTTP/1.1"]
    );
}

#[test]
fn setting_the_same_status_adds_no_note() {
    let mut w = kev_world();
    let cfg = notify_cfg(443, None);
    w.conn
        .execute(
            "INSERT INTO redmine_issue (cve_id, issue_id, created_at) VALUES (?1, 42, ?2)",
            [KEV_CVE, T1],
        )
        .unwrap();
    let id: i64 = w
        .conn
        .query_row("SELECT id FROM finding WHERE cve_id = ?1", [KEV_CVE], |r| {
            r.get(0)
        })
        .unwrap();
    let open = triage::validate(&ChangeRequest {
        status: "open".into(),
        note: "再確認".into(),
        ..ChangeRequest::default()
    })
    .unwrap();
    // CLI：即使狀態相同也記錄事件（skip_unchanged = false），但不產生 Redmine 註記
    let outcome =
        triage_notes::set_status_many(&mut w.conn, &cfg, &[id], &open, at(T1), false).unwrap();
    assert_eq!(outcome.changed, [id]);
    assert!(outbox::list(&w.conn).unwrap().is_empty());
}

#[test]
fn interrupted_send_checks_for_existing_issue_before_posting() {
    let e = env();
    let mut w = kev_world();
    let existing = json!({"issues": [
        {"id": 77, "subject": format!("[cvewatch][P1][KEV] {KEV_CVE} PAN-OS — FW-01")}
    ]})
    .to_string();
    let (n, seen) = notifier(&e, vec![(200, existing)]);
    n.queue_urgent(&mut w.conn, at(T1)).unwrap();
    // 先前的程序送出途中中止：認領已過期、失敗次數仍為 0
    w.conn
        .execute(
            "UPDATE outbox SET claimed_until = ?1",
            [(at(T1) - Duration::minutes(30)).to_rfc3339()],
        )
        .unwrap();
    n.flush(&w.conn, at(T1)).unwrap();
    assert_eq!(issue_of(&w, KEV_CVE), Some(77));
    let lines: Vec<String> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.line.clone())
        .collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("GET /issues.json?"), "{}", lines[0]);
}

#[test]
fn missing_api_key_keeps_items_queued_without_using_attempts() {
    let mut w = kev_world();
    let n = Notifier {
        config: notify_cfg(443, None),
        stale_days: 14,
        lifecycle: None,
        zone: Zone::Local,
        mail: Endpoint::Disabled,
        redmine: Endpoint::Broken("Credential Manager 中沒有 redmine_api_key".into()),
    };
    for hour in 0..20 {
        let (_, stats) = n
            .after_sync(&mut w.conn, at(T1) + Duration::hours(hour))
            .unwrap();
        assert_eq!(stats.blocked, 1);
        assert!(stats.errors[0].contains("redmine_api_key"));
    }
    let items = outbox::list(&w.conn).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].attempts, 0, "設定問題不計次數");
    assert!(!items[0].is_dead());
}

#[test]
fn pending_dismiss_note_is_queued_in_same_transaction() {
    let mut w = world();
    let c = w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.4")])[0];
    w.cve(
        KEV_CVE,
        json!([common::matching::affected(
            "Palo Alto Networks",
            "PAN-OS",
            None,
            json!([])
        )]),
        json!([]),
    );
    w.run();
    w.conn
        .execute(
            "INSERT INTO redmine_issue (cve_id, issue_id, created_at) VALUES (?1, 42, ?2)",
            [KEV_CVE, T1],
        )
        .unwrap();
    let cfg = notify_cfg(443, None);
    let scoring = w.settings.scoring.clone();
    let request = cvewatch_core::matching::pending_ops::ResolveRequest {
        catalog: &w.catalog,
        scoring: &scoring,
        raw_cve: KEV_CVE,
        action: &cvewatch_core::matching::pending_ops::Action::Dismiss,
        note: "未使用此功能",
        now: at(T1),
    };
    let resolution = triage_notes::resolve_pending(&mut w.conn, &cfg, &request).unwrap();
    assert_eq!(resolution.ignored, [w.finding(c, KEV_CVE).unwrap().id]);
    let items = outbox::list(&w.conn).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0]
        .payload_json
        .contains("FW-01：ignored（false_positive；等待區人工判定不相關；未使用此功能）"));
}
