//! 送出佇列（outbox）：Email 與 Redmine 請求失敗時保留，依退避排程重送。
//!
//! 退避：第 1 次失敗後 5 分鐘、第 2 次 30 分鐘、第 3 次 2 小時，之後每 6 小時；
//! 失敗達 [`MAX_ATTEMPTS`] 次或遇到不可重試的錯誤即標為 dead（不再重送，於摘要與 Dashboard 顯示）。
//! 送出成功即刪除該列，並把對應的 `notification_log` 標為已送出。

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::CoreError;

/// 失敗達此次數即標為 dead。
pub const MAX_ATTEMPTS: u32 = 10;
/// 退避間隔（分鐘）：第 1、2、3 次失敗後；之後一律 [`BACKOFF_STEADY_MINUTES`]。
pub const BACKOFF_MINUTES: [i64; 3] = [5, 30, 120];
pub const BACKOFF_STEADY_MINUTES: i64 = 6 * 60;
/// 送出前先「認領」項目（`claimed_until`），避免排程 sync 與摘要同時執行時重複送出或刪除
/// 送出中的項目。程序在送出途中中止時，認領於此時間後失效。
pub const CLAIM_LEASE_MINUTES: i64 = 15;

string_enum!(
    Channel {
        Email => "email",
        Redmine => "redmine",
    }
);

impl Channel {
    pub fn label(self) -> &'static str {
        match self {
            Channel::Email => "Email",
            Channel::Redmine => "Redmine",
        }
    }
}

/// 第 `failures` 次失敗（從 1 起算）之後的等待時間。
pub fn backoff(failures: u32) -> Duration {
    let idx = usize::try_from(failures.saturating_sub(1)).unwrap_or(usize::MAX);
    Duration::minutes(
        BACKOFF_MINUTES
            .get(idx)
            .copied()
            .unwrap_or(BACKOFF_STEADY_MINUTES),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxItem {
    pub id: i64,
    pub channel: Channel,
    pub kind: String,
    pub payload_json: String,
    pub created_at: String,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub next_attempt_at: String,
    pub dead_at: Option<String>,
    /// 正在送出（另一個程序認領）的期限
    pub claimed_until: Option<String>,
}

fn passed(t: &str, now: DateTime<Utc>) -> bool {
    // 時間無法解析時視為已到，避免永久卡住
    DateTime::parse_from_rfc3339(t).map_or(true, |t| t.with_timezone(&Utc) <= now)
}

impl OutboxItem {
    pub fn is_dead(&self) -> bool {
        self.dead_at.is_some()
    }

    /// 是否有其他程序正在送出（認領未過期）。
    pub fn is_claimed(&self, now: DateTime<Utc>) -> bool {
        self.claimed_until
            .as_deref()
            .is_some_and(|t| !passed(t, now))
    }

    /// 是否已到重送時間且沒有被認領。
    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        !self.is_dead() && !self.is_claimed(now) && passed(&self.next_attempt_at, now)
    }
}

const SELECT: &str = "SELECT id, channel, kind, payload_json, created_at, attempts, last_error,
                             next_attempt_at, dead_at, claimed_until FROM outbox";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<OutboxItem> {
    let channel: String = r.get(1)?;
    Ok(OutboxItem {
        id: r.get(0)?,
        channel: Channel::parse(&channel).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                format!("不合法的管道：{channel}").into(),
            )
        })?,
        kind: r.get(2)?,
        payload_json: r.get(3)?,
        created_at: r.get(4)?,
        attempts: r.get(5)?,
        last_error: r.get(6)?,
        next_attempt_at: r.get(7)?,
        dead_at: r.get(8)?,
        claimed_until: r.get(9)?,
    })
}

