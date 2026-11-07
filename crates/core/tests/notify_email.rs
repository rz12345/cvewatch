//! 06 任務 3.5：緊急信與每日摘要內容（golden file）；本機假 SMTP 驗證送出、失敗重送、
//! STARTTLS 不降級、明文拒送密碼、驗證失敗訊息與自簽 CA。
//!
//! 更新 golden：`CVEWATCH_UPDATE_GOLDEN=1 cargo test -p cvewatch-core --test notify_email`

mod common;
#[path = "common/notify.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::FixedOffset;
use common::matching::{at, panos, world, World};
use cvewatch_core::config::Config;
use cvewatch_core::notify::content::{self, Zone};
use cvewatch_core::notify::digest::{self, DIGEST_LAST_SENT_KEY};
use cvewatch_core::notify::dispatch::{DigestOutcome, Endpoint, Notifier};
use cvewatch_core::notify::email::{Mail, MailTransport, SmtpMailer, SmtpSettings};
use cvewatch_core::notify::outbox::{self, Channel};
use cvewatch_core::notify::settings::{EmailConfig, NotifyConfig, SmtpSecurity};
use cvewatch_core::notify::urgent;
use rusqlite::params;
use serde_json::json;
use support::{pki, smtp_server, SmtpOptions};
use zeroize::Zeroizing;

const KEV_CVE: &str = "CVE-2026-2001";
const HIGH_CVE: &str = "CVE-2026-2002";
const PROBABLE_CVE: &str = "CVE-2026-2003";

fn zone() -> Zone {
    Zone::Fixed(FixedOffset::east_opt(8 * 3600).unwrap())
}

fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/notify")
        .join(name);
    if std::env::var_os("CVEWATCH_UPDATE_GOLDEN").is_some() || !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(actual, expected, "golden {name} 不符");
}

