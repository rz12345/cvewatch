//! 命中查詢：`cvewatch finding list | show | conflicts`（輸出格式在執行檔）。

use std::collections::BTreeSet;

use rusqlite::{params, Connection, OptionalExtension};

use super::engine::Confidence;
use super::evidence::{Detail, Source, Verdict};
use super::scoring::{rank_cmp, Priority, RankKey, ScoreInputs};
use super::triage::{Decider, Status};
use crate::assets::pending::PENDING_COMPONENT_SQL;
use crate::error::CoreError;
use crate::ingest::cvss;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FindingFilter {
    pub statuses: Vec<Status>,
    pub priorities: Vec<Priority>,
    pub asset_tag: Option<String>,
    pub confidence: Option<Confidence>,
    /// 只列出此時間（RFC 3339 UTC）之後建立的命中
    pub first_seen_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FindingRow {
    pub id: i64,
    pub asset_tag: String,
    pub asset_status: String,
    /// 產品 key 或套件 purl
    pub component: String,
    pub version: String,
    pub cve_id: String,
    pub title: Option<String>,
    pub priority: Priority,
    pub confidence: Confidence,
    pub decided_by: String,
    pub status: Status,
    pub still_vulnerable: bool,
    pub scores: ScoreInputs,
    /// 資產價值 MAX(C, I, A)
    pub asset_value: u8,
    /// 元件已自清冊或套件清單移除（命中保留但不再更新）
    pub component_removed: bool,
    /// 使用者人工判定套用
    pub manual: bool,
    /// 資產名稱（TUI 搜尋用）
    pub asset_name: String,
    /// CVE 英文描述全文（TUI 搜尋用）
    pub description: Option<String>,
    pub first_seen: String,
}

const ROW_SELECT: &str =
    "SELECT f.id, a.asset_tag, a.status, coalesce(p.key, c.purl), c.version_raw, f.cve_id,
            coalesce(cv.title, substr(cv.description_en, 1, 80)), f.priority, f.confidence,
            f.decided_by, f.status, (f.status = 'resolved' AND f.in_range), k.date_added, e.score,
            max(a.confidentiality, a.integrity, a.availability),
            c.removed_at IS NOT NULL, f.manual, a.name, cv.description_en, f.first_seen
     FROM finding f
     JOIN component c ON c.id = f.component_id
     JOIN asset a ON a.id = c.asset_id
     LEFT JOIN catalog_product p ON p.id = c.product_id
     LEFT JOIN cve cv ON cv.id = f.cve_id
     LEFT JOIN kev k ON k.cve_id = f.cve_id
     LEFT JOIN epss e ON e.cve_id = f.cve_id";

fn row_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<FindingRow> {
    let invalid = |i: usize, v: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            i,
            rusqlite::types::Type::Text,
            format!("不合法的值：{v}").into(),
        )
    };
    let priority: String = r.get(7)?;
    let confidence: String = r.get(8)?;
    let status: String = r.get(10)?;
    Ok(FindingRow {
        id: r.get(0)?,
        asset_tag: r.get(1)?,
        asset_status: r.get(2)?,
        component: r.get(3)?,
        version: r.get(4)?,
        cve_id: r.get(5)?,
        title: r.get(6)?,
        priority: Priority::parse(&priority).ok_or_else(|| invalid(7, &priority))?,
        confidence: Confidence::parse(&confidence).ok_or_else(|| invalid(8, &confidence))?,
        decided_by: r.get(9)?,
        status: Status::parse(&status).ok_or_else(|| invalid(10, &status))?,
        still_vulnerable: r.get(11)?,
        scores: ScoreInputs {
            kev_date: r.get(12)?,
            epss: r.get(13)?,
            cvss: None,
        },
        asset_value: r.get(14)?,
        component_removed: r.get(15)?,
        manual: r.get(16)?,
        asset_name: r.get(17)?,
        description: r.get(18)?,
        first_seen: r.get(19)?,
    })
}

