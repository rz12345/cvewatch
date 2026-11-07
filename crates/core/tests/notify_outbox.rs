//! 06 任務 1.3：送出佇列的退避時間、10 次失敗後 dead、成功時刪除並標記通知已送出。

mod common;

use std::cell::RefCell;

use chrono::{DateTime, Duration, Utc};
use common::matching::at;
use cvewatch_core::notify::outbox::{
    self, backoff, Channel, Deliver, Delivery, FailOutcome, OutboxItem, MAX_ATTEMPTS,
};
use rusqlite::{params, Connection};

const NOW: &str = "2026-11-07T00:00:00+00:00";

fn db() -> Connection {
    common::memory_db()
}

/// 依序回傳預設結果的替身；記錄送出的項目 id。
struct Script {
    results: RefCell<Vec<Delivery>>,
    seen: RefCell<Vec<i64>>,
}

impl Script {
    fn new(results: Vec<Delivery>) -> Self {
        Script {
            results: RefCell::new(results),
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl Deliver for Script {
    fn deliver(&self, _conn: &Connection, item: &OutboxItem, _now: DateTime<Utc>) -> Delivery {
        self.seen.borrow_mut().push(item.id);
        let mut r = self.results.borrow_mut();
        if r.is_empty() {
            Delivery::Sent
        } else {
            r.remove(0)
        }
    }
}

#[test]
fn backoff_schedule_is_5m_30m_2h_then_6h() {
    assert_eq!(backoff(1), Duration::minutes(5));
    assert_eq!(backoff(2), Duration::minutes(30));
    assert_eq!(backoff(3), Duration::hours(2));
    assert_eq!(backoff(4), Duration::hours(6));
    assert_eq!(backoff(9), Duration::hours(6));
}

#[test]
fn failure_schedules_next_attempt_and_item_is_not_due_until_then() {
    let conn = db();
    let now = at(NOW);
    let id = outbox::enqueue(&conn, Channel::Email, "urgent", "{}", now).unwrap();
    assert_eq!(outbox::due(&conn, now).unwrap().len(), 1);

    let outcome = outbox::fail(&conn, id, "連線逾時", false, now).unwrap();
    assert_eq!(
        outcome,
        FailOutcome::Retry {
            next_attempt_at: now + Duration::minutes(5)
        }
    );
    assert!(outbox::due(&conn, now + Duration::minutes(4))
        .unwrap()
        .is_empty());
    let item = &outbox::due(&conn, now + Duration::minutes(5)).unwrap()[0];
    assert_eq!(item.attempts, 1);
    assert_eq!(item.last_error.as_deref(), Some("連線逾時"));

    outbox::fail(&conn, id, "x", false, now).unwrap();
    let item = outbox::get(&conn, id).unwrap().unwrap();
    assert_eq!(
        DateTime::parse_from_rfc3339(&item.next_attempt_at).unwrap(),
        now + Duration::minutes(30)
    );
}

#[test]
fn tenth_failure_marks_item_dead() {
    let conn = db();
    let now = at(NOW);
    let id = outbox::enqueue(&conn, Channel::Redmine, "create_issue", "{}", now).unwrap();
    for n in 1..MAX_ATTEMPTS {
        let outcome = outbox::fail(&conn, id, "HTTP 502", false, now).unwrap();
        assert!(matches!(outcome, FailOutcome::Retry { .. }), "第 {n} 次");
    }
    assert_eq!(
        outbox::fail(&conn, id, "HTTP 502", false, now).unwrap(),
        FailOutcome::Dead
    );
    let item = outbox::get(&conn, id).unwrap().unwrap();
    assert!(item.is_dead());
    assert_eq!(item.attempts, MAX_ATTEMPTS);
    assert!(outbox::due(&conn, now + Duration::days(30))
        .unwrap()
        .is_empty());
    assert_eq!(outbox::dead(&conn).unwrap().len(), 1);
}

#[test]
fn permanent_error_is_dead_immediately() {
    let conn = db();
    let now = at(NOW);
    let id = outbox::enqueue(&conn, Channel::Redmine, "create_issue", "{}", now).unwrap();
    assert_eq!(
        outbox::fail(&conn, id, "HTTP 422", true, now).unwrap(),
        FailOutcome::Dead
    );
    assert_eq!(outbox::get(&conn, id).unwrap().unwrap().attempts, 1);
}

#[test]
fn flush_sends_in_order_and_records_outcomes() {
    let conn = db();
    let now = at(NOW);
    let a = outbox::enqueue(&conn, Channel::Email, "urgent", "{}", now).unwrap();
    let b = outbox::enqueue(&conn, Channel::Redmine, "create_issue", "{}", now).unwrap();
    let c = outbox::enqueue(&conn, Channel::Redmine, "add_note", "{}", now).unwrap();
    let d = outbox::enqueue(&conn, Channel::Email, "digest", "{}", now).unwrap();
    // a 的通知紀錄：成功後標為已送出
    conn.execute(
        "INSERT INTO notification_log (channel, event_kind, cve_id, queued_at, outbox_id)
         VALUES ('email', 'NEW_URGENT', 'CVE-2026-0001', ?1, ?2)",
        params![NOW, a],
    )
    .unwrap();
    let script = Script::new(vec![
        Delivery::Sent,
        Delivery::Defer("議題尚未建立".into()),
        Delivery::Retry("HTTP 502".into()),
        Delivery::Permanent("HTTP 422".into()),
    ]);
    let stats = outbox::flush(&conn, &script, now).unwrap();
    assert_eq!(*script.seen.borrow(), vec![a, b, c, d]);
    assert_eq!(
        (stats.sent, stats.retrying, stats.deferred, stats.dead),
        (1, 1, 1, 1)
    );
    assert_eq!(stats.errors.len(), 2);
    assert!(outbox::get(&conn, a).unwrap().is_none(), "成功即刪除");
    let sent_at: Option<String> = conn
        .query_row("SELECT sent_at FROM notification_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sent_at.as_deref(), Some(now.to_rfc3339().as_str()));
    let deferred = outbox::get(&conn, b).unwrap().unwrap();
    assert_eq!(deferred.attempts, 0, "延後不計次數");
    assert!(deferred.is_due(now));
    assert!(outbox::get(&conn, d).unwrap().unwrap().is_dead());

    // 下一次：只有 b 到期（c 在 5 分鐘後）
    let again = Script::new(vec![]);
    outbox::flush(&conn, &again, now).unwrap();
    assert_eq!(*again.seen.borrow(), vec![b]);
}

#[test]
fn discard_pending_keeps_dead_items() {
    let conn = db();
    let now = at(NOW);
    let old = outbox::enqueue(&conn, Channel::Email, "digest", "{}", now).unwrap();
    let dead = outbox::enqueue(&conn, Channel::Email, "digest", "{}", now).unwrap();
    outbox::fail(&conn, dead, "x", true, now).unwrap();
    assert_eq!(
        outbox::discard_pending(&conn, Channel::Email, "digest", now).unwrap(),
        1
    );
    assert!(outbox::get(&conn, old).unwrap().is_none());
    assert!(outbox::get(&conn, dead).unwrap().is_some());
}

#[test]
fn transient_failure_stops_that_channel_for_this_flush() {
    let conn = db();
    let now = at(NOW);
    let a = outbox::enqueue(&conn, Channel::Redmine, "add_note", "{}", now).unwrap();
    let b = outbox::enqueue(&conn, Channel::Redmine, "add_note", "{}", now).unwrap();
    let c = outbox::enqueue(&conn, Channel::Email, "urgent", "{}", now).unwrap();
    let script = Script::new(vec![Delivery::Retry("連線逾時".into()), Delivery::Sent]);
    let stats = outbox::flush(&conn, &script, now).unwrap();
    assert_eq!(
        *script.seen.borrow(),
        vec![a, c],
        "Redmine 第一筆失敗後不再嘗試 b"
    );
    assert_eq!((stats.retrying, stats.postponed, stats.sent), (1, 1, 1));
    let left = outbox::get(&conn, b).unwrap().unwrap();
    assert_eq!(left.attempts, 0, "未嘗試不計次數");
    assert!(left.is_due(now));
}

#[test]
fn claimed_items_are_neither_sent_nor_discarded_by_another_process() {
    let conn = db();
    let now = at(NOW);
    let id = outbox::enqueue(&conn, Channel::Email, "digest", "{}", now).unwrap();
    conn.execute(
        "UPDATE outbox SET claimed_until = ?2 WHERE id = ?1",
        params![id, (now + Duration::minutes(10)).to_rfc3339()],
    )
    .unwrap();
    assert!(outbox::due(&conn, now).unwrap().is_empty());
    assert!(outbox::in_flight(&conn, Channel::Email, "digest", now).unwrap());
    assert_eq!(
        outbox::discard_pending(&conn, Channel::Email, "digest", now).unwrap(),
        0
    );
    // 認領過期（程序中止）後重新到期
    let later = now + Duration::minutes(20);
    assert_eq!(outbox::due(&conn, later).unwrap().len(), 1);
}
