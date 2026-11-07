//! 等待區：可能與資產有關、但資料不足以判定的 CVE（CVE 層級）。
//!
//! 進入條件：
//! - 產品相符，但所有來源皆無法判定，且原廠標示調查中 → `vendor_investigating`
//! - 產品相符，但所有來源都沒有版本資訊 → `no_version_data`
//! - 僅描述關鍵字相符（`relevance_reason = keyword_only`）且關鍵字屬於使用中的產品 → `keyword_only`
//!
//! 每次比對後重新評估；資料足以判定時移出並記錄原因。使用者人工移出的 CVE 不再自動放回。

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use super::engine::Decision;
use super::evaluate::PairDecision;
use super::evidence::Verdict;
use super::load::{CveInfo, MatchData, Target};
use crate::catalog::Catalog;
use crate::error::CoreError;
use crate::ingest::cve5::CveState;
use crate::ingest::relevance::{mentions_any_lower, normalize_keywords, Relevance};

string_enum!(
    PendingReason {
        KeywordOnly => "keyword_only",
        NoVersionData => "no_version_data",
        VendorInvestigating => "vendor_investigating",
    }
);

impl PendingReason {
    pub fn label(self) -> &'static str {
        match self {
            PendingReason::KeywordOnly => "僅關鍵字相符",
            PendingReason::NoVersionData => "缺少版本資料",
            PendingReason::VendorInvestigating => "原廠調查中",
        }
    }
}

string_enum!(
    ResolvedReason {
        DataAffected => "data_affected",
        DataNotAffected => "data_not_affected",
        DataInconclusive => "data_inconclusive",
        NoLongerRelevant => "no_longer_relevant",
        ManualApply => "manual_apply",
        ManualDismiss => "manual_dismiss",
    }
);

impl ResolvedReason {
    pub fn label(self) -> &'static str {
        match self {
            ResolvedReason::DataAffected => "資料補齊，受影響",
            ResolvedReason::DataNotAffected => "資料補齊，不受影響",
            ResolvedReason::DataInconclusive => "已有版本資料但無法判定（保留 probable 命中）",
            ResolvedReason::NoLongerRelevant => "已與資產無關",
            ResolvedReason::ManualApply => "人工判定套用",
            ResolvedReason::ManualDismiss => "人工判定不相關",
        }
    }

    fn is_manual(self) -> bool {
        matches!(
            self,
            ResolvedReason::ManualApply | ResolvedReason::ManualDismiss
        )
    }
}

/// `[pending]` 設定的預設逾期天數。
pub const DEFAULT_STALE_DAYS: u32 = 60;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingStats {
    pub entered: usize,
    pub resolved: usize,
    pub active: usize,
    pub stale: usize,
}

/// 使用中產品的正規化關鍵字（每次比對建立一次）。
pub struct KeywordIndex {
    products: Vec<(String, Vec<String>)>,
}

impl KeywordIndex {
    /// 只收 `keys` 內（使用中）的產品。
    pub fn new(catalog: &Catalog, keys: &BTreeSet<&str>) -> Self {
        let products = catalog
            .products()
            .iter()
            .filter(|p| keys.contains(p.key.as_str()))
            .map(|p| (p.key.clone(), normalize_keywords(&p.keywords)))
            .filter(|(_, k)| !k.is_empty())
            .collect();
        KeywordIndex { products }
    }

    /// CVE 描述中出現關鍵字的產品 key。
    pub fn products_in(&self, description: &str) -> Vec<String> {
        let text = description.to_lowercase();
        self.products
            .iter()
            .filter(|(_, keywords)| mentions_any_lower(&text, keywords))
            .map(|(key, _)| key.clone())
            .collect()
    }
}

/// 使用者人工判定不相關（manual_dismiss）的 CVE → 判定當時是否已在本機 KEV。
pub fn dismissed(conn: &Connection) -> Result<BTreeMap<String, bool>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT cve_id, kev_at_dismissal FROM pending
         WHERE resolved_reason = 'manual_dismiss' AND resolved_at IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// 單一產品的資料是否仍不足：全部未知，且原廠調查中或沒有任何版本資料。
