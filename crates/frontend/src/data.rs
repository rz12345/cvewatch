//! 畫面資料：進入時一次載入到記憶體（數千列），篩選與排序在記憶體中進行。
//! 一律透過 core 的查詢函式存取資料庫。

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use cvewatch_core::assets::{
    Asset, AssetFilter, AssetRepo, Component, ComponentRepo, UnresolvedEntry,
};
use cvewatch_core::catalog::Catalog;
use cvewatch_core::ingest::runlog::{self, RunRow};
use cvewatch_core::ingest::show::{self, CveDetail};
use cvewatch_core::matching::dashboard::{self, Dashboard, DataMarker};
use cvewatch_core::matching::pending_ops::{self, PendingRow};
use cvewatch_core::matching::query::{self, FindingDetail, FindingFilter, FindingRow};
use cvewatch_core::matching::scoring::ScoringConfig;
use cvewatch_core::matching::triage::Status;
use cvewatch_core::CoreError;
use rusqlite::Connection;

use crate::links::{self, Link};

/// Sync log 顯示的筆數。
pub const SYNC_LOG_LIMIT: u32 = 50;

/// 寫入操作需要的設定（與 CLI 相同）。
pub struct Settings {
    pub catalog: Catalog,
    pub scoring: ScoringConfig,
    pub stale_days: u32,
}

#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub asset: Asset,
    pub components: Vec<Component>,
    pub open_findings: usize,
    pub unresolved: Vec<UnresolvedEntry>,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub dashboard: Dashboard,
    /// 全部狀態的命中，依風險排序
    pub findings: Vec<FindingRow>,
    /// `findings[i]` 的搜尋鍵（[`crate::filter::search_key`]）
    pub search_keys: Vec<String>,
    pub assets: Vec<AssetEntry>,
    pub unresolved: Vec<UnresolvedEntry>,
    pub pending: Vec<PendingRow>,
    pub sync_runs: Vec<RunRow>,
    /// 載入時的同步／比對標記；之後改變即表示有新資料
    pub marker: DataMarker,
    pub loaded_at: DateTime<Utc>,
}

impl Snapshot {
    /// 標記先讀：載入期間結束的同步仍會在下次檢查時被偵測到。
    pub fn load(conn: &Connection, stale_days: u32, now: DateTime<Utc>) -> Result<Self, CoreError> {
        let marker = dashboard::data_marker(conn)?;
        let findings = query::list(conn, &FindingFilter::default())?;
        let search_keys = findings.iter().map(crate::filter::search_key).collect();
        let unresolved = AssetRepo::new(conn).unresolved()?;
        Ok(Snapshot {
            dashboard: dashboard::load(conn, stale_days, now)?,
            assets: assets(conn, &findings, &unresolved)?,
            findings,
            search_keys,
            unresolved,
            pending: pending_ops::list(conn, false, stale_days, now)?,
            sync_runs: runlog::history(conn, SYNC_LOG_LIMIT)?,
            marker,
            loaded_at: now,
        })
    }

    pub fn asset(&self, tag: &str) -> Option<&AssetEntry> {
        self.assets
            .iter()
            .find(|a| a.asset.record.asset_tag.eq_ignore_ascii_case(tag))
    }

    /// 資產的全部命中（依風險排序）。
    pub fn asset_findings(&self, tag: &str) -> Vec<&FindingRow> {
        self.findings
            .iter()
            .filter(|f| f.asset_tag.eq_ignore_ascii_case(tag))
            .collect()
    }
}

/// 使用中資產與其元件、未處理命中數、未辨識產品（元件一次查詢，計數在記憶體中彙整）。
fn assets(
    conn: &Connection,
    findings: &[FindingRow],
    unresolved: &[UnresolvedEntry],
) -> Result<Vec<AssetEntry>, CoreError> {
    let mut components = ComponentRepo::new(conn).all_by_asset()?;
    let mut open: HashMap<&str, usize> = HashMap::new();
    for f in findings.iter().filter(|f| f.status == Status::Open) {
        *open.entry(f.asset_tag.as_str()).or_default() += 1;
    }
    let mut unresolved_by: HashMap<&str, Vec<UnresolvedEntry>> = HashMap::new();
    for u in unresolved {
        unresolved_by
            .entry(u.asset_tag.as_str())
            .or_default()
            .push(u.clone());
    }
    let assets = AssetRepo::new(conn).list(&AssetFilter::default())?;
    Ok(assets
        .into_iter()
        .map(|asset| {
            let tag = asset.record.asset_tag.as_str();
            AssetEntry {
                components: components.remove(&asset.id).unwrap_or_default(),
                open_findings: open.get(tag).copied().unwrap_or(0),
                unresolved: unresolved_by.remove(tag).unwrap_or_default(),
                asset,
            }
        })
        .collect())
}

/// 命中詳情：證據、矛盾、時間軸（core）＋ CVE 描述、各來源分數、原廠修正與連結。
#[derive(Debug, Clone)]
pub struct DetailData {
    pub finding: FindingDetail,
    pub cve: Option<CveDetail>,
    pub links: Vec<Link>,
}

pub fn load_detail(conn: &Connection, id: i64) -> Result<Option<DetailData>, CoreError> {
    let Some(finding) = query::show(conn, id)? else {
        return Ok(None);
    };
    let cve = show::load(conn, &finding.row.cve_id)?;
    let links = links::for_cve(&finding.row.cve_id, cve.as_ref());
    Ok(Some(DetailData {
        finding,
        cve,
        links,
    }))
}

/// 單一 CVE 的連結（等待區用）。
pub fn cve_links(conn: &Connection, cve_id: &str) -> Result<Vec<Link>, CoreError> {
    let cve = show::load(conn, cve_id)?;
    Ok(links::for_cve(cve_id, cve.as_ref()))
}
