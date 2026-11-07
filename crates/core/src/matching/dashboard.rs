//! TUI Dashboard 的統計：各等級未處理命中、KEV、等待區、來源矛盾與各來源最後一次同步。

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use super::pending::stale_cutoff;
use super::run::MATCH_LAST_RUN_KEY;
use super::scoring::Priority;
use crate::db::lock::stale_after;
use crate::db::meta;
use crate::error::CoreError;
use crate::ingest::runlog::{self, FinishedMarker, RunRow};

/// 來源最近一次結束的同步，以及之後仍在執行（未逾時）的同步。
///
/// 程序被終止或關機而沒有結束的紀錄超過 sync 鎖的逾時（[`stale_after`]）後視為中斷，
/// 不再顯示為執行中，也不會蓋過該來源先前的失敗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceStatus {
    pub source: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// `running` / `success` / `failed`
    pub status: String,
    pub error: Option<String>,
    /// 最近一次結束之後開始、仍在執行的同步的開始時間
    pub running_since: Option<String>,
}

impl SourceStatus {
    pub fn failed(&self) -> bool {
        self.status == "failed"
    }

    pub fn running(&self) -> bool {
        self.running_since.is_some()
    }

    /// 有結束的同步紀錄。
    pub fn has_finished(&self) -> bool {
        self.finished_at.is_some()
    }