fn product_pending(decisions: &[&Decision]) -> Option<PendingReason> {
    if decisions.iter().any(|d| d.verdict != Verdict::Unknown) {
        return None;
    }
    if decisions.iter().any(|d| d.investigating()) {
        Some(PendingReason::VendorInvestigating)
    } else if !decisions.iter().any(|d| d.has_version_data()) {
        Some(PendingReason::NoVersionData)
    } else {
        None
    }
}

/// CVE 層級的等待區判斷（純函式）：`Ok` 為應在等待區的原因，`Err` 為應移出（或不進入）的原因。
///
/// `by_product` 為各使用中產品的判定；任一產品仍資料不足，CVE 就留在等待區
/// （其他產品已有明確結論也一樣），原廠調查中優先於缺少版本資料。
pub fn assess(
    by_product: &[Vec<&Decision>],
    keyword_related: bool,
) -> Result<PendingReason, ResolvedReason> {
    if by_product.is_empty() {
        return if keyword_related {
            Ok(PendingReason::KeywordOnly)
        } else {
            Err(ResolvedReason::NoLongerRelevant)
        };
    }
    let reasons: Vec<PendingReason> = by_product
        .iter()
        .filter_map(|ds| product_pending(ds))
        .collect();
    if reasons.contains(&PendingReason::VendorInvestigating) {
        return Ok(PendingReason::VendorInvestigating);
    }
    if let Some(reason) = reasons.first() {
        return Ok(*reason);
    }
    let all = || by_product.iter().flatten();
    Err(if all().any(|d| d.verdict == Verdict::Affected) {
        ResolvedReason::DataAffected
    } else if all().any(|d| d.verdict != Verdict::Unknown) {
        ResolvedReason::DataNotAffected
    } else {
        // 例如元件版本無法解析、或不在任何區間且沒有 defaultStatus：已建立 probable 命中
        ResolvedReason::DataInconclusive
    })
}

struct Row {
    resolved: Option<ResolvedReason>,
    active: bool,
    /// 上次評估時相關的產品 key
    products: BTreeSet<String>,
}

fn load_rows(conn: &Connection) -> Result<HashMap<String, Row>, CoreError> {
    let mut stmt =
        conn.prepare("SELECT cve_id, resolved_at, resolved_reason, products_json FROM pending")?;
    let mut rows = stmt.query([])?;
    let mut out = HashMap::new();
    while let Some(r) = rows.next()? {
        let cve_id: String = r.get(0)?;
        let resolved_at: Option<String> = r.get(1)?;
        let reason: Option<String> = r.get(2)?;
        let products_json: String = r.get(3)?;
        let products =
            super::load::json_list::<String>(&products_json, "pending.products_json", &cve_id)
                .into_iter()
                .collect();
        let row = Row {
            resolved: reason.as_deref().and_then(ResolvedReason::parse),
            active: resolved_at.is_none(),
            products,
        };
        out.insert(cve_id, row);
    }
    Ok(out)
}

/// 僅關鍵字相符的 CVE，描述中出現關鍵字的使用中產品。
fn keyword_products(cve: &CveInfo, keywords: &KeywordIndex) -> Vec<String> {
    if cve.relevance != Some(Relevance::KeywordOnly) {
        return Vec::new();
    }
    cve.description
        .as_deref()
        .map(|d| keywords.products_in(d))
        .unwrap_or_default()
}

fn desired(
    cve: &CveInfo,
    decisions: &[Vec<&Decision>],
    keyword_related: bool,
) -> Result<PendingReason, ResolvedReason> {
    if cve.state == CveState::Rejected {
        return Err(ResolvedReason::NoLongerRelevant);
    }
    assess(decisions, keyword_related)
}

