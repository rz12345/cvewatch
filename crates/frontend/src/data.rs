//! 畫面資料：進入時一次載入到記憶體（數千列），篩選與排序在記憶體中進行。
//! 一律透過 core 的查詢函式存取資料庫。

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use cvewatch_core::assets::systems::SystemRepo;
use cvewatch_core::assets::{
    Asset, AssetClasses, AssetFilter, AssetIp, AssetIpRepo, AssetRepo, AssetStatus, ClassFilter,
    Component, ComponentRepo, HistoryEntry, SystemCode, SystemDirectory, UnresolvedEntry,
};
use cvewatch_core::catalog::Catalog;
use cvewatch_core::ingest::runlog::{self, RunRow};
use cvewatch_core::ingest::show::{self, CveDetail};
use cvewatch_core::lifecycle::{
    evaluate, load_eol, AssetLifecycle, EolData, LifecycleCounts, LifecycleFilter, LifecycleFlags,
};
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
    /// 目前生效的 ISMS 子類別（內建 + 設定檔）
    pub classes: AssetClasses,
    pub scoring: ScoringConfig,
    pub stale_days: u32,
    /// 生命週期提醒天數（`[lifecycle] reminder_days`）
    pub reminder_days: u32,
}

/// 資產畫面清單的篩選。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetListFilter {
    /// 含已退役資產
    pub include_retired: bool,
    /// 只列出有版本待補元件的資產（Dashboard 的版本待補數字；18 design D8）
    pub pending_only: bool,
    /// 子類別或大類（None = 全部；選項見 [`crate::filter::class_filter_choices`]）
    pub class: Option<ClassFilter>,
    /// 搜尋字（編號、名稱、主機名稱、序號、財產編號、IP；空白不篩選，20 design D10）
    pub search: String,
    /// 生命週期（None = 全部；retired 資產不符合任何生命週期篩選，22 design D11）
    pub lifecycle: Option<LifecycleFilter>,
}

#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub asset: Asset,
    pub components: Vec<Component>,
    pub open_findings: usize,
    pub unresolved: Vec<UnresolvedEntry>,
    /// 元件 id → 版本歷史（新到舊）
    pub history: HashMap<i64, Vec<HistoryEntry>>,
    /// IP（依儲存順序）
    pub ips: Vec<AssetIp>,
    /// 生命週期（以快照的「今天」計算；retired 資產也計算，詳情用）
    pub lifecycle: AssetLifecycle,
    /// `lifecycle.flags()` 的快取（清單篩選與標示每次重繪都用到）
    pub lifecycle_flags: LifecycleFlags,
}

impl AssetEntry {
    /// 生命週期篩選：只有使用中資產會符合。
    pub fn matches_lifecycle(&self, filter: LifecycleFilter) -> bool {
        self.asset.status == AssetStatus::Active && filter.matches(self.lifecycle_flags)
    }

    /// 有版本待補的清冊元件（18 design D1；未辨識產品另有「未辨識」標示，不列入）。
    pub fn has_pending(&self) -> bool {
        self.components
            .iter()
            .any(|c| c.is_inventory_product() && c.is_pending())
    }
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
    /// 資產說明（16 design D8）
    pub systems: SystemDirectory,
    /// 每個資產說明歸屬的資產數（含已退役；資產說明對話框的刪除檢查）
    pub system_usage: HashMap<SystemCode, u32>,
    pub pending: Vec<PendingRow>,
    pub sync_runs: Vec<RunRow>,
    /// 載入時的同步／比對標記；之後改變即表示有新資料
    pub marker: DataMarker,
    pub loaded_at: DateTime<Utc>,
    /// endoflife.date 資料（跨日重算生命週期時使用）
    pub eol: EolData,
    /// 生命週期以此日期計算（使用者時區）
    pub lifecycle_today: NaiveDate,
    /// Dashboard 的生命週期計數（只計使用中資產）
    pub lifecycle_counts: LifecycleCounts,
}

