//! 原廠 CSAF 判定（`csaf_status` / `csaf_fix`）。
//!
//! - 同一 CVE 有多份公告時取 `updated` 最新者
//! - `exact` 等值比較優先於 `range`；`range` 以產品 scheme 評估 `vers:` 區間
//! - 多個區間同時命中時取下界最大者（最具體）：Palo Alto 以只有下界的區間表示各版本線的修正點
//!   （`>=11.1.0` affected、`>=11.1.4-h37` fixed），等同 CVE 5.x 的 changes 語意；
//!   同樣具體但結論相反時判定為未知
//! - 產品層級（沒有版本，`version_expr = *`）的狀態列為最後順位：精確版本與區間都沒涵蓋時才採用
//! - affected → 受影響；not_affected / fixed → 不受影響；under_investigation → 未知（原廠調查中）

use std::collections::BTreeSet;

use super::range::{more_specific, vers_contains, VersHit};
use super::{ComponentVersion, Evidence, Source, Verdict};
use crate::ingest::csaf::status::{AFFECTED, FIXED, NOT_AFFECTED, UNDER_INVESTIGATION};
use crate::ingest::csaf::tree::ANY_VERSION;
use crate::ingest::csaf::vers;
use crate::matching::version::{Version, VersionScheme};

/// `csaf_status` 的一列（含所屬公告的更新時間）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsafRow {
    pub provider: String,
    pub tracking_id: String,
    pub doc_updated: Option<String>,
    pub version_expr: String,
    /// `exact` 或 `range`
    pub kind: String,
    /// `range` 列的 `vers:` 區間，載入時解析一次（無法解析為 None）
    pub range: Option<vers::VersRange>,
    pub status: String,
    pub justification: Option<String>,
}

/// `csaf_fix` 的一列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsafFix {
    pub provider: String,
    pub tracking_id: String,
    pub fixed_version: Option<String>,
}

fn status_verdict(status: &str) -> Verdict {
    match status {
        AFFECTED => Verdict::Affected,
        NOT_AFFECTED | FIXED => Verdict::NotAffected,
        _ => Verdict::Unknown,
    }
}

/// 公告更新時間；以 UTC 時間比較（時區與精度不同的寫法也能正確排序），無法解析者排在最前。
fn updated_at(row: &CsafRow) -> Option<chrono::DateTime<chrono::Utc>> {
    row.doc_updated
        .as_deref()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&chrono::Utc))
}

/// 最新公告的 (provider, tracking_id)。
fn latest_doc(rows: &[CsafRow]) -> Option<(&str, &str)> {
    rows.iter()
        .max_by(|a, b| {
            updated_at(a)
                .cmp(&updated_at(b))
                .then_with(|| a.doc_updated.cmp(&b.doc_updated))
                .then_with(|| b.tracking_id.cmp(&a.tracking_id))
        })
        .map(|r| (r.provider.as_str(), r.tracking_id.as_str()))
}

struct Match<'a> {
    row: &'a CsafRow,
    hit: VersHit,
}

/// 命中的列與是否有列無法判斷。
fn matching_rows<'a>(
    scheme: &dyn VersionScheme,
    v: &Version,
    rows: &[&'a CsafRow],
) -> (Vec<&'a CsafRow>, Vec<Match<'a>>, bool) {
    let mut exact = Vec::new();
    let mut ranges = Vec::new();
    let mut unparseable = false;
    for row in rows
        .iter()
        .copied()
        .filter(|r| r.version_expr != ANY_VERSION)
    {
        if row.kind == "exact" {
            match scheme
                .parse(&row.version_expr)
                .and_then(|t| scheme.covers(&t, v))
            {
                None => unparseable = true,
                Some(true) => exact.push(row),
                Some(false) => {}
            }
            continue;
        }
        match row.range.as_ref().and_then(|r| vers_contains(scheme, v, r)) {
            None => unparseable = true,
            Some(Some(hit)) => ranges.push(Match { row, hit }),
            Some(None) => {}
        }
    }
    (exact, ranges, unparseable)
}

/// 多列結論：一致時取該結論，否則未知（under_investigation 視為未知並標示調查中）。
fn combine(rows: &[&CsafRow]) -> (Verdict, bool) {
    let verdicts: BTreeSet<&str> = rows
        .iter()
        .map(|r| status_verdict(&r.status).as_str())
        .collect();
    let investigating = rows.iter().any(|r| r.status == UNDER_INVESTIGATION);
    match (verdicts.len(), rows.first()) {
        (1, Some(r)) => (status_verdict(&r.status), investigating),
        _ => (Verdict::Unknown, investigating),
    }
}

