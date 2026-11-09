//! `cvewatch match`：全量重算（sync 結束與資產匯入後自動執行）。

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, TransactionBehavior};

use super::evaluate::{evaluate_all, Products};
use super::evidence::osv::osv_is_current;
use super::findings::{self, ApplyStats};
use super::load::{ActiveComponent, MatchData, Target};
use super::pending::{self, PendingStats};
use super::scoring::ScoringConfig;
use super::version::Schemes;
use crate::assets::pending::pending_product_keys;
use crate::catalog::Catalog;
use crate::config::Config;
use crate::db::meta;
use crate::error::CoreError;

/// 有影響比對的資料寫入、但尚未完成全量比對（值為時間）。清冊匯入與 sync 的各來源都在
/// 寫入資料的同一個交易中、且只在資料有變動時記錄；全量比對成功時清除。
pub const MATCH_STALE_KEY: &str = "match_stale_since";

/// 上次全量比對完成的時間（TUI 以此偵測比對產生的新資料）。
pub const MATCH_LAST_RUN_KEY: &str = "match_last_run";

/// 上次全量比對使用的輸入指紋（產品目錄 + 評分設定）；變更後即使資料沒變也要重新比對。
pub const MATCH_INPUTS_KEY: &str = "match_inputs_fingerprint";

/// 記錄「比對待補」。
pub fn mark_match_stale(conn: &Connection, now: DateTime<Utc>) -> Result<(), CoreError> {
    meta::set(conn, MATCH_STALE_KEY, &now.to_rfc3339())
}

/// 是否有尚未比對的寫入。
pub fn match_is_stale(conn: &Connection) -> Result<bool, CoreError> {
    Ok(meta::get(conn, MATCH_STALE_KEY)?.is_some())
}

/// 影響比對結果的輸入：產品目錄（只含比對相關欄位）與評分門檻、曝露上調清單。
pub fn inputs_fingerprint(catalog: &Catalog, settings: &MatchSettings) -> String {
    let s = &settings.scoring;
    let exposures: Vec<&str> = s.exposure_bump.iter().map(|e| e.as_str()).collect();
    let text = format!(
        "{}\n{}|{}|{}|{}|{}",
        catalog.fingerprint(),
        s.epss_high,
        s.epss_medium,
        s.cvss_high,
        s.cvss_medium,
        exposures.join(",")
    );
    crate::hash::sha256_hex(&text)
}

/// 寫入前的比對狀態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RematchState {
    /// 有尚未比對的寫入（例如上次自動比對失敗）
    pub stale: bool,
    /// 產品目錄或評分設定自上次全量比對後已變更
    pub inputs_changed: bool,
}

impl RematchState {
    pub fn needed(self) -> bool {
        self.stale || self.inputs_changed
    }
}

pub fn rematch_state(
    conn: &Connection,
    catalog: &Catalog,
    settings: &MatchSettings,
) -> Result<RematchState, CoreError> {
    let fingerprint = meta::get(conn, MATCH_INPUTS_KEY)?;
    Ok(RematchState {
        stale: match_is_stale(conn)?,
        inputs_changed: fingerprint.as_deref()
            != Some(inputs_fingerprint(catalog, settings).as_str()),
    })
}

/// 比對參數（來自設定檔 `[scoring]`、`[pending]`）。
#[derive(Debug, Clone, PartialEq)]
pub struct MatchSettings {
    pub scoring: ScoringConfig,
    pub stale_days: u32,
    /// OSV 來源是否啟用（停用時套件元件無從判定，比對摘要會說明）
    pub osv_enabled: bool,
}

impl MatchSettings {
    pub fn from_config(config: &Config) -> Self {
        MatchSettings {
            scoring: config.scoring.clone(),
            stale_days: config.pending.stale_days,
            osv_enabled: config.sources.osv_enabled,
        }
    }
}

