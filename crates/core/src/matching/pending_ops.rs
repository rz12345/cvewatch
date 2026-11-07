//! 等待區的查詢與人工處理：`cvewatch pending list [--stale]`、
//! `cvewatch pending resolve <cve> --apply <asset_tag>[:<product>] | --dismiss`。

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::engine::Confidence;
use super::evaluate::{evaluate_product, Products};
use super::evidence::Verdict;
use super::findings::{insert_finding, NewFinding, MANUAL};
use super::load::{ActiveComponent, MatchData, Scope};
use super::pending::{
    active_reason, resolve_row, stale_cutoff, KeywordIndex, PendingReason, ResolvedReason,
};
use super::scoring::{priority, ScoringConfig};
use super::triage::{apply_user_change, record_event, Change, IgnoreReason, Status};
use super::version::Schemes;
use crate::catalog::Catalog;
use crate::error::CoreError;
use crate::ingest::show::normalize_id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRow {
    pub cve_id: String,
    pub reason: PendingReason,
    pub entered_at: String,
    pub stale: bool,
    pub kev_date: Option<String>,
    pub title: Option<String>,
}

/// 等待區中的 CVE：列入 KEV 者置頂（加入日期新到舊），其次逾期，再依進入時間。
/// 逾期依當下的 `stale_days` 計算。
pub fn list(
    conn: &Connection,
    only_stale: bool,
    stale_days: u32,
    now: DateTime<Utc>,
) -> Result<Vec<PendingRow>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT p.cve_id, p.reason, p.entered_at, p.entered_at < ?2, k.date_added,
                coalesce(c.title, substr(c.description_en, 1, 80))
         FROM pending p
         LEFT JOIN kev k ON k.cve_id = p.cve_id
         LEFT JOIN cve c ON c.id = p.cve_id
         WHERE p.resolved_at IS NULL AND (?1 = 0 OR p.entered_at < ?2)
         ORDER BY k.date_added IS NULL, k.date_added DESC, p.entered_at < ?2 DESC, p.entered_at,
                  p.cve_id",
    )?;
    let cutoff = stale_cutoff(now, stale_days);
    let rows = stmt.query_map(params![only_stale, cutoff], |r| {
        let reason: String = r.get(1)?;
        Ok((
            r.get(0)?,
            reason,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (cve_id, reason, entered_at, stale, kev_date, title) = row?;
        let Some(reason) = PendingReason::parse(&reason) else {
            return Err(CoreError::Invalid(format!(
                "等待區 {cve_id} 的原因「{reason}」無法辨識"
            )));
        };
        out.push(PendingRow {
            cve_id,
            reason,
            entered_at,
            stale,
            kev_date,
            title,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// 資產編號，可帶 `:<product key>` 指定元件
    Apply(String),
    Dismiss,
}

/// 人工套用時建立（或已存在）的命中。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub finding_id: i64,
    pub product_key: String,
    pub created: bool,
    /// 既有命中原為已處理或忽略，已重新開啟
    pub reopened: bool,
}

/// 人工移出等待區的結果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolution {
    /// 正規化後的 CVE ID
    pub cve_id: String,
    /// `--apply`：建立或重新開啟的命中
    pub applied: Vec<Applied>,
    /// `--dismiss`：一併轉為忽略（false_positive）的 probable 命中 id
    pub ignored: Vec<i64>,
}

/// 人工移出等待區。`--apply` 對指定資產的相關元件建立判定來源為 manual 的 probable 命中；
/// `--dismiss` 將此 CVE 由系統建立、仍未處理的 probable 命中轉為忽略（原因 false_positive）。
pub fn resolve(
    conn: &mut Connection,
    catalog: &Catalog,
    scoring: &ScoringConfig,
    raw_cve: &str,
    action: &Action,
    note: &str,
    now: DateTime<Utc>,
) -> Result<Resolution, CoreError> {
    let request = ResolveRequest {
        catalog,
        scoring,
        raw_cve,
        action,
        note,
        now,
    };
    resolve_with(conn, &request, &|_, _| Ok(()))
}

/// 人工移出等待區的參數。
pub struct ResolveRequest<'a> {
    pub catalog: &'a Catalog,
    pub scoring: &'a ScoringConfig,
    pub raw_cve: &'a str,
    pub action: &'a Action,
    pub note: &'a str,
    pub now: DateTime<Utc>,
}

/// 提交前呼叫（例如排入 Redmine 註記）；失敗時整筆回復。
pub type AfterResolve<'a> = dyn Fn(&Connection, &Resolution) -> Result<(), CoreError> + 'a;

/// 同 [`resolve`]，並在同一交易中呼叫 `after`。
pub fn resolve_with(
    conn: &mut Connection,
    req: &ResolveRequest<'_>,
    after: &AfterResolve<'_>,
) -> Result<Resolution, CoreError> {
    let (catalog, scoring, raw_cve, action, note, now) = (
        req.catalog,
        req.scoring,
        req.raw_cve,
        req.action,
        req.note,
        req.now,
    );
    let cve_id = normalize_id(raw_cve)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let reason = active_reason(&tx, &cve_id)?
        .ok_or_else(|| CoreError::Invalid(format!("{cve_id} 不在等待區")))?;
    let now_str = now.to_rfc3339();
    let resolution = match action {
        Action::Dismiss => {
            let event_note = manual_note("人工判定不相關", reason, note);
            resolve_dismiss(&tx, &cve_id, &event_note, &now_str)?
        }
        Action::Apply(target) => {
            let event_note = manual_note("人工判定套用", reason, note);
            let request = ApplyRequest {
                catalog,
                scoring,
                target,
                note: &event_note,
            };
            resolve_apply(&tx, &cve_id, &request, &now_str)?
        }
    };
    after(&tx, &resolution)?;
    tx.commit()?;
    Ok(resolution)
}

fn resolve_dismiss(
    conn: &Connection,
    cve_id: &str,
    note: &str,
    now: &str,
) -> Result<Resolution, CoreError> {
    let ignored = dismiss_probable(conn, cve_id, note, now)?;
    resolve_row(conn, cve_id, ResolvedReason::ManualDismiss, now)?;
    // 與被忽略的既有命中（kev_listed）相同的依據：判定當時本機是否已有 KEV 紀錄
    conn.execute(
        "UPDATE pending SET kev_at_dismissal = EXISTS (SELECT 1 FROM kev WHERE cve_id = ?1)
         WHERE cve_id = ?1",
        params![cve_id],
    )?;
    Ok(Resolution {
        cve_id: cve_id.to_string(),
        applied: Vec::new(),
        ignored,
    })
}

/// `--apply` 的輸入。
struct ApplyRequest<'a> {
    catalog: &'a Catalog,
    scoring: &'a ScoringConfig,
    target: &'a str,
    note: &'a str,
}

fn resolve_apply(
    conn: &Connection,
    cve_id: &str,
    request: &ApplyRequest<'_>,
    now: &str,
) -> Result<Resolution, CoreError> {
    let target = ApplyTarget::resolve(conn, request.target)?;
    // 只載入此 CVE 與目標資產，不讀整個資料庫
    let scope = Scope {
        cve_id: Some(cve_id),
        asset_tag: Some(target.tag),
    };
    let data = MatchData::load_scoped(conn, scope)?;
    let components = targets(&data, request.catalog, cve_id, &target)?;
    let applied = components
        .iter()
        .map(|c| apply_one(conn, &data, request.scoring, c, cve_id, request.note, now))
        .collect::<Result<Vec<_>, _>>()?;
    resolve_row(conn, cve_id, ResolvedReason::ManualApply, now)?;
    Ok(Resolution {
        cve_id: cve_id.to_string(),
        applied,
        ignored: Vec::new(),
    })
}

/// 由系統建立、仍為 open 的 probable 命中（含已移除元件、退役資產）轉為忽略；
/// confirmed、人工命中與已處理者不動。
fn dismiss_probable(
    conn: &Connection,
    cve_id: &str,
    note: &str,
    now: &str,
) -> Result<Vec<i64>, CoreError> {
    // 包含已移除元件與退役資產的命中：之後恢復時已是忽略，不必在比對時推斷是否錯過判定
    let mut stmt = conn.prepare(
        "SELECT id FROM finding
         WHERE cve_id = ?1 AND status = 'open' AND confidence = 'probable' AND manual = 0
         ORDER BY id",
    )?;
    let ids = stmt
        .query_map(params![cve_id], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let change = Change {
        status: Status::Ignored,
        reason: Some(IgnoreReason::FalsePositive),
        justification: None,
        note: note.to_string(),
    };
    for id in &ids {
        apply_user_change(conn, *id, &change, true, now)?;
    }
    Ok(ids)
}

fn manual_note(action: &str, reason: PendingReason, note: &str) -> String {
    let base = format!("{action}（等待區：{}）", reason.label());
    if note.trim().is_empty() {
        base
    } else {
        format!("{base}：{}", note.trim())
    }
}

/// `--apply` 的對象：`<資產編號>` 或 `<資產編號>:<產品 key>`。
struct ApplyTarget<'a> {
    tag: &'a str,
    key: Option<&'a str>,
}

impl<'a> ApplyTarget<'a> {
    /// 資產編號本身可能含 `:`：先以完整字串查找使用中的資產，找不到才從最後一個 `:` 拆出產品 key
    /// （產品 key 只含小寫英數字與 -）。
    fn resolve(conn: &Connection, raw: &'a str) -> Result<Self, CoreError> {
        let raw = raw.trim();
        let whole_is_asset: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM asset
                            WHERE asset_tag = ?1 COLLATE NOCASE AND status = 'active')",
            params![raw],
            |r| r.get(0),
        )?;
        Ok(match raw.rsplit_once(':') {
            Some((t, k)) if !whole_is_asset => ApplyTarget {
                tag: t.trim(),
                key: Some(k.trim()),
            },
            _ => ApplyTarget {
                tag: raw,
                key: None,
            },
        })
    }
}