/// 逾期門檻：進入時間早於此值者為逾期（查詢時計算，`stale_days` 變更立即生效）。
pub fn stale_cutoff(now: DateTime<Utc>, stale_days: u32) -> String {
    (now - Duration::days(i64::from(stale_days))).to_rfc3339()
}

/// 各 CVE 依產品分組的判定（只看產品元件；套件由 OSV 判定，不進等待區）。
type ByProduct<'a> = BTreeMap<&'a str, Vec<&'a Decision>>;

fn group_by_cve<'a>(
    data: &'a MatchData,
    decisions: &'a [PairDecision],
) -> HashMap<&'a str, ByProduct<'a>> {
    let product_of: HashMap<i64, &str> = data
        .components
        .iter()
        .filter_map(|c| match &c.target {
            Target::Product(key) => Some((c.id, key.as_str())),
            Target::Package(_) => None,
        })
        .collect();
    let mut grouped: HashMap<&str, ByProduct<'_>> = HashMap::new();
    for d in decisions {
        if let Some(key) = product_of.get(&d.component_id) {
            grouped
                .entry(d.cve_id.as_str())
                .or_default()
                .entry(key)
                .or_default()
                .push(&d.decision);
        }
    }
    grouped
}

/// 重新評估時的共用參數。
struct Pass<'a> {
    conn: &'a Connection,
    rows: HashMap<String, Row>,
    keywords: KeywordIndex,
    /// 本次因不在目錄中而略過的產品 key
    skipped: &'a BTreeSet<String>,
    /// affected 資料無法讀取、本次不比對的 CVE（維持原狀）
    corrupt: &'a BTreeSet<String>,
    now: String,
    stats: PendingStats,
}

impl Pass<'_> {
    fn apply(&mut self, cve: &CveInfo, by_product: &ByProduct<'_>) -> Result<(), CoreError> {
        if self.corrupt.contains(&cve.id) {
            return Ok(());
        }
        let row = self.rows.get(&cve.id);
        let keyword = keyword_products(cve, &self.keywords);
        let decisions: Vec<Vec<&Decision>> = by_product.values().cloned().collect();
        match desired(cve, &decisions, !keyword.is_empty()) {
            Ok(reason) => {
                // 暫時略過的產品仍算相關，否則下次比對會失去延後的依據
                let kept = row.map(|r| &r.products).into_iter().flatten();
                let products: BTreeSet<&str> = by_product
                    .keys()
                    .copied()
                    .chain(keyword.iter().map(String::as_str))
                    .chain(
                        kept.filter(|p| self.skipped.contains(*p))
                            .map(String::as_str),
                    )
                    .collect();
                let entry = Entry {
                    cve_id: &cve.id,
                    reason,
                    products: &products,
                };
                let entered = enter(self.conn, row, &entry, &self.now)?;
                self.stats.entered += usize::from(entered);
            }
            // 相關產品本次因不在目錄中而略過：缺少該產品的判定，無法斷定可以移出，維持原狀
            Err(_) if self.deferred(cve, row) => {}
            Err(why) if row.is_some_and(|r| r.active) => {
                resolve_row(self.conn, &cve.id, why, &self.now)?;
                self.stats.resolved += 1;
            }
            Err(_) => {}
        }
        Ok(())
    }

    fn deferred(&self, cve: &CveInfo, row: Option<&Row>) -> bool {
        cve.state != CveState::Rejected
            && row.is_some_and(|r| r.active && !r.products.is_disjoint(self.skipped))
    }

    /// 本機已沒有的 CVE 仍在等待區 → 移出。
    fn resolve_orphans(&mut self, seen: &BTreeSet<&str>) -> Result<(), CoreError> {
        let orphaned: Vec<String> = self
            .rows
            .iter()
            .filter(|(id, r)| r.active && !seen.contains(id.as_str()))
            .map(|(id, _)| id.clone())
            .collect();
        for cve_id in orphaned {
            resolve_row(
                self.conn,
                &cve_id,
                ResolvedReason::NoLongerRelevant,
                &self.now,
            )?;
            self.stats.resolved += 1;
        }
        Ok(())
    }
}