impl Default for MatchSettings {
    /// 與內建預設設定檔相同。
    fn default() -> Self {
        Self::from_config(&Config::default())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchReport {
    /// 參與比對的 active 元件數
    pub components: usize,
    /// 本機 CVE 數
    pub cves: usize,
    pub findings: ApplyStats,
    pub pending: PendingStats,
    /// 產品已不在目錄中（或版本格式無比較器）而略過的元件數與產品 key
    pub skipped_components: usize,
    pub skipped_products: Vec<String>,
    /// OSV 結果不是最新（版本變更後尚待重查，或 OSV 停用）而略過的套件數
    pub awaiting_osv: usize,
    pub osv_enabled: bool,
    /// affected 資料無法讀取而本次不比對的 CVE 數
    pub unreadable_cves: usize,
}

/// 在單一交易內重算全部命中與等待區；成功時清除「比對待補」並記錄輸入指紋。
///
/// 以 IMMEDIATE 交易一開始就取得寫入鎖：DEFERRED 交易先讀後寫，若讀取期間另一個指令
/// （例如 `finding set`）已提交，升級為寫入時會得到 `SQLITE_BUSY_SNAPSHOT` 且不會重試。
pub fn run_match(
    conn: &mut Connection,
    catalog: &Catalog,
    settings: &MatchSettings,
    now: DateTime<Utc>,
) -> Result<MatchReport, CoreError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let report = run_in(&tx, catalog, settings, now)?;
    meta::delete(&tx, MATCH_STALE_KEY)?;
    meta::set(
        &tx,
        MATCH_INPUTS_KEY,
        &inputs_fingerprint(catalog, settings),
    )?;
    meta::set(&tx, MATCH_LAST_RUN_KEY, &now.to_rfc3339())?;
    tx.commit()?;
    tracing::info!(
        components = report.components,
        cves = report.cves,
        new = report.findings.new,
        resolved = report.findings.resolved,
        reopened = report.findings.reopened,
        conflicts = report.findings.conflicts,
        pending = report.pending.active,
        "比對完成"
    );
    Ok(report)
}

/// 本次不比對的元件。
struct Skipped {
    /// 產品不在目錄中的元件 id
    component_ids: Vec<i64>,
    products: Vec<String>,
    awaiting_osv: usize,
}

/// 排除產品不在目錄中的元件（否則命中會被當成「來源不再列出」而自動處理），
/// 以及 OSV 結果不是最新或資料損壞的套件（以舊結果判定會誤改狀態，等下次 sync 重查）。
fn exclude_undecidable(loaded: MatchData, products: &Products<'_>) -> (MatchData, Skipped) {
    let unknown = |c: &ActiveComponent| c.product_key().is_some_and(|k| !products.knows(k));
    let last_osv = loaded.last_osv_sync.clone();
    let stale_osv = |c: &ActiveComponent| {
        matches!(c.target, Target::Package(_))
            && (!osv_is_current(&c.updated_at, last_osv.as_deref())
                || loaded.osv_corrupt.contains(&c.id))
    };
    let mut products_skipped = BTreeSet::new();
    let mut skipped = Skipped {
        component_ids: Vec::new(),
        products: Vec::new(),
        awaiting_osv: 0,
    };
    let mut excluded = BTreeSet::new();
    for c in &loaded.components {
        if unknown(c) {
            skipped.component_ids.push(c.id);
            products_skipped.extend(c.product_key().map(str::to_string));
            excluded.insert(c.id);
        } else if stale_osv(c) {
            skipped.awaiting_osv += 1;
            excluded.insert(c.id);
        }
    }
    skipped.products = products_skipped.into_iter().collect();
    if !skipped.products.is_empty() {
        tracing::warn!(
            products = %skipped.products.join(","),
            components = skipped.component_ids.len(),
            "產品已不在目錄中，這些元件本次不比對，既有命中維持不變"
        );
    }
    (
        loaded.retain_components(|c| !excluded.contains(&c.id)),
        skipped,
    )
}

/// 呼叫端已開啟交易時使用。
pub fn run_in(
    conn: &Connection,
    catalog: &Catalog,
    settings: &MatchSettings,
    now: DateTime<Utc>,
) -> Result<MatchReport, CoreError> {
    let schemes = Schemes::from_catalog(catalog);
    let products = Products::new(catalog, &schemes);
    let (data, skipped) = exclude_undecidable(MatchData::load(conn)?, &products);
    let active: BTreeSet<i64> = data.components.iter().map(|c| c.id).collect();
    // 資料損壞的 CVE 本次不比對：既有命中不動、不建立新命中
    let existing: std::collections::HashMap<_, _> = findings::load_existing(conn, &active)?
        .into_iter()
        .filter(|(key, _)| !data.corrupt_cves.contains(&key.1))
        .collect();
    let existing_keys: BTreeSet<(i64, String)> = existing.keys().cloned().collect();
    let rejected = data.rejected_cves();
    let dismissed = pending::dismissed(conn)?;
    let decisions: Vec<_> = evaluate_all(&data, &products, &existing_keys, &rejected)
        .into_iter()
        .filter(|d| !data.corrupt_cves.contains(&d.cve_id))
        .collect();
    if !data.corrupt_cves.is_empty() {
        tracing::warn!(
            cves = data.corrupt_cves.len(),
            "部分 CVE 的 affected 資料無法讀取，本次不比對（命中與等待區維持原狀）；重新同步可修復"
        );
    }
    let state = findings::RunState {
        data: &data,
        rejected: &rejected,
        dismissed: &dismissed,
    };
    let applied = findings::apply(conn, &state, &decisions, &existing, &settings.scoring, now)?;
    // 與略過的產品相關的等待區 CVE 無法斷定可以移出，維持原狀。只屬於版本待補元件的產品
    // 也算略過：待補期間不比對，但不能因此把等待區的 CVE 當成不再相關而移出（18 code review）
    let loaded_products: BTreeSet<&str> = data
        .components
        .iter()
        .filter_map(ActiveComponent::product_key)
        .collect();
    let skipped_products: BTreeSet<String> = skipped
        .products
        .iter()
        .cloned()
        .chain(
            pending_product_keys(conn)?
                .into_iter()
                .filter(|k| !loaded_products.contains(k.as_str())),
        )
        .collect();
    let pending = pending::reevaluate(
        conn,
        &data,
        &decisions,
        catalog,
        settings.stale_days,
        &skipped_products,
        now,
    )?;
    Ok(MatchReport {
        components: data.components.len(),
        cves: data.cves.len(),
        findings: applied,
        pending,
        skipped_components: skipped.component_ids.len(),
        skipped_products: skipped.products,
        awaiting_osv: skipped.awaiting_osv,
        osv_enabled: settings.osv_enabled,
        unreadable_cves: data.corrupt_cves.len(),
    })
}
