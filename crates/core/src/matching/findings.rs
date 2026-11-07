//! 將判定寫入 `finding` / `finding_evidence` / `match_conflict`，並套用自動狀態轉換。
//!
//! | 觸發 | 轉換 |
//! |---|---|
//! | 新命中 | → open |
//! | 上次仍受影響、這次已不受影響，status = open | → resolved（「版本已不在受影響範圍」） |
//! | CVE 變為 REJECTED，status = open | → ignored（false_positive，「CVE 已撤銷」） |
//! | status = ignored 的 CVE 新列入 KEV | → open（「已列入 KEV，重新開啟」） |
//! | status = resolved 但重算後仍受影響 | 不變，in_range = true（顯示「已處理但仍受影響」） |
//!
//! 使用者設定的狀態、原因與備註在重算後保留；退役資產的元件不在此處理（命中保留但不再更新）。

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use super::engine::{Confidence, Decision};
use super::evaluate::PairDecision;
use super::load::{ids_json, ActiveComponent, MatchData};
use super::scoring::{priority, Priority, ScoreInputs, ScoringConfig};
use super::triage::{record_event, Actor, IgnoreReason, Status};
use crate::error::CoreError;

pub const NOTE_REJECTED: &str = "CVE 已撤銷";
pub const NOTE_KEV_REOPEN: &str = "已列入 KEV，重新開啟";
pub const NOTE_OUT_OF_RANGE: &str = "版本已不在受影響範圍";
pub const NOTE_NO_LONGER_LISTED: &str = "比對來源已不再列出此元件的產品";
pub const NOTE_DISMISSED: &str = "等待區已人工判定此 CVE 不相關，自動忽略";
pub const NOTE_CONFIRMED_AFTER_DISMISS: &str = "資料確定受影響，重新開啟（先前於等待區判定不相關）";
/// 人工建立、尚無來源判定時的 `decided_by`。
pub const MANUAL: &str = "manual";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingFinding {
    pub id: i64,
    pub status: Status,
    /// 使用者人工判定套用
    pub manual: bool,
    pub kev_listed: bool,
    pub in_range: bool,
    /// 因等待區判定不相關而忽略
    pub dismissed: bool,
}

/// 新命中的欄位（比對與人工套用共用）。
pub struct NewFinding<'a> {
    pub component_id: i64,
    pub cve_id: &'a str,
    pub confidence: Confidence,
    /// 證據來源或 `manual`
    pub decided_by: &'a str,
    pub priority: Priority,
    pub kev_listed: bool,
    /// 使用者人工判定套用
    pub manual: bool,
    /// 一般為 open；等待區已判定不相關的 CVE 新出現的 probable 命中直接為 ignored
    pub status: Status,
    pub ignore_reason: Option<IgnoreReason>,
    /// 因等待區判定不相關而忽略（之後資料確定受影響時自動重新開啟）
    pub dismissed: bool,
    pub now: &'a str,
}

