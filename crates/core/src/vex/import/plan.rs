//! 匯入計畫：把 VEX 項目分為將套用、已相同、未對應、本機較新、格式不完整。
//!
//! 1. analysis 不完整（缺少、未知 state、not_affected 缺理由）或缺少 affects → 格式不完整
//! 2. 沒有 CVE ID、找不到元件、或元件上沒有該 CVE 的命中 → 未對應（不新建命中）
//! 3. 同一命中在檔案中有不同判斷 → 格式不完整（不猜測哪一個才對）
//! 4. 判定（狀態、原因、理由）與本機相同且 detail 沒有新內容 → 已相同（不寫入事件）
//! 5. 檔案為 `in_triage`（尚未判定）而本機狀態不是未處理 → 本機較新（未判定不覆蓋本機的判斷或
//!    系統的自動處理，例如同事剛同步、還沒處理過的命中）；`--force` 時仍套用
//! 6. 本機最後一筆人為判定晚於 VEX `lastUpdated`（晚於匯入當下超過容許偏差的時間視為未知）（缺少時以 `metadata.timestamp`；兩者皆無
//!    而本機有人為判定時視為本機較新）→ 本機較新；`--force` 時仍套用。匯入的判定以來源檔中的
//!    時間計算（見 [`detail::last_decision_at`]）
//! 7. 其餘 → 將套用

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use super::doc::{self, Bom};
use super::resolve::{Method, Resolver};
use crate::error::CoreError;
use crate::matching::triage::Status;
use crate::vex::detail::{self, EventFacts};
use crate::vex::export::{load_events, load_verdict};
use crate::vex::mapping::{from_analysis, AnalysisState, Verdict};

/// 報告中的一個項目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// CVE ID（沒有時為 VEX 的 id）
    pub vuln: String,
    /// 本機 `<資產>:<元件>`，未對應時為 VEX 的元件參照
    pub target: String,
    pub method: Option<Method>,
    /// 判定或原因說明
    pub note: String,
}

/// 將套用的變更。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub finding_id: i64,
    pub verdict: Verdict,
    /// 套用前的本機判定
    pub local: Verdict,
    pub detail: Option<String>,
    /// 來源檔中的判定時間（記錄於事件，之後的衝突判斷與再匯出以此為準）
    pub updated: Option<DateTime<Utc>>,
    pub item: Item,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub apply: Vec<Planned>,
    pub unchanged: Vec<Item>,
    pub unmatched: Vec<Item>,
    pub local_newer: Vec<Item>,
    pub incomplete: Vec<Item>,
}

/// 一個（元件 × CVE）的聲明。
#[derive(Clone)]
struct Statement {
    verdict: Verdict,
    /// 檔案為 in_triage（尚未判定）
    undecided: bool,
    detail: Option<String>,
    updated: Option<DateTime<Utc>>,
}

pub fn verdict_label(v: &Verdict) -> String {
    let mut s = v.status.label().to_string();
    match (v.reason, v.justification) {
        (Some(r), Some(j)) => s.push_str(&format!("（{}，{}）", r.label(), j.as_str())),
        (Some(r), None) => s.push_str(&format!("（{}）", r.label())),
        _ => {}
    }
    s
}

fn item(vuln: &str, target: &str, method: Option<Method>, note: impl Into<String>) -> Item {
    Item {
        vuln: vuln.to_string(),
        target: target.to_string(),
        method,
        note: note.into(),
    }
}

/// 項目的元件參照（報告用）。
fn affect_refs(v: &doc::Vulnerability) -> Vec<String> {
    v.affects
        .iter()
        .map(|a| a.reference.clone().unwrap_or_default())
        .collect()
}

/// 一個 VEX 項目解析後的內容（尚未對應元件）。
struct Parsed {
    cve: String,
    refs: Vec<String>,
    statement: Statement,
}

/// 無法套用的項目。
enum Rejected {
    Incomplete(Item),
    Unmatched(Item),
}

/// 命中 id → 檔案中對它的聲明與報告項目。
type ByFinding = BTreeMap<i64, Vec<(Statement, Item)>>;

/// 容許來源時間比匯入當下晚的時鐘偏差；超過者視為時間未知（不讓錯誤的未來時間永遠勝過本機判斷）。
const FUTURE_SKEW: chrono::TimeDelta = chrono::TimeDelta::minutes(5);

/// 來源時間；晚於 `now` 超過容許偏差時為 `None`（未知）。
fn credible(t: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    t.filter(|t| *t <= now + FUTURE_SKEW)
}

pub fn plan(
    conn: &Connection,
    bom: &Bom,
    force: bool,
    now: DateTime<Utc>,
) -> Result<Plan, CoreError> {
    let resolver = Resolver::load(conn)?;
    let components = bom.components_by_ref();
    let fallback_time = credible(bom.timestamp().and_then(detail::parse_time), now);
    let mut out = Plan::default();
    let mut by_finding = ByFinding::new();
    for v in &bom.vulnerabilities {
        match parse_vulnerability(v, fallback_time, now) {
            Ok(parsed) => {
                let ctx = (&resolver, &components);
                collect_targets(conn, ctx, &parsed, &mut out, &mut by_finding)?
            }
            Err(Rejected::Incomplete(it)) => out.incomplete.push(it),
            Err(Rejected::Unmatched(it)) => out.unmatched.push(it),
        }
    }
    for (finding_id, statements) in by_finding {
        classify(conn, finding_id, statements, force, &mut out)?;
    }
    Ok(out)
}