/// 套用對象：指定元件，或資產中與此 CVE 相關（有來源提到或描述關鍵字相符）的產品元件。
fn targets<'a>(
    data: &'a MatchData,
    catalog: &Catalog,
    cve_id: &str,
    target: &ApplyTarget<'_>,
) -> Result<Vec<&'a ActiveComponent>, CoreError> {
    let tag = target.tag;
    let products = asset_products(data, tag)?;
    let keys: Vec<&str> = products.iter().filter_map(|c| c.product_key()).collect();
    if let Some(key) = target.key {
        return products
            .iter()
            .find(|c| c.product_key().is_some_and(|k| k.eq_ignore_ascii_case(key)))
            .map(|c| vec![*c])
            .ok_or_else(|| {
                CoreError::Invalid(format!(
                    "資產 `{tag}` 沒有產品 `{key}`；可用：{}",
                    keys.join("、")
                ))
            });
    }
    let (candidates, related) = classify(data, catalog, cve_id, &products, &keys);
    if candidates.is_empty() {
        return Err(CoreError::Invalid(format!(
            "來源已判定 `{tag}` 的元件（{}）不受影響；若仍要套用，請以 `--apply {tag}:<產品>` 明確指定",
            keys.join("、")
        )));
    }
    match (related.is_empty(), candidates.len()) {
        (false, _) => Ok(related),
        (true, 1) => Ok(candidates),
        (true, _) => Err(CoreError::Invalid(format!(
            "無法判斷 {cve_id} 屬於 `{tag}` 的哪個元件；請以 `--apply {tag}:<產品>` 指定：{}",
            keys.join("、")
        ))),
    }
}