/// 排入佇列，立即可送。
pub fn enqueue(
    conn: &Connection,
    channel: Channel,
    kind: &str,
    payload_json: &str,
    now: DateTime<Utc>,
) -> Result<i64, CoreError> {
    let at = now.to_rfc3339();
    conn.execute(
        "INSERT INTO outbox (channel, kind, payload_json, created_at, next_attempt_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![channel.as_str(), kind, payload_json, at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<OutboxItem>, CoreError> {
    Ok(conn
        .query_row(&format!("{SELECT} WHERE id = ?1"), params![id], row)
        .optional()?)
}

/// 全部項目（含 dead），依建立順序。
pub fn list(conn: &Connection) -> Result<Vec<OutboxItem>, CoreError> {
    let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY id"))?;
    let items = stmt.query_map([], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(items)
}

/// 已到重送時間的項目，依建立順序。
pub fn due(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<OutboxItem>, CoreError> {
    Ok(list(conn)?.into_iter().filter(|i| i.is_due(now)).collect())
}

/// 無法送出（dead）的項目。
pub fn dead(conn: &Connection) -> Result<Vec<OutboxItem>, CoreError> {
    Ok(list(conn)?
        .into_iter()
        .filter(OutboxItem::is_dead)
        .collect())
}

/// 送出成功：刪除並標記對應的通知為已送出。
pub fn succeed(conn: &Connection, id: i64, now: DateTime<Utc>) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE notification_log SET sent_at = ?2, outbox_id = NULL WHERE outbox_id = ?1",
        params![id, now.to_rfc3339()],
    )?;
    conn.execute("DELETE FROM outbox WHERE id = ?1", params![id])?;
    Ok(())
}

/// 失敗的處理結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailOutcome {
    /// 排定下次重送時間
    Retry { next_attempt_at: DateTime<Utc> },
    /// 不再重送
    Dead,
}

/// 送出失敗：次數 +1；`permanent`（例如 HTTP 422）或達上限即標為 dead，否則依退避排程。
pub fn fail(
    conn: &Connection,
    id: i64,
    error: &str,
    permanent: bool,
    now: DateTime<Utc>,
) -> Result<FailOutcome, CoreError> {
    let attempts: u32 = conn
        .query_row(
            "SELECT attempts FROM outbox WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid(format!("找不到送出佇列項目 {id}")))?;
    let attempts = attempts.saturating_add(1);
    if permanent || attempts >= MAX_ATTEMPTS {
        conn.execute(
            "UPDATE outbox SET attempts = ?2, last_error = ?3, dead_at = ?4, claimed_until = NULL
             WHERE id = ?1",
            params![id, attempts, error, now.to_rfc3339()],
        )?;
        return Ok(FailOutcome::Dead);
    }
    let next = now + backoff(attempts);
    conn.execute(
        "UPDATE outbox SET attempts = ?2, last_error = ?3, next_attempt_at = ?4,
                           claimed_until = NULL
         WHERE id = ?1",
        params![id, attempts, error, next.to_rfc3339()],
    )?;
    Ok(FailOutcome::Retry {
        next_attempt_at: next,
    })
}

/// 延後到下次執行（不計失敗次數），例如註記所屬的議題仍在佇列中等待建立。
pub fn defer(conn: &Connection, id: i64, reason: &str) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE outbox SET last_error = ?2 WHERE id = ?1",
        params![id, reason],
    )?;
    Ok(())
}

/// 刪除尚未送出（非 dead、沒有其他程序正在送出）的同類項目，回傳刪除數
/// （例如新的摘要取代未送出的舊摘要）。
pub fn discard_pending(
    conn: &Connection,
    channel: Channel,
    kind: &str,
    now: DateTime<Utc>,
) -> Result<usize, CoreError> {
    let mut removed = 0;
    for item in list(conn)? {
        if item.channel == channel && item.kind == kind && !item.is_dead() && !item.is_claimed(now)
        {
            removed += conn.execute("DELETE FROM outbox WHERE id = ?1", params![item.id])?;
        }
    }
    Ok(removed)
}

/// 是否有其他程序正在送出此類項目。
pub fn in_flight(
    conn: &Connection,
    channel: Channel,
    kind: &str,
    now: DateTime<Utc>,
) -> Result<bool, CoreError> {
    Ok(list(conn)?
        .iter()
        .any(|i| i.channel == channel && i.kind == kind && i.is_claimed(now)))
}

/// 單一項目的送出結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    /// 暫時性錯誤：依退避排程重送
    Retry(String),
    /// 不可重試的錯誤：直接標為 dead
    Permanent(String),
    /// 這次不送、不計次數（附原因）
    Defer(String),
    /// 管道停用：保留在佇列，不計次數
    Skipped,
    /// 設定未就緒（例如缺少密碼、CA 檔錯誤）：保留在佇列、不計次數，回報原因；
    /// 修正設定後即可送出，不會因此標為無法送出
    Blocked(String),
}

