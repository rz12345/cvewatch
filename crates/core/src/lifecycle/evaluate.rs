//! 生命週期判定（22 design D9）：硬體、各清冊元件的軟體 EoL 與三個到期日，依今天與提醒天數
//! 分類。主機套件元件與未辨識產品不產生項目。

use chrono::NaiveDate;

use super::cycle::{by_name, by_name_or_latest, comparable_version, CycleMatch};
use super::eol_data::EolData;
use super::hardware::hardware_state;
use super::model::{
    AssetLifecycle, ItemKind, ItemState, LifecycleItem, MissingReason, Source, Status,
};
use crate::assets::{Asset, Component, ComponentKind};
use crate::catalog::{Catalog, Product};

pub const HARDWARE_LABEL: &str = "硬體 EoL";
pub const WARRANTY_LABEL: &str = "保固到期";
pub const SUPPORT_LABEL: &str = "維護合約到期";
pub const LICENSE_LABEL: &str = "軟體授權到期";

/// 日期與今天的關係：當天已算到期；`window` 天內為即將。
pub fn status_of(state: &ItemState, today: NaiveDate, window: u32) -> Status {
    match state {
        ItemState::Dated { date, .. } => {
            let days_left = (*date - today).num_days();
            if days_left <= 0 {
                Status::Past {
                    days_ago: Some(-days_left),
                }
            } else if days_left <= i64::from(window) {
                Status::Soon { days_left }
            } else {
                Status::Later { days_left }
            }
        }
        ItemState::EolUndated { .. } => Status::Past { days_ago: None },
        ItemState::NotAnnounced { .. } | ItemState::Missing(_) => Status::None,
    }
}

/// 判定一個資產；`components` 為該資產的元件（套件元件會被略過）。retired 資產也可計算
/// （詳情用），清單標示、篩選與計數由呼叫端只取 active 資產。
pub fn evaluate(
    asset: &Asset,
    components: &[Component],
    catalog: &Catalog,
    eol: &EolData,
    today: NaiveDate,
    window: u32,
) -> AssetLifecycle {
    let item = |kind, label: String, detail, cycle, state: ItemState| LifecycleItem {
        status: status_of(&state, today, window),
        kind,
        label,
        detail,
        cycle,
        state,
    };
    let r = &asset.record;
    let (hw_state, matched) = hardware_state(r, catalog.hardware());
    let mut items = vec![item(
        ItemKind::Hardware,
        HARDWARE_LABEL.into(),
        matched,
        None,
        hw_state,
    )];
    for c in components.iter().filter(|c| c.is_inventory_product()) {
        let ComponentKind::Product { product_key, .. } = &c.kind else {
            continue;
        };
        let Some(product) = catalog.get(product_key) else {
            continue;
        };
        let (state, cycle) = software_state(c, product, catalog, eol);
        let version = (!c.is_pending()).then(|| c.version_raw.trim().to_string());
        items.push(item(
            ItemKind::Software { component_id: c.id },
            product.display_name.clone(),
            version,
            cycle,
            state,
        ));
    }
    for (kind, label, date) in [
        (ItemKind::Warranty, WARRANTY_LABEL, r.warranty_end),
        (ItemKind::Support, SUPPORT_LABEL, r.support_end),
        (ItemKind::License, LICENSE_LABEL, r.license_end),
    ] {
        let state = match date {
            Some(date) => ItemState::Dated {
                date,
                source: Source::AssetField,
            },
            None => ItemState::Missing(MissingReason::NotFilled),
        };
        items.push(item(kind, label.into(), None, None, state));
    }
    AssetLifecycle { items }
}

/// 軟體 EoL 的判定順序（spec Software end-of-life determination）：元件覆寫 → 目錄宣告的分支
/// → endoflife.date。回傳狀態與對應到的分支。
fn software_state(
    c: &Component,
    product: &Product,
    catalog: &Catalog,
    eol: &EolData,
) -> (ItemState, Option<String>) {
    if let Some(o) = &c.eol_override {
        let state = ItemState::Dated {
            date: o.date,
            source: Source::Override(o.source.clone()),
        };
        return (state, None);
    }
    if c.is_pending() {
        return (ItemState::Missing(MissingReason::PendingVersion), None);
    }
    let raw = c.version_raw.trim();
    let unmapped = || {
        (
            ItemState::Missing(MissingReason::Unmapped {
                version: raw.to_string(),
            }),
            None,
        )
    };
    let Some(version) = comparable_version(raw, &product.version_scheme, catalog.esxi_builds())
    else {
        return unmapped();
    };
    if !product.eol_cycles.is_empty() {
        let names = product.eol_cycles.iter().map(|c| c.cycle.as_str());
        if let CycleMatch::Found(i) = by_name(names, &version) {
            let cycle = &product.eol_cycles[i];
            let state = ItemState::Dated {
                date: cycle.eol,
                source: Source::Catalog(cycle.source.clone()),
            };
            return (state, Some(cycle.cycle.clone()));
        }
        if product.endoflife.is_none() {
            return unmapped();
        }
    }
    let Some(slug) = &product.endoflife else {
        return (ItemState::Missing(MissingReason::NoEolData), None);
    };
    let Some(data) = eol.get(slug) else {
        return (ItemState::Missing(MissingReason::NotSynced), None);
    };
    // 404 時保留先前的分支資料：有資料仍判定（寧可提醒），沒有資料才顯示查無此產品
    if data.releases.is_empty() {
        let reason = if data.not_found {
            MissingReason::SlugNotFound
        } else {
            MissingReason::NoEolData
        };
        return (ItemState::Missing(reason), None);
    }
    let candidates: Vec<(&str, Option<&str>)> = data
        .releases
        .iter()
        .map(|r| (r.name.as_str(), r.latest_name.as_deref()))
        .collect();
    let CycleMatch::Found(i) = by_name_or_latest(&candidates, &version) else {
        return unmapped();
    };
    let release = &data.releases[i];
    let source = Source::EndOfLifeDate {
        slug: slug.clone(),
        cycle: release.name.clone(),
        link: data.html_link.clone(),
    };
    let state = match (release.eol_from, release.is_eol) {
        (Some(date), _) => ItemState::Dated { date, source },
        (None, true) => ItemState::EolUndated { source },
        (None, false) => ItemState::NotAnnounced {
            cycle: release.name.clone(),
        },
    };
    (state, Some(release.name.clone()))
}

#[cfg(test)]
#[path = "evaluate_tests.rs"]
mod tests;
