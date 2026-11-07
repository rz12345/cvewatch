//! 套件清單寫入：整批取代該資產的套件元件，版本變更寫入歷史。
//! 清單中已沒有的套件軟刪除（命中與處理紀錄保留），再次出現時沿用原元件。

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use super::ParsedPackages;
use crate::assets::model::{AssetStatus, ComponentKind};
use crate::assets::repo::{AssetRepo, ComponentRepo, PackageUpsert, UpsertOutcome};
use crate::error::CoreError;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageImportReport {
    pub added: usize,
    /// (purl 識別, 舊版本, 新版本)
    pub version_changes: Vec<(String, String, String)>,
    pub removed: Vec<String>,
    pub unchanged: usize,
}

pub fn import_packages(
    conn: &mut Connection,
    asset_tag: &str,
    packages: &ParsedPackages,
    now: DateTime<Utc>,
) -> Result<PackageImportReport, CoreError> {
    let tx = conn.transaction()?;
    let asset = AssetRepo::new(&tx).find_by_tag(asset_tag)?.ok_or_else(|| {
        CoreError::Invalid(format!(
            "資產「{asset_tag}」不存在；請先以 `cvewatch asset import` 建立"
        ))
    })?;
    if asset.status != AssetStatus::Active {
        return Err(CoreError::Invalid(format!(
            "資產「{asset_tag}」已退役，不接受套件匯入"
        )));
    }
    let components = ComponentRepo::new(&tx);
    let mut report = PackageImportReport::default();
    let source = packages.format.source();
    for entry in &packages.entries {
        let upsert = PackageUpsert {
            purl: &entry.purl,
            purl_full: &entry.purl_full,
            version_raw: &entry.version,
            source,
        };
        match components.upsert_package(asset.id, &upsert, now)? {
            UpsertOutcome::Inserted => report.added += 1,
            UpsertOutcome::Unchanged => report.unchanged += 1,
            UpsertOutcome::VersionChanged { previous } => {
                report
                    .version_changes
                    .push((entry.purl.clone(), previous, entry.version.clone()));
            }
        }
    }
    let incoming: HashSet<&str> = packages.entries.iter().map(|e| e.purl.as_str()).collect();
    for c in components.for_asset(asset.id)? {
        if let ComponentKind::Package { purl, .. } = &c.kind {
            if !incoming.contains(purl.as_str()) {
                components.remove(c.id, now)?;
                report.removed.push(purl.clone());
            }
        }
    }
    tx.commit()?;
    tracing::info!(
        asset_tag,
        format = packages.format.as_str(),
        added = report.added,
        changed = report.version_changes.len(),
        removed = report.removed.len(),
        unchanged = report.unchanged,
        "套件清單匯入完成"
    );
    Ok(report)
}