/// 寫入一筆命中（狀態依 `NewFinding::status`），回傳 id。
pub fn insert_finding(conn: &Connection, f: &NewFinding<'_>) -> Result<i64, CoreError> {
    conn.execute(
        "INSERT INTO finding (component_id, cve_id, confidence, decided_by, priority, status,
                              ignore_reason, first_seen, last_evaluated, in_range, kev_listed,
                              manual, dismissed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1, ?9, ?10, ?11)",
        params![
            f.component_id,
            f.cve_id,
            f.confidence.as_str(),
            f.decided_by,
            f.priority.as_str(),
            f.status.as_str(),
            f.ignore_reason.map(IgnoreReason::as_str),
            f.now,
            f.kev_listed,
            f.manual,
            f.dismissed
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 一次比對的統計。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyStats {
    pub new: usize,
    pub resolved: usize,
    pub reopened: usize,
    pub rejected: usize,
    pub still_vulnerable: usize,
    pub conflicts: usize,
    /// 等待區已判定不相關而自動忽略的 probable 命中
    pub dismissed: usize,
}

type Key = (i64, String);

/// active 元件的既有命中。
pub fn load_existing(
    conn: &Connection,
    active: &BTreeSet<i64>,
) -> Result<HashMap<Key, ExistingFinding>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, component_id, cve_id, status, manual, kev_listed, in_range, dismissed
         FROM finding WHERE component_id IN (SELECT value FROM json_each(?1))",
    )?;
    let ids = super::load::ids_json(&active.iter().copied().collect::<Vec<_>>())?;
    let mut rows = stmt.query(params![ids])?;
    let mut out = HashMap::new();
    while let Some(r) = rows.next()? {
        let component_id: i64 = r.get(1)?;
        let status: String = r.get(3)?;
        let status = super::triage::parse_status(r.get(0)?, &status)?;
        out.insert(
            (component_id, r.get(2)?),
            ExistingFinding {
                id: r.get(0)?,
                status,
                manual: r.get(4)?,
                kev_listed: r.get(5)?,
                in_range: r.get(6)?,
                dismissed: r.get(7)?,
            },
        );
    }
    Ok(out)
}

struct Ctx<'a> {
    conn: &'a Connection,
    data: &'a MatchData,
    dismissed: &'a BTreeMap<String, bool>,
    components: HashMap<i64, &'a ActiveComponent>,
    scoring: &'a ScoringConfig,
    now: String,
    stats: ApplyStats,
}

impl Ctx<'_> {
    fn event(
        &self,
        id: i64,
        from: Option<Status>,
        to: Status,
        note: &str,
    ) -> Result<(), CoreError> {
        record_event(self.conn, id, &self.now, Actor::System, from, to, note)
    }

    fn replace_evidence(&self, id: i64, decision: &Decision) -> Result<(), CoreError> {
        let wanted: Vec<(String, String, String)> = decision
            .evidence
            .iter()
            .map(|e| {
                Ok((
                    e.source.as_str().to_string(),
                    e.verdict.as_str().to_string(),
                    e.detail_json()?,
                ))
            })
            .collect::<Result<_, CoreError>>()?;
        let mut current = self.conn.prepare_cached(
            "SELECT source, verdict, detail_json FROM finding_evidence WHERE finding_id = ?1 ORDER BY rowid",
        )?;
        let stored = current
            .query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<(String, String, String)>, _>>()?;
        if stored == wanted {
            return Ok(());
        }
        self.conn.execute(
            "DELETE FROM finding_evidence WHERE finding_id = ?1",
            params![id],
        )?;
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO finding_evidence (finding_id, source, verdict, detail_json)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (source, verdict, detail) in &wanted {
            stmt.execute(params![id, source, verdict, detail])?;
        }
        Ok(())
    }

    fn priority_of(&self, component: &ActiveComponent, scores: &ScoreInputs) -> Priority {
        priority(scores, component.exposure, self.scoring)
    }
}

/// 一次比對的共用輸入（只計算一次再往下傳）。
pub struct RunState<'a> {
    pub data: &'a MatchData,
    /// 已撤銷的 CVE
    pub rejected: &'a BTreeSet<String>,
    /// 等待區已人工判定不相關的 CVE → 判定當時是否已在本機 KEV
    pub dismissed: &'a BTreeMap<String, bool>,
}