fn most_specific<'a>(scheme: &dyn VersionScheme, matches: &[Match<'a>]) -> Vec<&'a CsafRow> {
    let best = matches.iter().fold(None, |acc, m| {
        Some(more_specific(scheme, acc, m.hit.clone()))
    });
    let Some(best) = best else { return Vec::new() };
    matches
        .iter()
        .filter(|m| match (&m.hit.lower, &best.lower) {
            (None, None) => true,
            (Some(a), Some(b)) => scheme.cmp(a, b) == Some(std::cmp::Ordering::Equal),
            _ => false,
        })
        .map(|m| m.row)
        .collect()
}

fn describe(rows: &[&CsafRow], verdict: Verdict) -> String {
    let exprs: BTreeSet<String> = rows
        .iter()
        .map(|r| {
            let expr = if r.version_expr == ANY_VERSION {
                "所有版本（產品層級）"
            } else {
                r.version_expr.as_str()
            };
            format!("{expr}（{}）", r.status)
        })
        .collect();
    format!(
        "符合 {}：{}",
        exprs.into_iter().collect::<Vec<_>>().join("、"),
        verdict.label()
    )
}

/// 只在原廠公告有此產品的狀態列時產生證據。
pub fn evaluate(
    rows: &[CsafRow],
    fixes: &[CsafFix],
    version: &ComponentVersion<'_>,
) -> Option<Evidence> {
    let (provider, tracking_id) = latest_doc(rows)?;
    let doc_rows: Vec<&CsafRow> = rows
        .iter()
        .filter(|r| r.provider == provider && r.tracking_id == tracking_id)
        .collect();
    let updated = doc_rows
        .first()
        .and_then(|r| r.doc_updated.clone())
        .unwrap_or_default();
    let header = format!(
        "{provider} 公告 {tracking_id}（更新 {}）",
        updated.get(..10).unwrap_or(&updated)
    );
    // 產品層級的明確結論（affected / not_affected / fixed）也算有判定資料；調查中則否
    let has_version_data = doc_rows
        .iter()
        .any(|r| r.version_expr != ANY_VERSION || r.status != UNDER_INVESTIGATION);
    let any_investigating = doc_rows
        .iter()
        .any(|r| r.version_expr == ANY_VERSION && r.status == UNDER_INVESTIGATION);
    let mut evidence = match &version.parsed {
        None => version.unparseable(Source::Vendor),
        Some(v) => verdict_for(version.scheme, v, &doc_rows),
    };
    evidence.summary = format!("{header}：{}", evidence.summary);
    evidence.has_version_data = has_version_data;
    evidence.investigating |= any_investigating;
    evidence.fixed_versions = fixes
        .iter()
        .filter(|f| f.provider == provider && f.tracking_id == tracking_id)
        .filter_map(|f| f.fixed_version.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Some(evidence)
}

fn verdict_for(scheme: &dyn VersionScheme, v: &Version, rows: &[&CsafRow]) -> Evidence {
    let (exact, ranges, unparseable) = matching_rows(scheme, v, rows);
    // 精確版本最具體；否則有無法解析的版本列時可能有更具體的列 → 不下定論
    let chosen = if !exact.is_empty() {
        exact
    } else if unparseable {
        Vec::new()
    } else {
        let in_range = most_specific(scheme, &ranges);
        if in_range.is_empty() {
            // 版本列都沒涵蓋 → 產品層級狀態
            rows.iter()
                .copied()
                .filter(|r| r.version_expr == ANY_VERSION)
                .collect()
        } else {
            in_range
        }
    };
    if chosen.is_empty() {
        let summary = if unparseable {
            "公告版本無法以產品格式解析"
        } else {
            "公告未涵蓋此版本"
        };
        return Evidence::new(Source::Vendor, Verdict::Unknown, summary);
    }
    let (verdict, investigating) = combine(&chosen);
    let summary = if verdict == Verdict::Unknown && !investigating {
        format!("{}（結論互相矛盾）", describe(&chosen, verdict))
    } else if investigating {
        format!("{}（原廠調查中）", describe(&chosen, verdict))
    } else {
        describe(&chosen, verdict)
    };
    let mut evidence = Evidence::new(Source::Vendor, verdict, summary);
    evidence.investigating = investigating;
    evidence
}