/// 填入顯示用 CVSS（一次查詢取得所有列的 CVE）。
fn fill_cvss(conn: &Connection, rows: &mut [FindingRow]) -> Result<(), CoreError> {
    let ids: Vec<String> = rows
        .iter()
        .map(|r| r.cve_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let metrics = super::load::metrics(conn, Some(&ids))?;
    for row in rows.iter_mut() {
        row.scores.cvss = metrics
            .get(&row.cve_id)
            .and_then(|m| cvss::preferred(m))
            .map(|m| m.base_score);
    }
    Ok(())
}

/// `col IN (?, ?, …)` 與其參數。
fn in_clause(col: &str, values: Vec<&'static str>, args: &mut Vec<String>) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    let marks = vec!["?"; values.len()].join(", ");
    args.extend(values.into_iter().map(str::to_string));
    Some(format!("{col} IN ({marks})"))
}

/// 篩選條件 → `WHERE …`（沒有條件時為空字串）與參數；`list` 與 `ids` 共用。
fn where_clause(filter: &FindingFilter) -> (String, Vec<String>) {
    let mut args: Vec<String> = Vec::new();
    let mut clauses: Vec<String> = Vec::new();
    let statuses = filter.statuses.iter().map(|s| s.as_str()).collect();
    clauses.extend(in_clause("f.status", statuses, &mut args));
    let priorities = filter.priorities.iter().map(|p| p.as_str()).collect();
    clauses.extend(in_clause("f.priority", priorities, &mut args));
    if let Some(tag) = &filter.asset_tag {
        clauses.push("a.asset_tag = ? COLLATE NOCASE".into());
        args.push(tag.clone());
    }
    if let Some(c) = filter.confidence {
        clauses.push("f.confidence = ?".into());
        args.push(c.as_str().to_string());
    }
    if let Some(from) = &filter.first_seen_from {
        clauses.push("f.first_seen >= ?".into());
        args.push(from.clone());
    }
    if clauses.is_empty() {
        return (String::new(), args);
    }
    (format!(" WHERE {}", clauses.join(" AND ")), args)
}

/// 依篩選條件列出命中（篩選在 SQL 中進行），依風險排序（見 [`rank_cmp`]）。
pub fn list(conn: &Connection, filter: &FindingFilter) -> Result<Vec<FindingRow>, CoreError> {
    let (clause, args) = where_clause(filter);
    let mut stmt = conn.prepare(&format!("{ROW_SELECT}{clause}"))?;
    let mut out = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), row_from)?
        .collect::<Result<Vec<_>, _>>()?;
    fill_cvss(conn, &mut out)?;
    out.sort_by(|a, b| rank_cmp(&rank_key(a), &rank_key(b)).then(a.id.cmp(&b.id)));
    Ok(out)
}