/// 寫入全部判定並套用自動規則。
pub fn apply(
    conn: &Connection,
    state: &RunState<'_>,
    decisions: &[PairDecision],
    existing: &HashMap<Key, ExistingFinding>,
    scoring: &ScoringConfig,
    now: DateTime<Utc>,
) -> Result<ApplyStats, CoreError> {
    let data = state.data;
    let mut ctx = Ctx {
        conn,
        data,
        dismissed: state.dismissed,
        components: data.components.iter().map(|c| (c.id, c)).collect(),
        scoring,
        now: now.to_rfc3339(),
        stats: ApplyStats::default(),
    };
    write_conflicts(&mut ctx, decisions)?;
    let mut decided = BTreeSet::new();
    for d in decisions {
        let key = (d.component_id, d.cve_id.clone());
        match existing.get(&key) {
            None => insert(&mut ctx, d)?,
            Some(f) => update(&mut ctx, f, d)?,
        }
        decided.insert(key);
    }
    let mut undecided_ids = Vec::new();
    for (key, f) in existing.iter().filter(|(k, _)| !decided.contains(*k)) {
        undecided(&mut ctx, key, f, state.rejected.contains(&key.1))?;
        undecided_ids.push(f.id);
    }
    // 本次沒有判定：上次的證據已不代表目前資料，一次清除（時間軸保留狀態變化）
    if !undecided_ids.is_empty() {
        conn.execute(
            "DELETE FROM finding_evidence WHERE finding_id IN (SELECT value FROM json_each(?1))",
            params![ids_json(&undecided_ids)?],
        )?;
    }
    Ok(ctx.stats)
}

fn write_conflicts(ctx: &mut Ctx<'_>, decisions: &[PairDecision]) -> Result<(), CoreError> {
    let ids: Vec<i64> = ctx.components.keys().copied().collect();
    ctx.conn.execute(
        "DELETE FROM match_conflict WHERE component_id IN (SELECT value FROM json_each(?1))",
        params![ids_json(&ids)?],
    )?;
    let mut stmt = ctx.conn.prepare_cached(
        "INSERT OR REPLACE INTO match_conflict (component_id, cve_id, final_source, final_verdict,
                                                other_source, other_verdict, at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for d in decisions {
        for c in &d.decision.conflicts {
            stmt.execute(params![
                d.component_id,
                d.cve_id,
                c.final_source.as_str(),
                c.final_verdict.as_str(),
                c.other_source.as_str(),
                c.other_verdict.as_str(),
                ctx.now
            ])?;
            ctx.stats.conflicts += 1;
        }
    }
    Ok(())
}

/// 新命中的初始狀態：(是否因等待區判定不相關而忽略, 是否判定後才列入 KEV)。
/// 判定不相關的 CVE 中資料仍不足（probable）者直接忽略；判定當時本機還沒有、之後才列入 KEV 者
/// 依「新列入 KEV」規則為未處理（與當時被忽略的既有命中以 `kev_listed` 判斷的結果一致）。
fn initial_dismissal(
    ctx: &Ctx<'_>,
    d: &PairDecision,
    confidence: Confidence,
    scores: &ScoreInputs,
) -> (bool, bool) {
    let Some(&kev_then) = ctx
        .dismissed
        .get(&d.cve_id)
        .filter(|_| confidence == Confidence::Probable)
    else {
        return (false, false);
    };
    let kev_after = scores.in_kev() && !kev_then;
    (!kev_after, kev_after)
}

fn insert(ctx: &mut Ctx<'_>, d: &PairDecision) -> Result<(), CoreError> {
    let Some(confidence) = d.decision.confidence else {
        return Ok(());
    };
    let scores = ctx.data.scores(&d.cve_id);
    let component = ctx.components[&d.component_id];
    let (dismissed, kev_after_dismissal) = initial_dismissal(ctx, d, confidence, &scores);
    let (status, ignore_reason) = if dismissed {
        (Status::Ignored, Some(IgnoreReason::FalsePositive))
    } else {
        (Status::Open, None)
    };
    let id = insert_finding(
        ctx.conn,
        &NewFinding {
            component_id: d.component_id,
            cve_id: &d.cve_id,
            confidence,
            decided_by: d.decision.decided_by.as_str(),
            priority: ctx.priority_of(component, &scores),
            kev_listed: scores.in_kev(),
            manual: false,
            status,
            ignore_reason,
            dismissed,
            now: &ctx.now,
        },
    )?;
    let source = d.decision.decided_by.label();
    let note = match (dismissed, kev_after_dismissal) {
        (true, _) => NOTE_DISMISSED.to_string(),
        (false, true) => format!("新命中（{source}）；{NOTE_KEV_REOPEN}"),
        (false, false) => format!("新命中（{source}）"),
    };
    ctx.event(id, None, status, &note)?;
    ctx.replace_evidence(id, &d.decision)?;
    if dismissed {
        ctx.stats.dismissed += 1;
    } else {
        ctx.stats.new += 1;
    }
    Ok(())
}

/// 「新列入 KEV」只在仍受影響時計算：不受影響期間不記錄 KEV 狀態，
/// 之後回到受影響範圍時才能依規則重新開啟。比對與使用者變更狀態共用。
pub(crate) fn kev_to_record(in_range: bool, kev_now: bool, recorded: bool) -> bool {
    if in_range {
        kev_now
    } else {
        recorded
    }
}

fn kev_seen(f: &ExistingFinding, in_range: bool, kev: bool) -> bool {
    kev_to_record(in_range, kev, f.kev_listed)
}

/// 自動狀態轉換。
struct Transition {
    to: Status,
    note: String,
}

impl Transition {
    fn to(to: Status, note: impl Into<String>) -> Self {
        Transition {
            to,
            note: note.into(),
        }
    }
}

/// 依序檢查：KEV 重新開啟（仍受影響時）→ dismiss 後資料確定受影響則重新開啟 →
/// open 但已由受影響變為不受影響則 resolved。
fn next_status(
    f: &ExistingFinding,
    decision: &Decision,
    kev: bool,
    version: &str,
) -> Option<Transition> {
    let in_range = decision.in_range();
    if f.status == Status::Ignored && in_range && kev && !f.kev_listed {
        return Some(Transition::to(Status::Open, NOTE_KEV_REOPEN));
    }
    let confirmed = decision.confidence == Some(Confidence::Confirmed);
    // 等待區判定不相關時資料不足；資料之後確定受影響 → 重新開啟（與新命中的處理一致）
    if f.status == Status::Ignored && f.dismissed && confirmed {
        return Some(Transition::to(Status::Open, NOTE_CONFIRMED_AFTER_DISMISS));
    }
    // 只在「由受影響變為不受影響」時自動處理：使用者手動重新開啟已不受影響的命中後不再被轉回；
    // 使用者人工判定套用（manual）的命中由使用者自行處理
    if f.status == Status::Open && f.in_range && !in_range && !f.manual {
        return Some(Transition::to(
            Status::Resolved,
            format!("{NOTE_OUT_OF_RANGE}（目前 {version}）"),
        ));
    }
    None
}

/// 重算後寫回命中的欄位。
struct UpdateRow<'a> {
    confidence: Option<&'a str>,
    decided_by: Option<&'a str>,
    priority: &'a str,
    status: Status,
    in_range: bool,
    kev_listed: bool,
}

