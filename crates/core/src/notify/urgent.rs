//! 緊急事件判定與去重。
//!
//! ```text
//! urgent(finding) := status = open AND (CVE 在 KEV OR 顯示用 CVSS ≥ notify.urgent_cvss)
//! NEW_URGENT  新建立（或首次符合）且符合 urgent
//! BECAME_KEV  既有命中的 CVE 新列入 KEV
//! ```
//!
//! 以目前資料狀態與 `notification_log` 比對（不依賴單次比對的結果），因此 `cvewatch match`、
//! 資產匯入或等待區套用產生的緊急命中也會在下一次 sync 時通知，不會漏掉。
//! 只看使用中資產、未移除元件的命中；`probable` 一律納入（寧可多報）。

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use super::outbox::Channel;
use crate::error::CoreError;
use crate::matching::engine::Confidence;
use crate::matching::query::{self, FindingFilter, FindingRow};
use crate::matching::triage::Status;

string_enum!(
    EventKind {
        NewUrgent => "NEW_URGENT",
        BecameKev => "BECAME_KEV",
    }
);

impl EventKind {
    pub fn label(self) -> &'static str {
        match self {
            EventKind::NewUrgent => "新緊急命中",
            EventKind::BecameKev => "列入 KEV",
        }
    }
}

/// KEV 資訊。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KevInfo {
    pub date_added: String,
    pub due_date: Option<String>,
}

/// 緊急事件（含通知內容需要的命中欄位）。
#[derive(Debug, Clone, PartialEq)]
pub struct UrgentEvent {
    pub kind: EventKind,
    pub finding: FindingRow,
    pub kev: Option<KevInfo>,
}

impl UrgentEvent {
    pub fn is_probable(&self) -> bool {
        self.finding.confidence == Confidence::Probable
    }
}

/// 命中是否符合緊急條件（不看狀態）。
pub fn is_urgent(row: &FindingRow, urgent_cvss: f64) -> bool {
    row.scores.in_kev() || row.scores.cvss.is_some_and(|c| c >= urgent_cvss)
}

/// 使用中資產、未移除元件、未處理且符合緊急條件的命中（依風險排序）。
pub fn urgent_findings(conn: &Connection, urgent_cvss: f64) -> Result<Vec<FindingRow>, CoreError> {
    let filter = FindingFilter {
        statuses: vec![Status::Open],
        ..FindingFilter::default()
    };
    Ok(query::list(conn, &filter)?
        .into_iter()
        .filter(|r| r.asset_status == "active" && !r.component_removed)
        .filter(|r| is_urgent(r, urgent_cvss))
        .collect())
}

/// 某管道已排入的通知：命中 → [(事件種類, 通知當時是否在 KEV)]。
fn logged(
    conn: &Connection,
    channel: Channel,
) -> Result<HashMap<i64, Vec<(String, bool)>>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT finding_id, event_kind, kev FROM notification_log
         WHERE channel = ?1 AND finding_id IS NOT NULL",
    )?;
    let mut out: HashMap<i64, Vec<(String, bool)>> = HashMap::new();
    let rows = stmt.query_map(params![channel.as_str()], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, bool>(2)?,
        ))
    })?;
    for row in rows {
        let (id, kind, kev) = row?;
        out.entry(id).or_default().push((kind, kev));
    }
    Ok(out)
}

