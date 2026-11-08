//! 匯入差異計算（純函式）：新增、更新、版本變更、未辨識、清冊中消失。

use std::collections::{BTreeMap, HashMap, HashSet};

use super::model::{AssetRecord, AssetStatus};
use super::validate::{IncomingAsset, Inventory, RowRef};
use crate::catalog::{Catalog, Product};
use crate::error::CoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedComponent {
    pub row: RowRef,
    pub product_key: String,
    pub version_raw: String,
    pub version_norm: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedItem {
    pub row: RowRef,
    pub asset_tag: String,
    pub product_raw: String,
    pub version_raw: String,
    pub suggestions: Vec<String>,
}

/// 版本字串不符合產品的版本格式（仍保存原始字串）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionWarning {
    /// 清冊匯入時的列位置；畫面編修時為 None
    pub row: Option<RowRef>,
    pub asset_tag: String,
    pub product_key: String,
    pub version_raw: String,
    pub scheme: String,
}

impl VersionWarning {
    /// 不含列位置的說明（匯入與畫面編修相同）。
    pub fn describe(&self) -> String {
        self.describe_with(&self.product_key)
    }

    /// 與 [`describe`](Self::describe) 相同，但產品以 `product` 呈現（畫面用顯示名稱）。
    pub fn describe_with(&self, product: &str) -> String {
        format!(
            "{} {product}「{}」不符合 {} 格式",
            self.asset_tag, self.version_raw, self.scheme
        )
    }
}

