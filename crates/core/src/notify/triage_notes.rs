//! cvewatch 中變更命中狀態時，在對應的 Redmine 議題加註記（`add_note_on_triage`）。
//!
//! 只排入送出佇列（TUI 與 `finding set` 不連網），於下次 sync 或摘要時送出；不變更議題狀態。
//! 只對已有議題（或建立中）的 CVE 加註記；一個 CVE 一則註記，列出所有變更的資產。

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use super::dispatch::KIND_ADD_NOTE;
use super::outbox::{self, Channel};
use super::redmine_content::{known_issue_cves, triage_line, NoteDraft};
use super::settings::NotifyConfig;
use crate::error::CoreError;
use crate::matching::pending_ops;
use crate::matching::triage::{self, BatchOutcome, Change, IgnoreReason, Status};

/// 是否需要在狀態變更時加註記。
pub fn enabled(cfg: &NotifyConfig) -> bool {
    cfg.redmine.enabled && cfg.redmine.add_note_on_triage
}

/// 對已變更的命中排入註記，回傳排入的註記數。
pub fn queue_notes(
    conn: &Connection,
    cfg: &NotifyConfig,
    changed: &[i64],
    change: &Change,
    now: DateTime<Utc>,
) -> Result<usize, CoreError> {
    if !enabled(cfg) || changed.is_empty() {
        return Ok(0);
    }
    let mut by_cve: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut stmt = conn.prepare_cached(
        "SELECT f.cve_id, a.asset_tag FROM finding f
         JOIN component c ON c.id = f.component_id JOIN asset a ON a.id = c.asset_id
         WHERE f.id = ?1",
    )?;
    for id in changed {
        let (cve, tag): (String, String) =
            stmt.query_row(params![id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        by_cve
            .entry(cve)
            .or_default()
            .push(triage_line(&tag, change));
    }
    let known = known_issue_cves(conn)?;
    let mut queued = 0;
    for (cve_id, lines) in by_cve {
        if !known.contains(&cve_id) {
            continue;
        }
        let note = NoteDraft {
            cve_id,
            notes: format!("cvewatch 狀態變更：\n{}", lines.join("\n")),
        };
        let json = serde_json::to_string(&note)
            .map_err(|e| CoreError::Invalid(format!("無法序列化註記：{e}")))?;
        outbox::enqueue(conn, Channel::Redmine, KIND_ADD_NOTE, &json, now)?;
        queued += 1;
    }
    Ok(queued)
}

/// 變更狀態（`finding set` 與 TUI 共用），並在同一交易中排入 Redmine 註記。
pub fn set_status_many(
    conn: &mut Connection,
    cfg: &NotifyConfig,
    ids: &[i64],
    change: &Change,
    now: DateTime<Utc>,
    skip_unchanged: bool,
) -> Result<BatchOutcome, CoreError> {
    triage::set_status_many_with(conn, ids, change, now, skip_unchanged, &|tx, changed| {
        queue_notes(tx, cfg, changed, change, now).map(|_| ())
    })
}

/// 人工移出等待區（`pending resolve` 與 TUI 共用），並在同一交易中為一併忽略的命中排入註記。
pub fn resolve_pending(
    conn: &mut Connection,
    cfg: &NotifyConfig,
    req: &pending_ops::ResolveRequest<'_>,
) -> Result<pending_ops::Resolution, CoreError> {
    pending_ops::resolve_with(conn, req, &|tx, resolution| {
        queue_dismiss_notes(tx, cfg, &resolution.ignored, req.note, req.now).map(|_| ())
    })
}

/// 等待區人工判定不相關時一併忽略的 probable 命中：排入註記。
pub fn queue_dismiss_notes(
    conn: &Connection,
    cfg: &NotifyConfig,
    ignored: &[i64],
    note: &str,
    now: DateTime<Utc>,
) -> Result<usize, CoreError> {
    let text = if note.trim().is_empty() {
        "等待區人工判定不相關".to_string()
    } else {
        format!("等待區人工判定不相關；{}", note.trim())
    };
    let change = Change {
        status: Status::Ignored,
        reason: Some(IgnoreReason::FalsePositive),
        justification: None,
        note: text,
    };
    queue_notes(conn, cfg, ignored, &change, now)
}
