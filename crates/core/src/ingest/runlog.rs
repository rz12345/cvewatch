//! `sync_run`：每個來源每次執行的開始/結束時間、狀態、新增/更新筆數與錯誤。

use rusqlite::{params, Connection};

use super::source::SyncStats;
use crate::error::CoreError;

/// `sync --history` 預設列出的筆數。
pub const HISTORY_LIMIT: u32 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRow {
    pub id: i64,
    pub source: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub added: i64,
    pub updated: i64,
    pub error: Option<String>,
}

pub fn start(conn: &Connection, source: &str, now: &str) -> Result<i64, CoreError> {
    conn.execute(
        "INSERT INTO sync_run (started_at, source, status) VALUES (?1, ?2, 'running')",
        params![now, source],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 成功：記錄筆數；`notes`（例如略過的文件）寫入 error 欄位供查閱。
pub fn succeed(conn: &Connection, id: i64, stats: &SyncStats, now: &str) -> Result<(), CoreError> {
    let notes = (!stats.notes.is_empty()).then(|| stats.notes.join("；"));
    finish(conn, id, "success", stats, notes.as_deref(), now)
}

pub fn fail(conn: &Connection, id: i64, error: &str, now: &str) -> Result<(), CoreError> {
    finish(conn, id, "failed", &SyncStats::default(), Some(error), now)
}

fn finish(
    conn: &Connection,
    id: i64,
    status: &str,
    stats: &SyncStats,
    error: Option<&str>,
    now: &str,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE sync_run SET finished_at = ?2, status = ?3, added_count = ?4, updated_count = ?5,
                item_count = ?4 + ?5, error = ?6
         WHERE id = ?1",
        params![
            id,
            now,
            status,
            stats.added as i64,
            stats.updated as i64,
            error
        ],
    )?;
    Ok(())
}

/// 最近 `limit` 筆，最新在前。
pub fn history(conn: &Connection, limit: u32) -> Result<Vec<RunRow>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, source, started_at, finished_at, status, added_count, updated_count, error
         FROM sync_run ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit], |r| {
            Ok(RunRow {
                id: r.get(0)?,
                source: r.get(1)?,
                started_at: r.get(2)?,
                finished_at: r.get(3)?,
                status: r.get(4)?,
                added: r.get(5)?,
                updated: r.get(6)?,
                error: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_success_and_failure() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate::migrate(&mut conn).unwrap();
        let a = start(&conn, "kev", "t0").unwrap();
        let stats = SyncStats {
            added: 3,
            updated: 1,
            notes: vec!["n".into()],
        };
        succeed(&conn, a, &stats, "t1").unwrap();
        let b = start(&conn, "nvd", "t2").unwrap();
        fail(&conn, b, "HTTP 503", "t3").unwrap();
        let rows = history(&conn, HISTORY_LIMIT).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].source, "nvd");
        assert_eq!(rows[0].status, "failed");
        assert_eq!(rows[0].error.as_deref(), Some("HTTP 503"));
        assert_eq!((rows[1].added, rows[1].updated), (3, 1));
        let items: i64 = conn
            .query_row("SELECT item_count FROM sync_run WHERE id = ?1", [a], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(items, 4);
    }
}