impl Snapshot {
    /// 標記先讀：載入期間結束的同步仍會在下次檢查時被偵測到。`today` 為使用者時區的日期
    /// （生命週期）。
    pub fn load(
        conn: &Connection,
        settings: &Settings,
        now: DateTime<Utc>,
        today: NaiveDate,
    ) -> Result<Self, CoreError> {
        let marker = dashboard::data_marker(conn)?;
        let findings = query::list(conn, &FindingFilter::default())?;
        let search_keys = findings.iter().map(crate::filter::search_key).collect();
        let unresolved = AssetRepo::new(conn).unresolved()?;
        let (assets, retired_assets) = assets(conn, &findings, &unresolved)?;
        let stale_days = settings.stale_days;
        let mut snapshot = Snapshot {
            eol: load_eol(conn)?,
            lifecycle_today: today,
            lifecycle_counts: LifecycleCounts::default(),
            dashboard: dashboard::load(conn, stale_days, now)?,
            assets,
            retired_assets,
            findings,
            search_keys,
            unresolved,
            systems: SystemDirectory::load(conn)?,
            system_usage: SystemRepo::new(conn).usage_counts()?.into_iter().collect(),
            pending: pending_ops::list(conn, false, stale_days, now)?,
            sync_runs: runlog::history(conn, SYNC_LOG_LIMIT)?,
            marker,
            loaded_at: now,
        };
        snapshot.compute_lifecycle(&settings.catalog, settings.reminder_days, today);
        Ok(snapshot)
    }

    /// 以 `today` 重算每個資產的生命週期與 Dashboard 計數。
    fn compute_lifecycle(&mut self, catalog: &Catalog, window: u32, today: NaiveDate) {
        let eol = &self.eol;
        for entry in self.assets.iter_mut().chain(self.retired_assets.iter_mut()) {
            entry.lifecycle =
                evaluate(&entry.asset, &entry.components, catalog, eol, today, window);
            entry.lifecycle_flags = entry.lifecycle.flags();
        }
        self.lifecycle_counts =
            LifecycleCounts::from_flags(self.assets.iter().map(|e| e.lifecycle_flags), window);
        self.lifecycle_today = today;
    }

    /// 跨日時重算生命週期（GUI 切到資產或 Dashboard 畫面時呼叫）；回傳是否有重算。
    pub fn refresh_lifecycle(&mut self, settings: &Settings, today: NaiveDate) -> bool {
        if today == self.lifecycle_today {
            return false;
        }
        self.compute_lifecycle(&settings.catalog, settings.reminder_days, today);
        true
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

    /// 資產畫面的清單（依 asset_tag 排序）；`include_retired` 時含已退役資產，`class` 篩選子類別或大類。
    pub fn asset_list(&self, filter: &AssetListFilter) -> Vec<&AssetEntry> {
        let mut list: Vec<&AssetEntry> = self.assets.iter().collect();
        if filter.include_retired {
            list.extend(self.retired_assets.iter());
            list.sort_by(|a, b| a.asset.record.asset_tag.cmp(&b.asset.record.asset_tag));
        }
        if let Some(class) = &filter.class {
            list.retain(|a| class.matches(&a.asset.record.category));
        }
        if filter.pending_only {
            list.retain(|a| a.has_pending());
        }
        if let Some(lifecycle) = filter.lifecycle {
            list.retain(|a| a.matches_lifecycle(lifecycle));
        }
        if let Some(search) = crate::filter::AssetSearch::new(&filter.search) {
            list.retain(|a| search.matches(a));
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
    let mut ips = AssetIpRepo::new(conn).all_by_asset()?;
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
                ips: ips.remove(&asset.id).unwrap_or_default(),
                lifecycle: AssetLifecycle::default(),
                lifecycle_flags: LifecycleFlags::default(),
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