/// 符合篩選條件的命中 id（依 id 排序；不計算顯示欄位，供匯出等只需 id 的用途）。
pub fn ids(conn: &Connection, filter: &FindingFilter) -> Result<Vec<i64>, CoreError> {
    let (clause, args) = where_clause(filter);
    let sql = format!(
        "SELECT f.id FROM finding f
         JOIN component c ON c.id = f.component_id
         JOIN asset a ON a.id = c.asset_id{clause}
         ORDER BY f.id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| r.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn rank_key(r: &FindingRow) -> RankKey {
    RankKey {
        priority: r.priority,
        inputs: r.scores.clone(),
        asset_value: r.asset_value,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceRow {
    pub source: Source,
    pub verdict: Verdict,
    pub summary: String,
    pub fixed_versions: Vec<String>,
    pub notes: Vec<String>,
}

/// 時間軸操作者的中文名稱（CLI 與 TUI 共用）。
pub fn actor_label(actor: &str) -> &'static str {
    match actor {
        Decider::USER_ACTOR => "使用者",
        Decider::IMPORT_ACTOR => "匯入",
        _ => "系統",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRow {
    pub at: String,
    pub actor: String,
    pub from: Option<Status>,
    pub to: Status,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FindingDetail {
    pub row: FindingRow,
    pub ignore_reason: Option<String>,
    pub justification: Option<String>,
    pub first_seen: String,
    pub last_evaluated: String,
    pub in_range: bool,
    pub evidence: Vec<EvidenceRow>,
    pub events: Vec<EventRow>,
    pub conflicts: Vec<ConflictRow>,
}

pub fn show(conn: &Connection, id: i64) -> Result<Option<FindingDetail>, CoreError> {
    let sql = format!("{ROW_SELECT} WHERE f.id = ?1");
    let Some(mut row) = conn.query_row(&sql, params![id], row_from).optional()? else {
        return Ok(None);
    };
    fill_cvss(conn, std::slice::from_mut(&mut row))?;
    let (ignore_reason, justification, first_seen, last_evaluated, in_range) = conn.query_row(
        "SELECT ignore_reason, justification, first_seen, last_evaluated, in_range FROM finding WHERE id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    let evidence = evidence(conn, id)?;
    let conflicts = conflicts_where(conn, Some(id))?;
    Ok(Some(FindingDetail {
        events: events(conn, id)?,
        row,
        ignore_reason,
        justification,
        first_seen,
        last_evaluated,
        in_range,
        evidence,
        conflicts,
    }))
}

/// 證據列；內容損壞或無法辨識的列記錄警告後略過。
fn evidence(conn: &Connection, id: i64) -> Result<Vec<EvidenceRow>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT source, verdict, detail_json FROM finding_evidence WHERE finding_id = ?1 ORDER BY rowid",
    )?;
    let rows = stmt
        .query_map(params![id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(s, v, d)| evidence_row(id, &s, &v, &d))
        .collect())
}

fn evidence_row(id: i64, source: &str, verdict: &str, detail: &str) -> Option<EvidenceRow> {
    let detail: Detail = serde_json::from_str(detail).unwrap_or_else(|e| {
        tracing::warn!(finding = id, source, error = %e, "證據內容無法解析，顯示為空白");
        Detail::default()
    });
    let (Some(source_enum), Some(verdict_enum)) = (Source::parse(source), Verdict::parse(verdict))
    else {
        tracing::warn!(finding = id, source, verdict, "略過無法辨識的證據列");
        return None;
    };
    Some(EvidenceRow {
        source: source_enum,
        verdict: verdict_enum,
        summary: detail.summary,
        fixed_versions: detail.fixed_versions,
        notes: detail.notes,
    })
}

fn events(conn: &Connection, id: i64) -> Result<Vec<EventRow>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT at, actor, from_status, to_status, note FROM finding_event WHERE finding_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![id], |r| {
        let from: Option<String> = r.get(2)?;
        let to: String = r.get(3)?;
        Ok((r.get(0)?, r.get(1)?, from, to, r.get(4)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (at, actor, from, to, note): (String, String, Option<String>, String, String) = row?;
        let status = |text: &str| super::triage::parse_status(id, text);
        out.push(EventRow {
            at,
            actor,
            from: from.as_deref().map(status).transpose()?,
            to: status(&to)?,
            note,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRow {
    pub asset_tag: String,
    pub component: String,
    pub version: String,
    pub cve_id: String,
    pub final_source: String,
    pub final_verdict: String,
    pub other_source: String,
    pub other_verdict: String,
    pub at: String,
}

/// 來源矛盾：最終判定不受影響、但有來源判定受影響（版本待補元件的舊紀錄不列出，與 Dashboard 一致）。
pub fn conflicts(conn: &Connection) -> Result<Vec<ConflictRow>, CoreError> {
    conflicts_where(conn, None)
}

/// `finding_id` 為 Some 時只列該命中的（元件 × CVE）。
fn conflicts_where(
    conn: &Connection,
    finding_id: Option<i64>,
) -> Result<Vec<ConflictRow>, CoreError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT c.asset_tag, coalesce(p.key, c.purl), c.version_raw, m.cve_id, m.final_source,
                m.final_verdict, m.other_source, m.other_verdict, m.at
         FROM match_conflict m
         JOIN active_component c ON c.id = m.component_id
         LEFT JOIN catalog_product p ON p.id = c.product_id
         WHERE (?1 IS NULL OR (m.component_id, m.cve_id) =
                (SELECT component_id, cve_id FROM finding WHERE id = ?1))
           AND NOT ({PENDING_COMPONENT_SQL})
         ORDER BY m.cve_id DESC, c.asset_tag, m.other_source"
    ))?;
    let rows = stmt.query_map(params![finding_id], |r| {
        Ok(ConflictRow {
            asset_tag: r.get(0)?,
            component: r.get(1)?,
            version: r.get(2)?,
            cve_id: r.get(3)?,
            final_source: r.get(4)?,
            final_verdict: r.get(5)?,
            other_source: r.get(6)?,
            other_verdict: r.get(7)?,
            at: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}