/// 比對後重新評估等待區。
pub fn reevaluate(
    conn: &Connection,
    data: &MatchData,
    decisions: &[PairDecision],
    catalog: &Catalog,
    stale_days: u32,
    skipped_products: &BTreeSet<String>,
    now: DateTime<Utc>,
) -> Result<PendingStats, CoreError> {
    let keys: BTreeSet<&str> = data
        .components
        .iter()
        .filter_map(|c| c.product_key())
        .collect();
    let by_cve = group_by_cve(data, decisions);
    let mut pass = Pass {
        conn,
        rows: load_rows(conn)?,
        keywords: KeywordIndex::new(catalog, &keys),
        skipped: skipped_products,
        corrupt: &data.corrupt_cves,
        now: now.to_rfc3339(),
        stats: PendingStats::default(),
    };
    let mut seen = BTreeSet::new();
    let none = ByProduct::new();
    for cve in &data.cves {
        seen.insert(cve.id.as_str());
        pass.apply(cve, by_cve.get(cve.id.as_str()).unwrap_or(&none))?;
    }
    pass.resolve_orphans(&seen)?;
    let (active, stale): (i64, i64) = conn.query_row(
        "SELECT count(*), coalesce(sum(entered_at < ?1), 0) FROM pending WHERE resolved_at IS NULL",
        params![stale_cutoff(now, stale_days)],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut stats = pass.stats;
    stats.active = usize::try_from(active).unwrap_or_default();
    stats.stale = usize::try_from(stale).unwrap_or_default();
    Ok(stats)
}

/// 應在等待區的 CVE。
struct Entry<'a> {
    cve_id: &'a str,
    reason: PendingReason,
    products: &'a BTreeSet<&'a str>,
}

/// 進入（或留在）等待區；回傳是否為新進入。人工移出者不再放回。
fn enter(
    conn: &Connection,
    row: Option<&Row>,
    entry: &Entry<'_>,
    now: &str,
) -> Result<bool, CoreError> {
    let products = serde_json::to_string(entry.products)
        .map_err(|e| CoreError::Invalid(format!("相關產品序列化失敗：{e}")))?;
    let (cve_id, reason) = (entry.cve_id, entry.reason.as_str());
    match row {
        Some(r) if r.active => {
            conn.execute(
                "UPDATE pending SET reason = ?2, last_checked = ?3, products_json = ?4
                 WHERE cve_id = ?1",
                params![cve_id, reason, now, products],
            )?;
            Ok(false)
        }
        Some(r) if r.resolved.is_some_and(ResolvedReason::is_manual) => Ok(false),
        _ => {
            conn.execute(
                "INSERT INTO pending (cve_id, reason, entered_at, last_checked, products_json)
                 VALUES (?1, ?2, ?3, ?3, ?4)
                 ON CONFLICT (cve_id) DO UPDATE SET reason = excluded.reason,
                     entered_at = excluded.entered_at, last_checked = excluded.last_checked,
                     products_json = excluded.products_json,
                     resolved_at = NULL, resolved_reason = NULL",
                params![cve_id, reason, now, products],
            )?;
            Ok(true)
        }
    }
}

pub fn resolve_row(
    conn: &Connection,
    cve_id: &str,
    why: ResolvedReason,
    now: &str,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE pending SET resolved_at = ?2, resolved_reason = ?3, last_checked = ?2
         WHERE cve_id = ?1",
        params![cve_id, now, why.as_str()],
    )?;
    Ok(())
}

/// 等待區中（尚未移出）的 CVE 原因；不在等待區為 None。
pub fn active_reason(conn: &Connection, cve_id: &str) -> Result<Option<PendingReason>, CoreError> {
    let reason: Option<String> = conn
        .query_row(
            "SELECT reason FROM pending WHERE cve_id = ?1 AND resolved_at IS NULL",
            params![cve_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(reason.as_deref().and_then(PendingReason::parse))
}
