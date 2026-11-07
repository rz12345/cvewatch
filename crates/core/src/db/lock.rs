//! sync 鎖：同一時間只允許一個 sync 寫入；超過 2 小時的鎖視為過期。

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::error::CoreError;

pub const SYNC_LOCK_STALE_AFTER_HOURS: i64 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockOutcome {
    Acquired,
    ReplacedStale {
        previous_pid: u32,
        previous_since: String,
    },
}

pub fn stale_after() -> Duration {
    Duration::hours(SYNC_LOCK_STALE_AFTER_HOURS)
}

/// 取得 sync 鎖。已有未過期的鎖時回傳 [`CoreError::SyncLocked`]（結束碼 3）。
pub fn acquire_sync_lock(
    conn: &mut Connection,
    pid: u32,
    now: DateTime<Utc>,
) -> Result<LockOutcome, CoreError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existing: Option<(u32, String)> = tx
        .query_row(
            "SELECT pid, acquired_at FROM sync_lock WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let outcome = match existing {
        None => LockOutcome::Acquired,
        Some((holder, since)) => {
            if !is_stale(&since, now) {
                return Err(CoreError::SyncLocked { pid: holder, since });
            }
            LockOutcome::ReplacedStale {
                previous_pid: holder,
                previous_since: since,
            }
        }
    };
    tx.execute(
        "INSERT OR REPLACE INTO sync_lock (id, pid, acquired_at) VALUES (1, ?1, ?2)",
        params![pid, now.to_rfc3339()],
    )?;
    tx.commit()?;

    if let LockOutcome::ReplacedStale { previous_pid, .. } = &outcome {
        tracing::warn!(previous_pid, pid, "取代過期鎖");
    }
    Ok(outcome)
}

/// 釋放本程序持有的鎖；回傳是否確實刪除。
pub fn release_sync_lock(conn: &Connection, pid: u32) -> Result<bool, CoreError> {
    let removed = conn.execute(
        "DELETE FROM sync_lock WHERE id = 1 AND pid = ?1",
        params![pid],
    )?;
    Ok(removed > 0)
}

/// 時間格式無法解析的鎖視為過期，避免永久卡住。
fn is_stale(acquired_at: &str, now: DateTime<Utc>) -> bool {
    match DateTime::parse_from_rfc3339(acquired_at) {
        Ok(t) => now.signed_duration_since(t.with_timezone(&Utc)) >= stale_after(),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staleness_threshold() {
        let now = Utc::now();
        let fresh = (now - Duration::minutes(119)).to_rfc3339();
        let old = (now - Duration::minutes(121)).to_rfc3339();
        assert!(!is_stale(&fresh, now));
        assert!(is_stale(&old, now));
        assert!(is_stale("garbage", now));
    }
}