/// FW-01（11.1.2）、FW-02（11.1.3）；KEV CVE 影響兩台，CVSS 7.5 的 CVE 只影響 FW-01，
/// 另一個 KEV CVE 版本資料不足（probable）。
fn scenario() -> World {
    let mut w = world();
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.2")]);
    w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.3")]);
    w.cve(
        KEV_CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.cvss(KEV_CVE, 9.8);
    w.kev(KEV_CVE, "2026-10-20");
    w.epss(KEV_CVE, 0.945);
    w.cve(
        HIGH_CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.3", "status": "affected"}])),
        json!([]),
    );
    w.cvss(HIGH_CVE, 7.5);
    w.cve(
        PROBABLE_CVE,
        json!([common::matching::affected(
            "Palo Alto Networks",
            "PAN-OS",
            None,
            json!([{"version": "10.2.0", "lessThan": "10.2.9", "status": "affected"}])
        )]),
        json!([]),
    );
    w.kev(PROBABLE_CVE, "2026-10-01");
    w.run_at("2026-11-06T01:00:00+00:00");
    w
}

fn email_cfg(port: u16, security: SmtpSecurity, username: &str) -> EmailConfig {
    EmailConfig {
        enabled: true,
        smtp_host: "localhost".into(),
        smtp_port: port,
        security,
        username: username.into(),
        from: "cvewatch <cvewatch@example.edu.tw>".into(),
        to: vec!["rz@example.edu.tw".into()],
        send_empty: true,
    }
}

fn mailer(
    email: &EmailConfig,
    password: Option<&str>,
    ca: Option<&Path>,
    insecure: bool,
) -> Result<SmtpMailer, String> {
    SmtpMailer::new(&SmtpSettings {
        email,
        password: password.map(|p| Zeroizing::new(p.to_string())),
        ca_file: ca,
        allow_insecure: insecure,
        timeout: Duration::from_secs(10),
    })
}

fn test_mail() -> Mail {
    Mail {
        subject: "[cvewatch] 測試".into(),
        body: "內容".into(),
    }
}

fn write_ca(dir: &Path, pem: &str) -> PathBuf {
    let p = dir.join("ca.pem");
    std::fs::write(&p, pem).unwrap();
    p
}

fn notifier(mail: Endpoint<dyn MailTransport>) -> Notifier {
    Notifier {
        config: NotifyConfig {
            email: EmailConfig {
                enabled: true,
                ..EmailConfig::default()
            },
            ..NotifyConfig::default()
        },
        stale_days: 14,
        zone: zone(),
        mail,
        redmine: Endpoint::Disabled,
    }
}

// ── 內容（golden）────────────────────────────────────

#[test]
fn urgent_mail_lists_all_events_in_one_message() {
    let w = scenario();
    let events = urgent::detect(&w.conn, Channel::Email, 9.0).unwrap();
    assert_eq!(events.len(), 4, "KEV CVE × 2 台 + probable × 2 台");
    let names = content::product_names(&w.conn).unwrap();
    let mail = content::urgent_mail(
        &events,
        &names,
        9.0,
        at("2026-11-06T01:00:00+00:00"),
        zone(),
    );
    golden(
        "urgent_four.txt",
        &format!("{}\n\n{}", mail.subject, mail.body),
    );
    assert!(mail.body.contains("版本待確認"));
}

#[test]
fn single_event_subject_names_cve_product_and_asset() {
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")]);
    w.cve(
        KEV_CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.kev(KEV_CVE, "2026-10-20");
    w.run();
    let events = urgent::detect(&w.conn, Channel::Email, 9.0).unwrap();
    let names = content::product_names(&w.conn).unwrap();
    let mail = content::urgent_mail(
        &events,
        &names,
        9.0,
        at("2026-11-01T00:00:00+00:00"),
        zone(),
    );
    assert_eq!(
        mail.subject,
        format!("[cvewatch][緊急][P1][KEV] {KEV_CVE} PAN-OS — FW-01")
    );
}

#[test]
fn digest_content_has_five_sections() {
    let mut w = scenario();
    // FW-01 升版：HIGH_CVE 自動轉為已處理
    let fw01: i64 = w
        .conn
        .query_row(
            "SELECT c.id FROM component c JOIN asset a ON a.id = c.asset_id WHERE a.asset_tag = 'FW-01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    w.set_version(fw01, "11.1.4", "2026-11-06T04:00:00+00:00");
    w.run_at("2026-11-06T05:00:00+00:00");
    for (cve, reason, entered, resolved, why) in [
        (
            "CVE-2026-3001",
            "keyword_only",
            "2026-11-06T02:00:00+00:00",
            None,
            None,
        ),
        (
            "CVE-2026-3002",
            "no_version_data",
            "2026-10-30T00:00:00+00:00",
            Some("2026-11-06T03:00:00+00:00"),
            Some("data_affected"),
        ),
        (
            "CVE-2026-3003",
            "vendor_investigating",
            "2026-10-01T00:00:00+00:00",
            None,
            None,
        ),
    ] {
        w.conn
            .execute(
                "INSERT INTO pending (cve_id, reason, entered_at, last_checked, resolved_at, resolved_reason)
                 VALUES (?1, ?2, ?3, ?3, ?4, ?5)",
                params![cve, reason, entered, resolved, why],
            )
            .unwrap();
    }
    for (source, at_, status, error) in [
        ("cvelist", "2026-11-06T20:00:00+00:00", "success", None),
        (
            "osv",
            "2026-11-06T20:05:00+00:00",
            "failed",
            Some("HTTP 503"),
        ),
    ] {
        w.conn
            .execute(
                "INSERT INTO sync_run (started_at, finished_at, source, status, error) VALUES (?1, ?1, ?2, ?3, ?4)",
                params![at_, source, status, error],
            )
            .unwrap();
    }
    let dead = outbox::enqueue(
        &w.conn,
        Channel::Redmine,
        "create_issue",
        "{}",
        at("2026-11-06T06:00:00+00:00"),
    )
    .unwrap();
    outbox::fail(
        &w.conn,
        dead,
        "Redmine HTTP 422：Tracker is not included in the list",
        true,
        at("2026-11-06T06:00:00+00:00"),
    )
    .unwrap();

    let data = digest::collect(
        &w.conn,
        at("2026-11-06T00:00:00+00:00"),
        at("2026-11-07T00:00:00+00:00"),
        14,
    )
    .unwrap();
    assert!(!data.is_empty());
    let names = content::product_names(&w.conn).unwrap();
    let mail = content::digest_mail(&data, &names, zone());
    golden(
        "digest_full.txt",
        &format!("{}\n\n{}", mail.subject, mail.body),
    );
}

#[test]
fn empty_digest_says_nothing_new_with_sync_status() {
    let w = world();
    w.conn
        .execute(
            "INSERT INTO sync_run (started_at, finished_at, source, status) VALUES (?1, ?1, 'kev', 'success')",
            params!["2026-11-06T20:00:00+00:00"],
        )
        .unwrap();
    let data = digest::collect(
        &w.conn,
        at("2026-11-06T00:00:00+00:00"),
        at("2026-11-07T00:00:00+00:00"),
        14,
    )
    .unwrap();
    assert!(data.is_empty());
    let mail = content::digest_mail(&data, &content::product_names(&w.conn).unwrap(), zone());
    golden(
        "digest_empty.txt",
        &format!("{}\n\n{}", mail.subject, mail.body),
    );
}

// ── 佇列與去重 ─────────────────────────────────────

/// 記錄寄出內容的替身；`fail` 為真時模擬 SMTP 無法連線。
#[derive(Default)]
struct Recorder {
    sent: std::sync::Mutex<Vec<Mail>>,
    fail: std::sync::atomic::AtomicBool,
}

struct Shared(std::sync::Arc<Recorder>);

impl MailTransport for Shared {
    fn send(&self, mail: &Mail) -> Result<(), String> {
        if self.0.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("SMTP 寄送失敗：connection refused".into());
        }
        self.0.sent.lock().unwrap().push(mail.clone());
        Ok(())
    }
}

#[test]
fn two_urgent_events_one_mail_then_nothing_on_next_sync() {
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.2")]);
    w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.3")]);
    w.cve(
        KEV_CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.cvss(KEV_CVE, 9.8);
    w.run();
    let rec = std::sync::Arc::new(Recorder::default());
    let n = notifier(Endpoint::Ready(Box::new(Shared(rec.clone()))));
    let (queued, stats) = n
        .after_sync(&mut w.conn, at("2026-11-01T00:00:00+00:00"))
        .unwrap();
    assert_eq!(queued.email_events, 2);
    assert_eq!(stats.sent, 1);
    assert_eq!(rec.sent.lock().unwrap().len(), 1, "只寄出一封");
    let (queued, _) = n
        .after_sync(&mut w.conn, at("2026-11-01T04:00:00+00:00"))
        .unwrap();
    assert_eq!(queued.email_events, 0);
    assert_eq!(rec.sent.lock().unwrap().len(), 1);
}

#[test]
fn smtp_failure_keeps_mail_queued_until_sent() {
    let mut w = scenario();
    let rec = std::sync::Arc::new(Recorder::default());
    rec.fail.store(true, std::sync::atomic::Ordering::SeqCst);
    let n = notifier(Endpoint::Ready(Box::new(Shared(rec.clone()))));
    let t0 = at("2026-11-06T01:00:00+00:00");
    let (_, stats) = n.after_sync(&mut w.conn, t0).unwrap();
    assert_eq!((stats.sent, stats.retrying), (0, 1));
    let unsent: i64 = w
        .conn
        .query_row(
            "SELECT count(*) FROM notification_log WHERE sent_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unsent, 4, "尚未記錄為已通知");
    // 下次 sync（4 小時後）：SMTP 恢復 → 重送成功，且不重複排入
    rec.fail.store(false, std::sync::atomic::Ordering::SeqCst);
    let (queued, stats) = n
        .after_sync(&mut w.conn, at("2026-11-06T05:00:00+00:00"))
        .unwrap();
    assert_eq!(queued.email_events, 0);
    assert_eq!(stats.sent, 1);
    let sent: i64 = w
        .conn
        .query_row(
            "SELECT count(*) FROM notification_log WHERE sent_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(sent, 4);
    assert!(outbox::list(&w.conn).unwrap().is_empty());
}

#[test]
fn digest_after_two_days_offline_is_one_mail_covering_both_days() {
    let mut w = scenario();
    digest::mark_sent(&w.conn, "2026-11-04T00:30:00+00:00").unwrap();
    let rec = std::sync::Arc::new(Recorder::default());
    rec.fail.store(true, std::sync::atomic::Ordering::SeqCst);
    let n = notifier(Endpoint::Ready(Box::new(Shared(rec.clone()))));
    // 離線：第一天的摘要寄不出去
    let day1 = at("2026-11-05T00:30:00+00:00");
    n.queue_digest(&mut w.conn, day1).unwrap();
    n.flush(&w.conn, day1).unwrap();
    // 兩天後開機：新摘要取代未寄出的舊摘要，只寄一封
    rec.fail.store(false, std::sync::atomic::Ordering::SeqCst);
    let day3 = at("2026-11-07T01:10:00+00:00");
    let outcome = n.queue_digest(&mut w.conn, day3).unwrap();
    assert_eq!(
        outcome,
        DigestOutcome::Queued {
            empty: false,
            replaced: 1
        }
    );
    n.flush(&w.conn, day3).unwrap();
    let sent = rec.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert!(
        sent[0].body.contains("2026-11-04 08:30 – 2026-11-07 09:10"),
        "{}",
        sent[0].body
    );
    assert!(sent[0].body.contains(KEV_CVE), "兩天內的新命中");
    let last: String = w
        .conn
        .query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            [DIGEST_LAST_SENT_KEY],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(last, day3.to_rfc3339());
}

#[test]
fn empty_digest_respects_send_empty() {
    let mut w = world();
    let rec = std::sync::Arc::new(Recorder::default());
    let mut n = notifier(Endpoint::Ready(Box::new(Shared(rec.clone()))));
    let now = at("2026-11-07T00:30:00+00:00");
    assert_eq!(
        n.queue_digest(&mut w.conn, now).unwrap(),
        DigestOutcome::Queued {
            empty: true,
            replaced: 0
        }
    );
    n.flush(&w.conn, now).unwrap();
    assert!(rec.sent.lock().unwrap()[0].subject.ends_with("無新事項"));
    n.config.email.send_empty = false;
    assert_eq!(
        n.queue_digest(&mut w.conn, now).unwrap(),
        DigestOutcome::SkippedEmpty
    );
    n.mail = Endpoint::Disabled;
    assert_eq!(
        n.queue_digest(&mut w.conn, now).unwrap(),
        DigestOutcome::EmailDisabled
    );
}

// ── SMTP（本機假伺服器）─────────────────────────────

#[test]
fn sends_through_plain_relay_without_auth() {
    let (port, inbox) = smtp_server(SmtpOptions::default());
    let cfg = email_cfg(port, SmtpSecurity::None, "");
    mailer(&cfg, None, None, true)
        .unwrap()
        .send(&test_mail())
        .unwrap();
    let inbox = inbox.lock().unwrap();
    assert_eq!(inbox.len(), 1);
    assert!(inbox[0].contains("rz@example.edu.tw"));
}

#[test]
fn unreachable_server_is_reported() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let cfg = email_cfg(port, SmtpSecurity::None, "");
    let err = mailer(&cfg, None, None, true)
        .unwrap()
        .send(&test_mail())
        .unwrap_err();
    assert!(err.starts_with("SMTP 寄送失敗"), "{err}");
}

#[test]
fn starttls_is_required_and_not_downgraded() {
    let (port, inbox) = smtp_server(SmtpOptions::default());
    let cfg = email_cfg(port, SmtpSecurity::Starttls, "");
    let err = mailer(&cfg, None, None, false)
        .unwrap()
        .send(&test_mail())
        .unwrap_err();
    assert_eq!(err, "伺服器不支援 STARTTLS，未寄出（不會降級為明文）");
    assert!(inbox.lock().unwrap().is_empty());
}

#[test]
fn plaintext_with_stored_password_is_refused() {
    let cfg = email_cfg(25, SmtpSecurity::None, "");
    let err = mailer(&cfg, Some("s3cret"), None, true).err().unwrap();
    assert!(err.contains("不得傳送 SMTP 密碼"), "{err}");
}

#[test]
fn internal_ca_is_trusted_via_ca_file() {
    let pki = pki();
    let dir = tempfile::tempdir().unwrap();
    let ca = write_ca(dir.path(), &pki.ca_pem);
    let (port, inbox) = smtp_server(SmtpOptions {
        tls: Some(pki.server.clone()),
        auth: Some(("rz".into(), "right".into())),
    });
    let cfg = email_cfg(port, SmtpSecurity::Tls, "rz");
    mailer(&cfg, Some("right"), Some(&ca), false)
        .unwrap()
        .send(&test_mail())
        .unwrap();
    assert_eq!(inbox.lock().unwrap().len(), 1);
    let err = mailer(&cfg, Some("right"), None, false)
        .unwrap()
        .send(&test_mail())
        .unwrap_err();
    assert!(
        err.contains("TLS") && err.contains("ca_file"),
        "未設定 ca_file 時憑證驗證失敗：{err}"
    );
}

#[test]
fn wrong_password_shows_server_error_without_password() {
    let pki = pki();
    let dir = tempfile::tempdir().unwrap();
    let ca = write_ca(dir.path(), &pki.ca_pem);
    let (port, _) = smtp_server(SmtpOptions {
        tls: Some(pki.server.clone()),
        auth: Some(("rz".into(), "right".into())),
    });
    let cfg = email_cfg(port, SmtpSecurity::Tls, "rz");
    let mut notify = Config::default().notify;
    notify.email = cfg.clone();
    notify.ca_file = Some(ca.clone());
    let err = cvewatch_core::notify::check::send_test_email(
        &notify,
        Some(Zeroizing::new("wrong-pass".into())),
        &Config::default().http,
    )
    .unwrap_err();
    assert!(err.contains("驗證失敗") && err.contains("535"), "{err}");
    assert!(!err.contains("wrong-pass"));
}
