//! 清冊匯入：在單一交易內同步目錄、計算差異並寫入；`--dry-run` 時整批回復。

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use super::model::{AssetStatus, ComponentKind, ComponentSource};
use super::plan::{plan_import, CurrentAsset, ImportPlan, PlannedAsset, Snapshot};
use super::repo::{AssetFilter, AssetRepo, ComponentRepo};
use super::validate::Inventory;
use crate::catalog::store::sync_catalog;
use crate::catalog::Catalog;
use crate::error::CoreError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportOptions {
    pub dry_run: bool,
    pub prune: bool,
}

/// 匯入清冊並回傳差異；`dry_run` 時資料庫不變。
pub fn import_inventory(
    conn: &mut Connection,
    catalog: &Catalog,
    inventory: &Inventory,
    options: ImportOptions,
    now: DateTime<Utc>,
) -> Result<ImportPlan, CoreError> {
    let tx = conn.transaction()?;
    let product_ids = sync_catalog(&tx, catalog)?;
    let snapshot = load_snapshot(&tx)?;
    let plan = plan_import(inventory, catalog, &snapshot)?;
    if options.dry_run {
        tx.rollback()?;
        return Ok(plan);
    }
    for planned in &plan.assets {
        apply_asset(
            &tx,
            planned,
            snapshot.get(&planned.record.asset_tag),
            &product_ids,
            now,
        )?;
    }
    if options.prune {
        let assets = AssetRepo::new(&tx);
        for tag in &plan.disappeared {
            assets.retire(snapshot[tag].id, now)?;
        }
    }
    tx.commit()?;
    tracing::info!(
        added = plan.added.len(),
        updated = plan.updated.len(),
        unchanged = plan.unchanged,
        unresolved = plan.unresolved.len(),
        disappeared = plan.disappeared.len(),
        pruned = options.prune,
        "清冊匯入完成"
    );
    Ok(plan)
}

/// 所有資產（含 retired）與其 Excel 來源的產品元件。
pub fn load_snapshot(conn: &Connection) -> Result<Snapshot, CoreError> {
    let assets = AssetRepo::new(conn).list(&AssetFilter {
        category: None,
        include_retired: true,
    })?;
    let components = ComponentRepo::new(conn);
    assets
        .into_iter()
        .map(|asset| {
            let excel: BTreeMap<String, String> = components
                .for_asset(asset.id)?
                .into_iter()
                .filter(|c| c.source == ComponentSource::Excel)
                .filter_map(|c| match c.kind {
                    ComponentKind::Product { product_key, .. } => {
                        Some((product_key, c.version_raw))
                    }
                    ComponentKind::Package { .. } => None,
                })
                .collect();
            let current = CurrentAsset {
                id: asset.id,
                record: asset.record,
                status: asset.status,
                components: excel,
            };
            Ok((current.record.asset_tag.clone(), current))
        })
        .collect()
}

fn apply_asset(
    conn: &Connection,
    planned: &PlannedAsset,
    existing: Option<&CurrentAsset>,
    product_ids: &HashMap<String, i64>,
    now: DateTime<Utc>,
) -> Result<(), CoreError> {
    let assets = AssetRepo::new(conn);
    let components = ComponentRepo::new(conn);
    let asset_id = match existing {
        None => assets.insert(&planned.record, now)?,
        Some(current) => {
            if current.record != planned.record || current.status == AssetStatus::Retired {
                assets.update_record(current.id, &planned.record, now)?;
            } else {
                assets.touch(current.id, now)?;
            }
            current.id
        }
    };
    for c in &planned.components {
        let product_id = product_ids[&c.product_key];
        components.upsert_product(
            asset_id,
            product_id,
            &c.version_raw,
            c.version_norm.as_deref(),
            now,
        )?;
    }
    if let Some(current) = existing {
        remove_dropped_components(conn, asset_id, current, planned)?;
    }
    assets.replace_unresolved(asset_id, &planned.unresolved, now)?;
    Ok(())
}

/// 清冊中已不再列出的 Excel 產品元件予以刪除（套件元件不受影響）。
fn remove_dropped_components(
    conn: &Connection,
    asset_id: i64,
    current: &CurrentAsset,
    planned: &PlannedAsset,
) -> Result<(), CoreError> {
    for key in current.components.keys() {
        if planned.components.iter().any(|c| &c.product_key == key) {
            continue;
        }
        conn.execute(
            "DELETE FROM component WHERE asset_id = ?1 AND source = 'excel'
               AND product_id = (SELECT id FROM catalog_product WHERE key = ?2)",
            params![asset_id, key],
        )?;
    }
    Ok(())
}