fn write_update(ctx: &Ctx<'_>, id: i64, row: &UpdateRow<'_>) -> Result<(), CoreError> {
    ctx.conn.execute(
        "UPDATE finding SET confidence = COALESCE(?2, confidence), decided_by = COALESCE(?3, decided_by),
                priority = ?4, status = ?5,
                ignore_reason = CASE WHEN ?5 = 'ignored' THEN ignore_reason END,
                justification = CASE WHEN ?5 = 'ignored' THEN justification END,
                last_evaluated = ?6, in_range = ?7, kev_listed = ?8,
                dismissed = (?5 = 'ignored' AND dismissed)
         WHERE id = ?1",
        params![
            id,
            row.confidence,
            row.decided_by,
            row.priority,
            row.status.as_str(),
            ctx.now,
            row.in_range,
            row.kev_listed
        ],
    )?;
    Ok(())
}

fn update(ctx: &mut Ctx<'_>, f: &ExistingFinding, d: &PairDecision) -> Result<(), CoreError> {
    let component = ctx.components[&d.component_id];
    let scores = ctx.data.scores(&d.cve_id);
    let kev = scores.in_kev();
    let in_range = d.decision.in_range();
    let transition = next_status(f, &d.decision, kev, &component.version_raw);
    let status = transition.as_ref().map_or(f.status, |t| t.to);
    // 人工判定以 `manual` 欄位保存，decided_by 一律反映最新的來源判定
    let (confidence, decided_by) = match d.decision.confidence {
        Some(c) => (Some(c.as_str()), Some(d.decision.decided_by.as_str())),
        None => (None, None),
    };
    let still = status == Status::Resolved && in_range;
    write_update(
        ctx,
        f.id,
        &UpdateRow {
            confidence,
            decided_by,
            priority: ctx.priority_of(component, &scores).as_str(),
            status,
            in_range,
            kev_listed: kev_seen(f, in_range, kev),
        },
    )?;
    ctx.replace_evidence(f.id, &d.decision)?;
    if let Some(t) = transition {
        ctx.event(f.id, Some(f.status), t.to, &t.note)?;
        match t.to {
            Status::Open => ctx.stats.reopened += 1,
            Status::Resolved => ctx.stats.resolved += 1,
            Status::Ignored => {}
        }
    }
    if still {
        ctx.stats.still_vulnerable += 1;
    }
    Ok(())
}

