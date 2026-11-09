//! 每日摘要的資料：範圍為「上次成功寄出的摘要」到現在（首次為過去 24 小時）。
//!
//! 段落：同步狀態、新命中（依等級）、狀態自動變化、等待區進出與逾期、無法送出的通知；
//! 有版本待補的元件時另列一行提醒（不算新事項）。

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};

use super::outbox::{self, OutboxItem};
use crate::assets::pending::PendingCounts;
use crate::db::meta;
use crate::error::CoreError;
use crate::matching::dashboard::{self, SourceStatus};
use crate::matching::pending::{PendingReason, ResolvedReason};
use crate::matching::pending_ops::{self, PendingRow};
use crate::matching::query::{self, FindingFilter, FindingRow};
use crate::matching::triage::Status;

/// 上次成功寄出的摘要所涵蓋的結束時間。
pub const DIGEST_LAST_SENT_KEY: &str = "digest_last_sent_at";
/// 首次摘要涵蓋的時間。
pub const FIRST_DIGEST_HOURS: i64 = 24;

/// 狀態自動變化（系統事件）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoChange {
    pub at: String,
    pub finding_id: i64,
    pub cve_id: String,
    pub asset_tag: String,
    pub from: Status,
    pub to: Status,
    pub note: String,
}

/// 等待區進出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMove {
    pub cve_id: String,
    pub at: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DigestData {
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
    pub sources: Vec<SourceStatus>,
    pub new_findings: Vec<FindingRow>,
    pub auto_changes: Vec<AutoChange>,
    pub pending_entered: Vec<PendingMove>,
    pub pending_exited: Vec<PendingMove>,
    pub pending_stale: Vec<PendingRow>,
    /// 期間內標為無法送出的通知
    pub dead: Vec<OutboxItem>,
    /// 目前無法送出的通知總數
    pub dead_total: usize,
    /// 寄出時版本待補的元件數與資產數（不算新事項；18 design D7）
    pub pending_versions: PendingCounts,
}

impl DigestData {
    /// 期間內有失敗的來源（最近一次同步失敗）。
    pub fn failed_sources(&self) -> impl Iterator<Item = &SourceStatus> {
        self.sources.iter().filter(|s| s.failed())
    }

    /// 沒有任何新事項（同步狀態與逾期清單不算新事項，但同步失敗算）。
    pub fn is_empty(&self) -> bool {
        self.new_findings.is_empty()
            && self.auto_changes.is_empty()
            && self.pending_entered.is_empty()
            && self.pending_exited.is_empty()
            && self.dead.is_empty()
            && self.failed_sources().next().is_none()
    }
}

/// 摘要起點：上次成功寄出摘要的結束時間；沒有時為 24 小時前。
pub fn period_start(conn: &Connection, now: DateTime<Utc>) -> Result<DateTime<Utc>, CoreError> {
    let last = meta::get(conn, DIGEST_LAST_SENT_KEY)?
        .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
        .map(|t| t.with_timezone(&Utc));
    Ok(last.unwrap_or(now - Duration::hours(FIRST_DIGEST_HOURS)))
}

/// 摘要寄出成功：記錄涵蓋的結束時間（只往後移）。
pub fn mark_sent(conn: &Connection, until: &str) -> Result<(), CoreError> {
    let newer = match meta::get(conn, DIGEST_LAST_SENT_KEY)? {
        Some(prev) => parse(&prev).is_none_or(|p| parse(until).is_some_and(|u| u > p)),
        None => true,
    };
    if newer {
        meta::set(conn, DIGEST_LAST_SENT_KEY, until)?;
    }
    Ok(())
}

fn parse(t: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(t)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// 時間字串是否落在 [since, until)。
fn within(t: &str, since: DateTime<Utc>, until: DateTime<Utc>) -> bool {
    parse(t).is_some_and(|t| t >= since && t < until)
}

pub fn collect(
    conn: &Connection,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    stale_days: u32,
) -> Result<DigestData, CoreError> {
    // SQL 先以期間起點篩選（時間皆為 RFC 3339 UTC 字串），再以 `within` 精確比對
    let filter = FindingFilter {
        first_seen_from: Some(since.to_rfc3339()),
        ..FindingFilter::default()
    };
    let new_findings = query::list(conn, &filter)?
        .into_iter()
        .filter(|f| within(&f.first_seen, since, until))
        .collect();
    let all_outbox = outbox::list(conn)?;
    let dead_total = all_outbox.iter().filter(|i| i.is_dead()).count();
    let dead = all_outbox
        .into_iter()
        .filter(|i| {
            i.dead_at
                .as_deref()
                .is_some_and(|t| within(t, since, until))
        })
        .collect();
    let (pending_entered, pending_exited) = pending_moves(conn, since, until)?;
    let dash = dashboard::load(conn, stale_days, until)?;
    Ok(DigestData {
        since,
        until,
        sources: dash.sources,
        pending_versions: dash.pending_versions,
        new_findings,
        auto_changes: auto_changes(conn, since, until)?,
        pending_entered,
        pending_exited,
        pending_stale: pending_ops::list(conn, true, stale_days, until)?,
        dead,
        dead_total,
    })
}

fn auto_changes(
    conn: &Connection,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<Vec<AutoChange>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT e.at, e.finding_id, f.cve_id, a.asset_tag, e.from_status, e.to_status, e.note
         FROM finding_event e
         JOIN finding f ON f.id = e.finding_id
         JOIN component c ON c.id = f.component_id
         JOIN asset a ON a.id = c.asset_id
         WHERE e.actor = 'system' AND e.from_status IS NOT NULL AND e.from_status <> e.to_status
           AND e.at >= ?1
         ORDER BY e.at, e.id",
    )?;
    let rows = stmt.query_map(params![since.to_rfc3339()], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (at, finding_id, cve_id, asset_tag, from, to, note) = row?;
        if !within(&at, since, until) {
            continue;
        }
        let (Some(from), Some(to)) = (Status::parse(&from), Status::parse(&to)) else {
            continue;
        };
        out.push(AutoChange {
            at,
            finding_id,
            cve_id,
            asset_tag,
            from,
            to,
            note,
        });
    }
    Ok(out)
}

type Moves = (Vec<PendingMove>, Vec<PendingMove>);

fn pending_moves(
    conn: &Connection,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<Moves, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT cve_id, reason, entered_at, resolved_at, resolved_reason FROM pending ORDER BY entered_at, cve_id",
    )?;
    let rows = stmt.query_map(params![], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
        ))
    })?;
    let (mut entered, mut exited) = (Vec::new(), Vec::new());
    for row in rows {
        let (cve_id, reason, entered_at, resolved_at, resolved_reason) = row?;
        if within(&entered_at, since, until) {
            entered.push(PendingMove {
                cve_id: cve_id.clone(),
                at: entered_at,
                reason: PendingReason::parse(&reason).map_or(reason, |r| r.label().to_string()),
            });
        }
        if let Some(at) = resolved_at.filter(|t| within(t, since, until)) {
            let reason = resolved_reason.unwrap_or_default();
            exited.push(PendingMove {
                cve_id,
                at,
                reason: ResolvedReason::parse(&reason).map_or(reason, |r| r.label().to_string()),
            });
        }
    }
    exited.sort_by(|a, b| a.at.cmp(&b.at).then(a.cve_id.cmp(&b.cve_id)));
    Ok((entered, exited))
}
