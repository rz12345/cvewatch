//! Dashboard 與每日摘要的生命週期計數（22 design D9、D12）：只計 active 資產。

use chrono::{Duration, NaiveDate};
use rusqlite::Connection;

use super::eol_data::load_eol;
use super::evaluate::evaluate;
use super::model::{AssetLifecycle, ItemState, LifecycleFlags, Status};
use crate::assets::{Asset, AssetFilter, AssetRepo, ComponentRepo};
use crate::catalog::Catalog;
use crate::error::CoreError;

/// 使用中資產與其生命週期（依資產編號）。
pub fn evaluate_active(
    conn: &Connection,
    catalog: &Catalog,
    today: NaiveDate,
    window: u32,
) -> Result<Vec<(Asset, AssetLifecycle)>, CoreError> {
    let eol = load_eol(conn)?;
    let assets = AssetRepo::new(conn).list(&AssetFilter::default())?;
    let mut components = ComponentRepo::new(conn).all_by_asset()?;
    Ok(assets
        .into_iter()
        .map(|a| {
            let list = components.remove(&a.id).unwrap_or_default();
            let lifecycle = evaluate(&a, &list, catalog, &eol, today, window);
            (a, lifecycle)
        })
        .collect())
}

/// 已 EoL、即將 EoL 與保固或合約到期（含即將到期與已到期）的資產數。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LifecycleCounts {
    pub eol: usize,
    pub eol_soon: usize,
    pub expiry: usize,
    /// 提醒天數
    pub window: u32,
}

impl LifecycleCounts {
    pub fn from_assets(assets: &[(Asset, AssetLifecycle)], window: u32) -> LifecycleCounts {
        Self::from_flags(assets.iter().map(|(_, l)| l.flags()), window)
    }

    /// 以使用中資產的旗標計數（Dashboard 與摘要共用同一套規則）。
    pub fn from_flags(
        flags: impl IntoIterator<Item = LifecycleFlags>,
        window: u32,
    ) -> LifecycleCounts {
        let mut c = LifecycleCounts {
            window,
            ..LifecycleCounts::default()
        };
        for f in flags {
            c.eol += usize::from(f.eol);
            c.eol_soon += usize::from(f.eol_soon);
            c.expiry += usize::from(f.expiry());
        }
        c
    }

    pub fn is_empty(&self) -> bool {
        self.eol == 0 && self.eol_soon == 0 && self.expiry == 0
    }
}

/// Dashboard 的計數。
pub fn summary(
    conn: &Connection,
    catalog: &Catalog,
    today: NaiveDate,
    window: u32,
) -> Result<LifecycleCounts, CoreError> {
    let assets = evaluate_active(conn, catalog, today, window)?;
    Ok(LifecycleCounts::from_assets(&assets, window))
}

/// 摘要的一列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestEntry {
    pub asset_tag: String,
    /// 項目（「維護合約到期」「PAN-OS 10.2.9（分支 10.2）」）
    pub subject: String,
    pub date: NaiveDate,
    /// 硬體或軟體 EoL（其他為到期）
    pub is_eol: bool,
    /// 剩餘天數（`until` 當天起算）
    pub days_left: i64,
}

/// 每日摘要的生命週期段落。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LifecycleDigest {
    /// 提醒天數內的項目（依日期）
    pub upcoming: Vec<DigestEntry>,
    /// 摘要期間內 EoL 或到期的項目
    pub reached: Vec<DigestEntry>,
    pub eol_assets: usize,
    pub expired_assets: usize,
    /// 期間內進入提醒天數或到期（算新事項）
    pub news: bool,
    pub window: u32,
}

impl LifecycleDigest {
    /// 沒有任何生命週期內容（省略整個段落）。
    pub fn is_empty(&self) -> bool {
        self.upcoming.is_empty()
            && self.reached.is_empty()
            && self.eol_assets == 0
            && self.expired_assets == 0
    }
}

/// 軟體項目加上「EoL」：「PAN-OS 10.2.9（分支 10.2）EoL」；硬體與到期項目的名稱已足以說明。
fn digest_subject(item: &super::model::LifecycleItem) -> String {
    let subject = item.subject();
    match item.kind {
        super::model::ItemKind::Software { .. } => {
            let sep = if subject.ends_with('）') { "" } else { " " };
            format!("{subject}{sep}EoL")
        }
        _ => subject,
    }
}

/// 以摘要期間 `(since, until]`（使用者時區的日期）整理；`assets` 以 `until` 當天評估。
pub fn digest(
    assets: &[(Asset, AssetLifecycle)],
    since: NaiveDate,
    until: NaiveDate,
    window: u32,
) -> LifecycleDigest {
    let in_period = |d: NaiveDate| d > since && d <= until;
    let mut out = LifecycleDigest {
        window,
        ..LifecycleDigest::default()
    };
    for (asset, lifecycle) in assets {
        let flags = lifecycle.flags();
        out.eol_assets += usize::from(flags.eol);
        out.expired_assets += usize::from(flags.expired);
        for item in &lifecycle.items {
            let ItemState::Dated { date, .. } = item.state else {
                continue;
            };
            let entry = DigestEntry {
                asset_tag: asset.record.asset_tag.clone(),
                subject: digest_subject(item),
                date,
                is_eol: item.kind.is_eol(),
                days_left: (date - until).num_days(),
            };
            if in_period(date) {
                out.news = true;
                out.reached.push(entry);
            } else if let Status::Soon { .. } = item.status {
                if in_period(date - Duration::days(i64::from(window))) {
                    out.news = true;
                }
                out.upcoming.push(entry);
            }
        }
    }
    let order = |e: &DigestEntry| (e.date, e.asset_tag.clone(), e.subject.clone());
    out.upcoming.sort_by_key(order);
    out.reached.sort_by_key(order);
    out
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