/// 沒有判定的命中應有的狀態：(新狀態, 新的 in_range（None 為不變）, 事件備註)。
fn undecided_outcome(
    f: &ExistingFinding,
    rejected: bool,
    kev: bool,
) -> (Status, Option<bool>, Option<&'static str>) {
    if rejected {
        // 撤銷的 CVE 不再受影響（已處理者不會再顯示「仍受影響」）
        let to = if f.status == Status::Open {
            Status::Ignored
        } else {
            f.status
        };
        return (to, Some(false), (to != f.status).then_some(NOTE_REJECTED));
    }
    if f.manual {
        let reopen = f.in_range && f.status == Status::Ignored && kev && !f.kev_listed;
        return if reopen {
            (Status::Open, None, Some(NOTE_KEV_REOPEN))
        } else {
            (f.status, None, None)
        };
    }
    if f.status == Status::Open && f.in_range {
        return (Status::Resolved, Some(false), Some(NOTE_NO_LONGER_LISTED));
    }
    (f.status, Some(false), None)
}

/// 本次沒有判定的既有命中：CVE 撤銷、人工命中或來源已不再提到此產品。
fn undecided(
    ctx: &mut Ctx<'_>,
    key: &Key,
    f: &ExistingFinding,
    rejected: bool,
) -> Result<(), CoreError> {
    let component = ctx.components[&key.0];
    let scores = ctx.data.scores(&key.1);
    let kev = scores.in_kev();
    let (status, in_range, note) = undecided_outcome(f, rejected, kev);
    let reason = (status == Status::Ignored && rejected && note.is_some())
        .then_some(IgnoreReason::FalsePositive.as_str());
    ctx.conn.execute(
        "UPDATE finding SET priority = ?2, status = ?3,
                ignore_reason = CASE WHEN ?3 = 'ignored' THEN COALESCE(?4, ignore_reason) END,
                justification = CASE WHEN ?3 = 'ignored' AND ?4 IS NULL THEN justification END,
                last_evaluated = ?5, in_range = COALESCE(?6, in_range), kev_listed = ?7
         WHERE id = ?1",
        params![
            f.id,
            ctx.priority_of(component, &scores).as_str(),
            status.as_str(),
            reason,
            ctx.now,
            in_range,
            kev_seen(f, in_range.unwrap_or(f.in_range), kev)
        ],
    )?;
    if let Some(note) = note {
        ctx.event(f.id, Some(f.status), status, note)?;
        match status {
            Status::Open => ctx.stats.reopened += 1,
            Status::Ignored => ctx.stats.rejected += 1,
            Status::Resolved => ctx.stats.resolved += 1,
        }
    }
    Ok(())
}
