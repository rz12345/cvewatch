//! 畫面資料：進入時一次載入到記憶體（數千列），篩選與排序在記憶體中進行。
//! 一律透過 core 的查詢函式存取資料庫。

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use cvewatch_core::assets::{
    Asset, AssetFilter, AssetRepo, AssetStatus, Component, ComponentRepo, HistoryEntry,
    UnresolvedEntry,
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
use crate::services::LinkPolicy;

/// Sync log 顯示的筆數。
pub const SYNC_LOG_LIMIT: u32 = 50;

/// 寫入操作需要的設定（與 CLI 相同）。
pub struct Settings {
    pub catalog: Catalog,
    pub scoring: ScoringConfig,
    pub stale_days: u32,
}

/// 資產畫面清單的篩選。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AssetListFilter {
    /// 含已退役資產
    pub include_retired: bool,
}

#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub asset: Asset,
    pub components: Vec<Component>,
    pub open_findings: usize,
    pub unresolved: Vec<UnresolvedEntry>,
    /// 元件 id → 版本歷史（新到舊）
    pub history: HashMap<i64, Vec<HistoryEntry>>,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub dashboard: Dashboard,
    /// 全部狀態的命中，依風險排序
    pub findings: Vec<FindingRow>,
    /// `findings[i]` 的搜尋鍵（[`crate::filter::search_key`]）
    pub search_keys: Vec<String>,
    /// 使用中資產（依 asset_tag 排序）
    pub assets: Vec<AssetEntry>,
    /// 已退役資產（資產畫面「顯示已退役」時列出；不參與比對、等待區套用與篩選）
    pub retired_assets: Vec<AssetEntry>,
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
        let (assets, retired_assets) = assets(conn, &findings, &unresolved)?;
        Ok(Snapshot {
            dashboard: dashboard::load(conn, stale_days, now)?,
            assets,
            retired_assets,
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

    /// 使用中或已退役的資產。
    pub fn any_asset(&self, tag: &str) -> Option<&AssetEntry> {
        self.asset(tag).or_else(|| {
            self.retired_assets
                .iter()
                .find(|a| a.asset.record.asset_tag.eq_ignore_ascii_case(tag))
        })
    }

    /// 資產畫面的清單（依 asset_tag 排序）；`include_retired` 時含已退役資產。
    pub fn asset_list(&self, filter: &AssetListFilter) -> Vec<&AssetEntry> {
        let mut list: Vec<&AssetEntry> = self.assets.iter().collect();
        if filter.include_retired {
            list.extend(self.retired_assets.iter());
            list.sort_by(|a, b| a.asset.record.asset_tag.cmp(&b.asset.record.asset_tag));
        }
        list
    }

    /// 資產的全部命中（依風險排序）。
    pub fn asset_findings(&self, tag: &str) -> Vec<&FindingRow> {
        self.findings
            .iter()
            .filter(|f| f.asset_tag.eq_ignore_ascii_case(tag))
            .collect()
    }
}

/// 使用中與已退役資產，各自含元件、未處理命中數、未辨識產品（元件一次查詢，計數在記憶體中彙整）。
fn assets(
    conn: &Connection,
    findings: &[FindingRow],
    unresolved: &[UnresolvedEntry],
) -> Result<(Vec<AssetEntry>, Vec<AssetEntry>), CoreError> {
    let mut components = ComponentRepo::new(conn).all_by_asset()?;
    let mut history = ComponentRepo::new(conn).history_by_component()?;
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
    let all = AssetRepo::new(conn).list(&AssetFilter {
        category: None,
        include_retired: true,
    })?;
    let (active, retired): (Vec<AssetEntry>, Vec<AssetEntry>) = all
        .into_iter()
        .map(|asset| {
            let tag = asset.record.asset_tag.as_str();
            let components = components.remove(&asset.id).unwrap_or_default();
            let history = components
                .iter()
                .filter_map(|c| history.remove(&c.id).map(|h| (c.id, h)))
                .collect();
            AssetEntry {
                components,
                history,
                open_findings: open.get(tag).copied().unwrap_or(0),
                unresolved: unresolved_by.remove(tag).unwrap_or_default(),
                asset,
            }
        })
        .partition(|a| a.asset.status == AssetStatus::Active);
    Ok((active, retired))
}

/// 命中詳情：證據、矛盾、時間軸（core）＋ CVE 描述、各來源分數、原廠修正與連結。
#[derive(Debug, Clone)]
pub struct DetailData {
    pub finding: FindingDetail,
    pub cve: Option<CveDetail>,
    pub links: Vec<Link>,
}

/// `policy`：此前端可開啟的連結種類（不符者不列出）。
pub fn load_detail(
    conn: &Connection,
    id: i64,
    policy: LinkPolicy,
) -> Result<Option<DetailData>, CoreError> {
    let Some(finding) = query::show(conn, id)? else {
        return Ok(None);
    };
    let cve = show::load(conn, &finding.row.cve_id)?;
    let links = allowed(links::for_cve(&finding.row.cve_id, cve.as_ref()), policy);
    Ok(Some(DetailData {
        finding,
        cve,
        links,
    }))
}

/// 單一 CVE 的連結（等待區用）。
pub fn cve_links(
    conn: &Connection,
    cve_id: &str,
    policy: LinkPolicy,
) -> Result<Vec<Link>, CoreError> {
    let cve = show::load(conn, cve_id)?;
    Ok(allowed(links::for_cve(cve_id, cve.as_ref()), policy))
}

fn allowed(links: Vec<Link>, policy: LinkPolicy) -> Vec<Link> {
    links
        .into_iter()
        .filter(|l| policy.allows(&l.url))
        .collect()
}