/// 以目錄產品的版本格式解析版本字串：回傳 `version_norm` 與無法解析時的警告。
/// 清冊匯入與畫面編修共用，確保兩者正規化結果一致。
pub fn resolve_version(
    product: &Product,
    version_raw: &str,
    asset_tag: &str,
    row: Option<RowRef>,
) -> (Option<String>, Option<VersionWarning>) {
    match product.scheme().parse(version_raw) {
        Some(parsed) => (Some(parsed.normalized), None),
        None => (
            None,
            Some(VersionWarning {
                row,
                asset_tag: asset_tag.to_string(),
                product_key: product.key.clone(),
                version_raw: version_raw.to_string(),
                scheme: product.version_scheme.clone(),
            }),
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionChange {
    pub product_key: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssetUpdate {
    pub asset_tag: String,
    pub changed_fields: Vec<&'static str>,
    /// 曝露度改變（影響風險等級，需要重新比對）
    pub exposure_changed: bool,
    pub version_changes: Vec<VersionChange>,
    pub components_added: Vec<String>,
    pub components_removed: Vec<String>,
    pub reactivated: bool,
}

/// 要寫入的一筆資產（含已辨識元件與未辨識寫法）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedAsset {
    pub record: AssetRecord,
    pub components: Vec<ResolvedComponent>,
    pub unresolved: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportPlan {
    pub assets: Vec<PlannedAsset>,
    pub added: Vec<String>,
    pub updated: Vec<AssetUpdate>,
    pub unchanged: usize,
    pub unresolved: Vec<UnresolvedItem>,
    pub disappeared: Vec<String>,
    pub warnings: Vec<VersionWarning>,
}

impl ImportPlan {
    /// 是否有影響比對的變更：新增資產、版本或元件增減、重新啟用、曝露度（影響風險等級）。
    /// 名稱、負責人、位置、備註、類別與重要性（只影響排序，查詢時讀取）不需要重新比對。
    pub fn affects_matching(&self) -> bool {
        !self.added.is_empty()
            || self.updated.iter().any(|u| {
                u.reactivated
                    || !u.version_changes.is_empty()
                    || !u.components_added.is_empty()
                    || !u.components_removed.is_empty()
                    || u.exposure_changed
            })
    }

    pub fn version_change_count(&self) -> usize {
        self.updated.iter().map(|u| u.version_changes.len()).sum()
    }
}

/// 資料庫中的現況（以 asset_tag 為鍵）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentAsset {
    pub id: i64,
    pub record: AssetRecord,
    pub status: AssetStatus,
    /// Excel 來源的產品元件：product_key → version_raw
    pub components: BTreeMap<String, String>,
}

pub type Snapshot = HashMap<String, CurrentAsset>;

pub fn plan_import(
    inventory: &Inventory,
    catalog: &Catalog,
    current: &Snapshot,
) -> Result<ImportPlan, CoreError> {
    let mut errors = Vec::new();
    let mut plan = ImportPlan::default();
    for incoming in &inventory.assets {
        let planned = resolve_asset(incoming, catalog, &mut plan, &mut errors);
        let tag = &planned.record.asset_tag;
        match current.get(tag) {
            None => plan.added.push(tag.clone()),
            Some(existing) => match diff_asset(existing, &planned) {
                Some(update) => plan.updated.push(update),
                None => plan.unchanged += 1,
            },
        }
        plan.assets.push(planned);
    }
    if !errors.is_empty() {
        return Err(CoreError::ImportRejected(errors));
    }
    let incoming_tags: HashSet<&str> = plan
        .assets
        .iter()
        .map(|a| a.record.asset_tag.as_str())
        .collect();
    let mut disappeared: Vec<String> = current
        .values()
        .filter(|a| a.status == AssetStatus::Active)
        .map(|a| a.record.asset_tag.clone())
        .filter(|tag| !incoming_tags.contains(tag.as_str()))
        .collect();
    disappeared.sort();
    plan.disappeared = disappeared;
    Ok(plan)
}

/// 以目錄辨識元件；未辨識者記錄建議，重複的產品視為列錯誤。
fn resolve_asset(
    incoming: &IncomingAsset,
    catalog: &Catalog,
    plan: &mut ImportPlan,
    errors: &mut Vec<String>,
) -> PlannedAsset {
    let tag = &incoming.record.asset_tag;
    let mut components: Vec<ResolvedComponent> = Vec::new();
    let mut unresolved = Vec::new();
    for c in &incoming.components {
        let Some(product) = catalog.resolve(&c.product_raw) else {
            plan.unresolved.push(UnresolvedItem {
                row: c.row,
                asset_tag: tag.clone(),
                product_raw: c.product_raw.clone(),
                version_raw: c.version_raw.clone(),
                suggestions: catalog
                    .suggest(&c.product_raw)
                    .into_iter()
                    .map(String::from)
                    .collect(),
            });
            unresolved.push((c.product_raw.clone(), c.version_raw.clone()));
            continue;
        };
        if let Some(first) = components.iter().find(|r| r.product_key == product.key) {
            errors.push(format!(
                "{}：{} 的產品「{}」與{}重複（皆為 {}）",
                c.row, tag, c.product_raw, first.row, product.key
            ));
            continue;
        }
        let (version_norm, warning) = resolve_version(product, &c.version_raw, tag, Some(c.row));
        plan.warnings.extend(warning);
        components.push(ResolvedComponent {
            row: c.row,
            product_key: product.key.clone(),
            version_raw: c.version_raw.clone(),
            version_norm,
        });
    }
    PlannedAsset {
        record: incoming.record.clone(),
        components,
        unresolved,
    }
}

fn changed_fields(old: &AssetRecord, new: &AssetRecord) -> Vec<&'static str> {
    let checks: [(&'static str, bool); 7] = [
        ("name", old.name != new.name),
        ("category", old.category != new.category),
        ("location", old.location != new.location),
        ("owner", old.owner != new.owner),
        ("exposure", old.exposure != new.exposure),
        ("criticality", old.criticality != new.criticality),
        ("notes", old.notes != new.notes),
    ];
    checks
        .into_iter()
        .filter(|(_, changed)| *changed)
        .map(|(name, _)| name)
        .collect()
}

fn diff_asset(existing: &CurrentAsset, planned: &PlannedAsset) -> Option<AssetUpdate> {
    let incoming: BTreeMap<&str, &str> = planned
        .components
        .iter()
        .map(|c| (c.product_key.as_str(), c.version_raw.as_str()))
        .collect();
    let version_changes = incoming
        .iter()
        .filter_map(|(key, version)| {
            let old = existing.components.get(*key)?;
            (old != version).then(|| VersionChange {
                product_key: key.to_string(),
                from: old.clone(),
                to: version.to_string(),
            })
        })
        .collect();
    let update = AssetUpdate {
        asset_tag: planned.record.asset_tag.clone(),
        changed_fields: changed_fields(&existing.record, &planned.record),
        exposure_changed: existing.record.exposure != planned.record.exposure,
        version_changes,
        components_added: incoming
            .keys()
            .filter(|k| !existing.components.contains_key(**k))
            .map(|k| k.to_string())
            .collect(),
        components_removed: existing
            .components
            .keys()
            .filter(|k| !incoming.contains_key(k.as_str()))
            .cloned()
            .collect(),
        reactivated: existing.status == AssetStatus::Retired,
    };
    let changed = !update.changed_fields.is_empty()
        || !update.version_changes.is_empty()
        || !update.components_added.is_empty()
        || !update.components_removed.is_empty()
        || update.reactivated;
    changed.then_some(update)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::columns::SheetKind;

    #[test]
    fn resolve_version_warns_like_import() {
        let catalog = Catalog::seed().unwrap();
        let esxi = catalog.get("vmware-esxi").unwrap();
        let (norm, warning) = resolve_version(esxi, "8.0 latest", "R6515-01", None);
        assert_eq!(norm, None);
        let w = warning.expect("warning");
        assert_eq!(w.row, None);
        assert_eq!(
            w.describe(),
            "R6515-01 vmware-esxi「8.0 latest」不符合 esxi 格式"
        );

        let row = RowRef {
            sheet: SheetKind::Assets,
            number: 3,
        };
        let (norm, warning) = resolve_version(esxi, "8.0 U3c", "R6515-01", Some(row));
        assert!(norm.is_some());
        assert!(warning.is_none());
    }
}