/// 使用中資產的產品元件（資產編號不分大小寫）。
fn asset_products<'a>(
    data: &'a MatchData,
    tag: &str,
) -> Result<Vec<&'a ActiveComponent>, CoreError> {
    let products: Vec<&ActiveComponent> = data
        .components
        .iter()
        .filter(|c| c.asset_tag.eq_ignore_ascii_case(tag) && c.product_key().is_some())
        .collect();
    if products.is_empty() {
        return Err(CoreError::Invalid(format!(
            "找不到使用中的資產 `{tag}`，或該資產沒有產品元件"
        )));
    }
    Ok(products)
}

/// (可套用的元件, 與此 CVE 相關的元件)。來源已明確判定不受影響者兩者皆排除；
/// 相關 = 有來源提到（未判定不受影響）或描述關鍵字相符。
fn classify<'a>(
    data: &MatchData,
    catalog: &Catalog,
    cve_id: &str,
    products: &[&'a ActiveComponent],
    keys: &[&str],
) -> (Vec<&'a ActiveComponent>, Vec<&'a ActiveComponent>) {
    let schemes = Schemes::from_catalog(catalog);
    let index = Products::new(catalog, &schemes);
    let description = data
        .cves
        .iter()
        .find(|c| c.id == cve_id)
        .and_then(|c| c.description.as_deref())
        .unwrap_or_default();
    let key_set = keys.iter().copied().collect();
    let by_keyword = KeywordIndex::new(catalog, &key_set).products_in(description);
    let mut candidates = Vec::new();
    let mut related = Vec::new();
    for c in products.iter().copied() {
        let verdict = evaluate_product(data, &index, c, cve_id).map(|d| d.verdict);
        if verdict == Some(Verdict::NotAffected) {
            continue;
        }
        candidates.push(c);
        let keyword = by_keyword
            .iter()
            .any(|k| Some(k.as_str()) == c.product_key());
        if verdict.is_some() || keyword {
            related.push(c);
        }
    }
    (candidates, related)
}