/// 實際送出一筆佇列項目；成功時的附帶寫入（例如記錄議題編號）由實作在 `conn` 上完成。
pub trait Deliver {
    fn deliver(&self, conn: &Connection, item: &OutboxItem, now: DateTime<Utc>) -> Delivery;
}

/// 一次清佇列的統計。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlushStats {
    pub sent: usize,
    pub retrying: usize,
    pub dead: usize,
    pub deferred: usize,
    /// 同一管道先前的項目暫時失敗，本次不再嘗試（避免每筆都等到逾時）
    pub postponed: usize,
    /// 設定未就緒而保留的項目
    pub blocked: usize,
    /// 失敗訊息（`#id 管道：錯誤`）
    pub errors: Vec<String>,
}

/// 送出所有已到時間的項目（依建立順序）。每筆各自寫入，單筆失敗不影響其他項目；
/// 某管道遇到暫時性失敗（例如伺服器無法連線）後，本次不再嘗試該管道的其餘項目。
pub fn flush(
    conn: &Connection,
    deliver: &dyn Deliver,
    now: DateTime<Utc>,
) -> Result<FlushStats, CoreError> {
    let mut stats = FlushStats::default();
    let mut unreachable: Vec<Channel> = Vec::new();
    for item in due(conn, now)? {
        if unreachable.contains(&item.channel) {
            stats.postponed += 1;
            continue;
        }
        if !claim(conn, &item, now)? {
            continue;
        }
        match deliver.deliver(conn, &item, now) {
            Delivery::Sent => {
                succeed(conn, item.id, now)?;
                stats.sent += 1;
            }
            Delivery::Retry(e) => {
                record_failure(conn, &item, &e, false, now, &mut stats)?;
                unreachable.push(item.channel);
            }
            Delivery::Permanent(e) => record_failure(conn, &item, &e, true, now, &mut stats)?,
            Delivery::Defer(reason) => {
                defer(conn, item.id, &reason)?;
                release(conn, item.id)?;
                stats.deferred += 1;
            }
            Delivery::Skipped => release(conn, item.id)?,
            Delivery::Blocked(reason) => {
                defer(conn, item.id, &reason)?;
                release(conn, item.id)?;
                stats.blocked += 1;
                stats.errors.push(format!(
                    "#{} {}：{reason}（設定修正後送出）",
                    item.id,
                    item.channel.label()
                ));
                unreachable.push(item.channel);
            }
        }
    }
    Ok(stats)
}

/// 認領項目（以讀取時的認領值做條件式更新）；另一個程序已認領或處理時回傳 false。
fn claim(conn: &Connection, item: &OutboxItem, now: DateTime<Utc>) -> Result<bool, CoreError> {
    let lease = (now + Duration::minutes(CLAIM_LEASE_MINUTES)).to_rfc3339();
    let n = conn.execute(
        "UPDATE outbox SET claimed_until = ?3
         WHERE id = ?1 AND claimed_until IS ?2 AND dead_at IS NULL",
        params![item.id, item.claimed_until, lease],
    )?;
    Ok(n == 1)
}

/// 解除認領（延後或管道停用時，項目維持原本的下次時間）。
fn release(conn: &Connection, id: i64) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE outbox SET claimed_until = NULL WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

fn record_failure(
    conn: &Connection,
    item: &OutboxItem,
    error: &str,
    permanent: bool,
    now: DateTime<Utc>,
    stats: &mut FlushStats,
) -> Result<(), CoreError> {
    let outcome = fail(conn, item.id, error, permanent, now)?;
    stats
        .errors
        .push(format!("#{} {}：{error}", item.id, item.channel.label()));
    match outcome {
        FailOutcome::Dead => {
            stats.dead += 1;
            tracing::error!(id = item.id, channel = %item.channel, kind = %item.kind, error, "通知無法送出，已標為 dead");
        }
        FailOutcome::Retry { next_attempt_at } => {
            stats.retrying += 1;
            tracing::warn!(id = item.id, channel = %item.channel, kind = %item.kind, error,
                next = %next_attempt_at.to_rfc3339(), "通知送出失敗，稍後重送");
        }
    }
    Ok(())
}