fn kev_info(conn: &Connection) -> Result<HashMap<String, KevInfo>, CoreError> {
    let mut stmt = conn.prepare("SELECT cve_id, date_added, due_date FROM kev")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            KevInfo {
                date_added: r.get(1)?,
                due_date: r.get(2)?,
            },
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// 尚未通知過的命中：KEV 加入日期晚於命中建立日期 → 列入 KEV；否則為新緊急命中。
fn first_event(row: &FindingRow) -> EventKind {
    let first_seen_day = row.first_seen.get(..10).unwrap_or(&row.first_seen);
    match &row.scores.kev_date {
        Some(added) if added.as_str() > first_seen_day => EventKind::BecameKev,
        _ => EventKind::NewUrgent,
    }
}

/// 已通知過的命中：之前的通知都不在 KEV、現在列入 → 列入 KEV（每個命中只一次）。
fn later_event(row: &FindingRow, history: &[(String, bool)]) -> Option<EventKind> {
    let kev_notified = history
        .iter()
        .any(|(kind, kev)| *kev || kind == EventKind::BecameKev.as_str());
    (row.scores.in_kev() && !kev_notified).then_some(EventKind::BecameKev)
}

/// 指定管道尚未通知的緊急事件。
pub fn detect(
    conn: &Connection,
    channel: Channel,
    urgent_cvss: f64,
) -> Result<Vec<UrgentEvent>, CoreError> {
    detect_in(conn, channel, &urgent_findings(conn, urgent_cvss)?)
}

/// 以預先載入的緊急命中（[`urgent_findings`]）判定指定管道尚未通知的事件。
pub fn detect_in(
    conn: &Connection,
    channel: Channel,
    urgent_rows: &[FindingRow],
) -> Result<Vec<UrgentEvent>, CoreError> {
    let history = logged(conn, channel)?;
    let kev = kev_info(conn)?;
    let events = urgent_rows
        .iter()
        .cloned()
        .filter_map(|row| {
            let kind = match history.get(&row.id) {
                None => Some(first_event(&row)),
                Some(h) => later_event(&row, h),
            }?;
            Some(UrgentEvent {
                kind,
                kev: kev.get(&row.cve_id).cloned(),
                finding: row,
            })
        })
        .collect();
    Ok(events)
}

/// 記錄已排入佇列的事件（去重依據）；`outbox_id` 為 None 表示不需送出（例如合併在他處）。
pub fn record(
    conn: &Connection,
    channel: Channel,
    events: &[UrgentEvent],
    outbox_id: Option<i64>,
    now: DateTime<Utc>,
) -> Result<(), CoreError> {
    let mut stmt = conn.prepare_cached(
        "INSERT OR IGNORE INTO notification_log
             (channel, event_kind, cve_id, finding_id, kev, queued_at, sent_at, outbox_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, CASE WHEN ?7 IS NULL THEN ?6 END, ?7)",
    )?;
    let at = now.to_rfc3339();
    for e in events {
        stmt.execute(params![
            channel.as_str(),
            e.kind.as_str(),
            e.finding.cve_id,
            e.finding.id,
            e.finding.scores.in_kev(),
            at,
            outbox_id
        ])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::scoring::{Priority, ScoreInputs};

    fn row(first_seen: &str, kev: Option<&str>) -> FindingRow {
        FindingRow {
            id: 1,
            asset_tag: "FW-01".into(),
            asset_status: "active".into(),
            component: "paloalto-panos".into(),
            version: "11.1.2".into(),
            cve_id: "CVE-2026-0001".into(),
            title: None,
            priority: Priority::P1,
            confidence: Confidence::Confirmed,
            decided_by: "cna".into(),
            status: Status::Open,
            still_vulnerable: false,
            scores: ScoreInputs {
                kev_date: kev.map(str::to_string),
                epss: None,
                cvss: Some(7.5),
            },
            criticality: 2,
            component_removed: false,
            manual: false,
            asset_name: "FW-01".into(),
            description: None,
            first_seen: first_seen.into(),
        }
    }

    #[test]
    fn kev_added_after_first_seen_is_became_kev() {
        let r = row("2026-10-24T00:00:00+00:00", Some("2026-11-07"));
        assert_eq!(first_event(&r), EventKind::BecameKev);
        let r = row("2026-11-07T00:00:00+00:00", Some("2024-01-01"));
        assert_eq!(first_event(&r), EventKind::NewUrgent);
        let r = row("2026-11-07T00:00:00+00:00", None);
        assert_eq!(first_event(&r), EventKind::NewUrgent);
    }

    #[test]
    fn kev_event_only_once_and_not_after_kev_notice() {
        let r = row("2026-10-24T00:00:00+00:00", Some("2026-11-07"));
        let before_kev = vec![("NEW_URGENT".to_string(), false)];
        assert_eq!(later_event(&r, &before_kev), Some(EventKind::BecameKev));
        let with_kev = vec![("NEW_URGENT".to_string(), true)];
        assert_eq!(later_event(&r, &with_kev), None);
        let became = vec![
            ("NEW_URGENT".to_string(), false),
            ("BECAME_KEV".to_string(), true),
        ];
        assert_eq!(later_event(&r, &became), None);
        let not_kev = row("2026-10-24T00:00:00+00:00", None);
        assert_eq!(later_event(&not_kev, &before_kev), None);
    }
}