/// 檢查 analysis 與 affects、取得 CVE ID、時間與 detail。
fn parse_vulnerability(
    v: &doc::Vulnerability,
    fallback_time: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<Parsed, Rejected> {
    let shown = v.id.clone().unwrap_or_default();
    let refs = affect_refs(v);
    let Some(a) = v.analysis.as_ref() else {
        let it = item(&shown, &refs.join("、"), None, "缺少 analysis");
        return Err(Rejected::Incomplete(it));
    };
    let verdict = from_analysis(a.state.as_deref(), a.justification.as_deref(), &a.response)
        .map_err(|e| Rejected::Incomplete(item(&shown, &refs.join("、"), None, e.0)))?;
    if refs.is_empty() {
        return Err(Rejected::Incomplete(item(&shown, "", None, "缺少 affects")));
    }
    let cve = v
        .cve()
        .ok_or_else(|| Rejected::Unmatched(item(&shown, &refs.join("、"), None, "沒有 CVE ID")))?;
    let statement = Statement {
        verdict,
        undecided: a.state.as_deref().and_then(AnalysisState::parse)
            == Some(AnalysisState::InTriage),
        detail: a
            .detail
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(str::to_string),
        updated: a
            .last_updated
            .as_deref()
            .and_then(detail::parse_time)
            .map_or(fallback_time, |t| credible(Some(t), now)),
    };
    Ok(Parsed {
        cve,
        refs,
        statement,
    })
}

/// 對應每個 affects 參照的本機元件與命中；找不到者列為未對應。
fn collect_targets(
    conn: &Connection,
    (resolver, components): (&Resolver, &HashMap<String, doc::Component>),
    p: &Parsed,
    out: &mut Plan,
    by_finding: &mut ByFinding,
) -> Result<(), CoreError> {
    for reference in &p.refs {
        let resolution = resolver.resolve(&super::descriptor(reference, components));
        let method = resolution.method;
        if resolution.matches.is_empty() {
            let why = match method {
                Some(Method::BomRef) => "找不到 bom-ref 指向的本機元件",
                _ => "找不到對應的本機元件",
            };
            out.unmatched.push(item(&p.cve, reference, method, why));
            continue;
        }
        for local in resolution.matches {
            let target = format!("{}:{}", local.asset_tag, local.label);
            let Some(finding_id) = finding_for(conn, local.id, &p.cve)? else {
                out.unmatched
                    .push(item(&p.cve, &target, method, "本機沒有此命中"));
                continue;
            };
            let it = item(&p.cve, &target, method, verdict_label(&p.statement.verdict));
            by_finding
                .entry(finding_id)
                .or_default()
                .push((p.statement.clone(), it));
        }
    }
    Ok(())
}

fn finding_for(conn: &Connection, component_id: i64, cve: &str) -> Result<Option<i64>, CoreError> {
    Ok(conn
        .query_row(
            "SELECT id FROM finding WHERE component_id = ?1 AND cve_id = ?2",
            params![component_id, cve],
            |r| r.get(0),
        )
        .optional()?)
}

/// 同一命中的多個聲明合併為一個；判斷或 detail 不同時回傳 `None`。
/// 時間取最新，全部都是 in_triage 時才視為未判定。
fn merge(statements: &[(Statement, Item)]) -> Option<Statement> {
    let (first, _) = statements.first()?;
    let same = statements
        .iter()
        .all(|(s, _)| s.verdict == first.verdict && s.detail == first.detail);
    same.then(|| Statement {
        verdict: first.verdict,
        undecided: statements.iter().all(|(s, _)| s.undecided),
        detail: first.detail.clone(),
        updated: statements.iter().filter_map(|(s, _)| s.updated).max(),
    })
}

/// 分類結果。
enum Outcome {
    Unchanged(String),
    LocalNewer(String),
    Apply,
}

fn decide(local: &Verdict, events: &[EventFacts], s: &Statement, force: bool) -> Outcome {
    let same_detail = s.detail.is_none() || s.detail == detail::current_detail(events);
    if *local == s.verdict && same_detail {
        return Outcome::Unchanged(format!("{}（與本機相同）", verdict_label(local)));
    }
    if force {
        return Outcome::Apply;
    }
    if s.undecided && local.status != Status::Open {
        return Outcome::LocalNewer(format!(
            "檔案尚未判定（in_triage），保留本機的{}",
            verdict_label(local)
        ));
    }
    let newer = match (detail::last_decision_at(events), s.updated) {
        (Some(l), Some(v)) => l > v,
        (Some(_), None) => true,
        (None, _) => false,
    };
    if newer {
        return Outcome::LocalNewer(format!(
            "本機為{}，較檔案中的{}新",
            verdict_label(local),
            verdict_label(&s.verdict)
        ));
    }
    Outcome::Apply
}

fn classify(
    conn: &Connection,
    finding_id: i64,
    statements: Vec<(Statement, Item)>,
    force: bool,
    out: &mut Plan,
) -> Result<(), CoreError> {
    let merged = merge(&statements);
    let Some(mut it) = statements.into_iter().next().map(|(_, it)| it) else {
        return Ok(());
    };
    let Some(statement) = merged else {
        it.note = "檔案中對同一命中有不同的判斷，未套用".into();
        out.incomplete.push(it);
        return Ok(());
    };
    let local = load_verdict(conn, finding_id)?;
    let events = load_events(conn, finding_id)?;
    match decide(&local, &events, &statement, force) {
        Outcome::Unchanged(note) => out.unchanged.push(Item { note, ..it }),
        Outcome::LocalNewer(note) => out.local_newer.push(Item { note, ..it }),
        Outcome::Apply => out.apply.push(Planned {
            finding_id,
            verdict: statement.verdict,
            local,
            detail: statement.detail,
            updated: statement.updated,
            item: it,
        }),
    }
    Ok(())
}