    /// 顯示用時間：結束時間，執行中則為開始時間。
    pub fn time(&self) -> &str {
        self.finished_at.as_deref().unwrap_or(&self.started_at)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dashboard {
    /// P1–P4 的未處理命中數（依等級順序，永遠四筆）
    pub open_by_priority: Vec<(Priority, i64)>,
    /// 未處理命中中 CVE 已列入 KEV 者
    pub kev_open: i64,
    pub pending: i64,
    pub pending_stale: i64,
    /// 來源矛盾（使用中元件）
    pub conflicts: i64,
    /// 無法送出（dead）的通知
    pub dead_notifications: i64,
    /// 各來源最近一次同步，依同步順序（cvelist → nvd → csaf:* → kev → epss → osv → 其他）
    pub sources: Vec<SourceStatus>,
}

impl Dashboard {
    /// 最近一次結束的同步（任一來源）。
    pub fn last_finished(&self) -> Option<&SourceStatus> {
        self.sources
            .iter()
            .filter(|s| s.finished_at.is_some())
            .max_by(|a, b| a.finished_at.cmp(&b.finished_at))
    }
}

pub fn load(
    conn: &Connection,
    stale_days: u32,
    now: DateTime<Utc>,
) -> Result<Dashboard, CoreError> {
    let (pending, pending_stale) = conn.query_row(
        "SELECT count(*), coalesce(sum(entered_at < ?1), 0) FROM pending WHERE resolved_at IS NULL",
        params![stale_cutoff(now, stale_days)],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(Dashboard {
        open_by_priority: open_by_priority(conn)?,
        kev_open: conn.query_row(
            "SELECT count(*) FROM finding f JOIN kev k ON k.cve_id = f.cve_id WHERE f.status = 'open'",
            [],
            |r| r.get(0),
        )?,
        pending,
        pending_stale,
        conflicts: conn.query_row(
            "SELECT count(*) FROM match_conflict m JOIN active_component c ON c.id = m.component_id",
            [],
            |r| r.get(0),
        )?,
        dead_notifications: conn.query_row(
            "SELECT count(*) FROM outbox WHERE dead_at IS NOT NULL",
            [],
            |r| r.get(0),
        )?,
        sources: sources(conn, now)?,
    })
}

/// 偵測「有新資料」的標記：任一來源同步結束，或全量比對完成（`cvewatch sync` 結尾的
/// 自動比對、`cvewatch match`、資產匯入後的比對）時改變。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DataMarker {
    pub sync: FinishedMarker,
    pub last_match: Option<String>,
}

pub fn data_marker(conn: &Connection) -> Result<DataMarker, CoreError> {
    Ok(DataMarker {
        sync: runlog::finished_marker(conn)?,
        last_match: meta::get(conn, MATCH_LAST_RUN_KEY)?,
    })
}

/// 未結束且開始時間早於此時間的同步紀錄視為中斷。
pub fn interrupted_cutoff(now: DateTime<Utc>) -> String {
    (now - stale_after()).to_rfc3339()
}

/// 同步紀錄沒有結束、且已超過 sync 鎖逾時（程序被終止或關機）。
pub fn is_interrupted(row: &RunRow, now: DateTime<Utc>) -> bool {
    row.finished_at.is_none() && row.started_at < interrupted_cutoff(now)
}

fn open_by_priority(conn: &Connection) -> Result<Vec<(Priority, i64)>, CoreError> {
    let mut stmt =
        conn.prepare("SELECT count(*) FROM finding WHERE status = 'open' AND priority = ?1")?;
    Priority::ALL
        .iter()
        .map(|p| Ok((*p, stmt.query_row(params![p.as_str()], |r| r.get(0))?)))
        .collect()
}

fn sources(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<SourceStatus>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT source, started_at, finished_at, status, error FROM sync_run
         WHERE id IN (SELECT max(id) FROM sync_run WHERE finished_at IS NOT NULL GROUP BY source)",
    )?;
    let mut rows = stmt
        .query_map([], |r| {
            Ok(SourceStatus {
                source: r.get(0)?,
                started_at: r.get(1)?,
                finished_at: r.get(2)?,
                status: r.get(3)?,
                error: r.get(4)?,
                running_since: None,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (source, since) in running(conn, now)? {
        match rows.iter_mut().find(|r| r.source == source) {
            Some(row) => row.running_since = Some(since),
            None => rows.push(SourceStatus {
                source,
                started_at: since.clone(),
                finished_at: None,
                status: "running".into(),
                error: None,
                running_since: Some(since),
            }),
        }
    }
    rows.sort_by(|a, b| {
        source_rank(&a.source)
            .cmp(&source_rank(&b.source))
            .then_with(|| a.source.cmp(&b.source))
    });
    Ok(rows)
}

/// 各來源在最近一次結束之後開始、尚未逾時的同步。
fn running(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<(String, String)>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT r.source, max(r.started_at) FROM sync_run r
         WHERE r.finished_at IS NULL AND r.started_at >= ?1
           AND r.id > coalesce((SELECT max(f.id) FROM sync_run f
                                WHERE f.source = r.source AND f.finished_at IS NOT NULL), 0)
         GROUP BY r.source",
    )?;
    let rows = stmt.query_map(params![interrupted_cutoff(now)], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// `cvewatch sync` 的執行順序。
fn source_rank(source: &str) -> usize {
    const ORDER: [&str; 6] = ["cvelist", "nvd", "csaf", "kev", "epss", "osv"];
    let family = source.split(':').next().unwrap_or(source);
    ORDER
        .iter()
        .position(|s| *s == family)
        .unwrap_or(ORDER.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate::migrate(&mut conn).unwrap();
        conn
    }

    fn run(conn: &Connection, source: &str, started: &str, finished: Option<&str>, status: &str) {
        conn.execute(
            "INSERT INTO sync_run (started_at, finished_at, source, status) VALUES (?1, ?2, ?3, ?4)",
            params![started, finished, source, status],
        )
        .unwrap();
    }

    #[test]
    fn empty_database_has_four_zero_priorities() {
        let d = load(&db(), 14, Utc::now()).unwrap();
        assert_eq!(
            d.open_by_priority,
            Priority::ALL.iter().map(|p| (*p, 0)).collect::<Vec<_>>()
        );
        assert!(d.sources.is_empty());
        assert!(d.last_finished().is_none());
    }

    #[test]
    fn dead_notifications_are_counted() {
        let conn = db();
        let now = Utc::now();
        let dead = crate::notify::outbox::enqueue(
            &conn,
            crate::notify::outbox::Channel::Email,
            "urgent",
            "{}",
            now,
        )
        .unwrap();
        crate::notify::outbox::fail(&conn, dead, "x", true, now).unwrap();
        crate::notify::outbox::enqueue(
            &conn,
            crate::notify::outbox::Channel::Email,
            "digest",
            "{}",
            now,
        )
        .unwrap();
        assert_eq!(load(&conn, 14, now).unwrap().dead_notifications, 1);
    }

    #[test]
    fn sources_use_latest_run_in_sync_order() {
        let conn = db();
        run(
            &conn,
            "osv",
            "2026-11-01T01:00",
            Some("2026-11-01T01:05"),
            "failed",
        );
        run(
            &conn,
            "kev",
            "2026-11-01T00:00",
            Some("2026-11-01T00:01"),
            "failed",
        );
        run(
            &conn,
            "kev",
            "2026-11-01T02:00",
            Some("2026-11-01T02:01"),
            "success",
        );
        run(
            &conn,
            "csaf:paloalto",
            "2026-11-01T03:00:00+00:00",
            None,
            "running",
        );
        run(
            &conn,
            "cvelist",
            "2026-11-01T00:00",
            Some("2026-11-01T00:02"),
            "success",
        );
        let now = DateTime::parse_from_rfc3339("2026-11-01T04:00:00+00:00")
            .unwrap()
            .to_utc();
        let d = load(&conn, 14, now).unwrap();
        let names: Vec<&str> = d.sources.iter().map(|s| s.source.as_str()).collect();
        assert_eq!(names, ["cvelist", "csaf:paloalto", "kev", "osv"]);
        assert_eq!(d.sources[2].status, "success");
        assert!(d.sources[3].failed());
        assert!(d.sources[1].running());
        assert_eq!(d.last_finished().unwrap().source, "kev");
    }

    #[test]
    fn interrupted_run_does_not_hide_earlier_failure() {
        let conn = db();
        run(
            &conn,
            "nvd",
            "2026-11-01T00:00:00+00:00",
            Some("2026-11-01T00:05:00+00:00"),
            "failed",
        );
        // 程序被終止：沒有結束時間，且已超過 sync 鎖逾時
        run(&conn, "nvd", "2026-11-02T00:00:00+00:00", None, "running");
        let now = DateTime::parse_from_rfc3339("2026-11-02T03:00:00+00:00")
            .unwrap()
            .to_utc();
        let d = load(&conn, 14, now).unwrap();
        assert_eq!(d.sources.len(), 1);
        assert!(d.sources[0].failed() && !d.sources[0].running());
        let rows = runlog::history(&conn, 10).unwrap();
        assert!(is_interrupted(&rows[0], now));
        assert!(!is_interrupted(&rows[1], now));
        // 尚未逾時：顯示為執行中，但仍保留上次失敗
        let soon = DateTime::parse_from_rfc3339("2026-11-02T00:30:00+00:00")
            .unwrap()
            .to_utc();
        let d = load(&conn, 14, soon).unwrap();
        assert!(d.sources[0].failed() && d.sources[0].running());
    }
}