fn apply_one(
    conn: &Connection,
    data: &MatchData,
    scoring: &ScoringConfig,
    component: &ActiveComponent,
    cve_id: &str,
    note: &str,
    now: &str,
) -> Result<Applied, CoreError> {
    let product_key = component.product_key().unwrap_or_default().to_string();
    let existing: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, status FROM finding WHERE component_id = ?1 AND cve_id = ?2",
            params![component.id, cve_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (finding_id, created, reopened) = match existing {
        Some((id, status)) => (id, false, apply_existing(conn, id, &status, note, now)?),
        None => {
            let id = insert_manual(conn, data, scoring, component, cve_id, now)?;
            record_event(conn, id, now, "user", None, Status::Open, note)?;
            (id, true, false)
        }
    };
    Ok(Applied {
        finding_id,
        product_key,
        created,
        reopened,
    })
}

/// 既有命中：已是未處理時不寫無意義的事件，資料不足（probable）或已不在受影響範圍者以使用者判定為準
/// （標記人工並視為仍受影響）；其他狀態重新開啟並記錄事件。回傳是否重新開啟。
fn apply_existing(
    conn: &Connection,
    id: i64,
    status: &str,
    note: &str,
    now: &str,
) -> Result<bool, CoreError> {
    let from = super::triage::parse_status(id, status)?;
    if from == Status::Open {
        conn.execute(
            "UPDATE finding SET manual = 1, in_range = 1, last_evaluated = ?2
             WHERE id = ?1 AND (confidence = ?3 OR in_range = 0)",
            params![id, now, Confidence::Probable.as_str()],
        )?;
        return Ok(false);
    }
    reopen_existing(conn, id, now)?;
    record_event(conn, id, now, "user", Some(from), Status::Open, note)?;
    Ok(true)
}

/// 人工判定的 probable 命中。
fn insert_manual(
    conn: &Connection,
    data: &MatchData,
    scoring: &ScoringConfig,
    component: &ActiveComponent,
    cve_id: &str,
    now: &str,
) -> Result<i64, CoreError> {
    let scores = data.scores(cve_id);
    insert_finding(
        conn,
        &NewFinding {
            component_id: component.id,
            cve_id,
            confidence: Confidence::Probable,
            decided_by: MANUAL,
            priority: priority(&scores, component.exposure, scoring),
            kev_listed: scores.in_kev(),
            manual: true,
            status: Status::Open,
            ignore_reason: None,
            dismissed: false,
            now,
        },
    )
}

/// 使用者明確判定套用：既有命中改為 open、視為仍受影響並標記為人工判定。
fn reopen_existing(conn: &Connection, finding_id: i64, now: &str) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE finding SET status = 'open', ignore_reason = NULL, justification = NULL,
                in_range = 1, last_evaluated = ?2, manual = 1, dismissed = 0
         WHERE id = ?1",
        params![finding_id, now],
    )?;
    Ok(())
}
