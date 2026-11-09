//! 匯入差異計算（純函式）：新增、更新、版本變更、未辨識、清冊中消失。

use std::collections::{BTreeMap, HashMap, HashSet};

use super::columns::StandardField;
use super::model::{is_pending_version, AssetRecord, AssetStatus};
use super::plan_fields::changed_fields;
use super::plan_systems::{assign_system, keep_existing, kept_columns_notice, plan_systems};
use super::systems::SystemDirectory;
use super::validate::{ImportNotice, IncomingAsset, Inventory, RowRef, ISMS_KEPT_NOTICE};

pub use super::plan_fields::FieldChange;
pub use super::plan_systems::SystemChange;
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

/// 版本待補在預覽與訊息中的寫法（18 design D3）。
pub const PENDING_LABEL: &str = "待補";

/// 預覽中的版本：待補以 [`PENDING_LABEL`] 呈現。
pub fn version_label(version_raw: &str) -> &str {
    version_or(version_raw, PENDING_LABEL)
}

/// 版本待補時改為 `pending`（CLI 預覽用「待補」，畫面用「版本待補」）。
pub fn version_or<'a>(version_raw: &'a str, pending: &'a str) -> &'a str {
    if is_pending_version(version_raw) {
        pending
    } else {
        version_raw
    }
}

/// 以目錄產品的版本格式解析版本字串：回傳 `version_norm` 與無法解析時的警告。
/// 清冊匯入與畫面編修共用，確保兩者正規化結果一致。版本待補不解析、不產生警告。
pub fn resolve_version(
    product: &Product,
    version_raw: &str,
    asset_tag: &str,
    row: Option<RowRef>,
) -> (Option<String>, Option<VersionWarning>) {
    if is_pending_version(version_raw) {
        return (None, None);
    }
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
    /// 空字串為版本待補
    pub from: String,
    /// 空字串為版本待補
    pub to: String,
}

impl VersionChange {
    /// 「11.1.6 → 待補」「待補 → 7.2.2-72806」。
    pub fn describe(&self) -> String {
        format!(
            "{} → {}",
            version_label(&self.from),
            version_label(&self.to)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssetUpdate {
    pub asset_tag: String,
    /// 變更的欄位與新舊值（18 design D5）
    pub changed_fields: Vec<FieldChange>,
    /// 曝露度改變（影響風險等級，需要重新比對）
    pub exposure_changed: bool,
    pub version_changes: Vec<VersionChange>,
    pub components_added: Vec<String>,
    pub components_removed: Vec<String>,
    pub reactivated: bool,
}

impl AssetUpdate {
    /// 變更的標準欄名（一般清冊匯入的預覽）。
    pub fn field_names(&self) -> Vec<&'static str> {
        self.changed_fields.iter().map(FieldChange::name).collect()
    }
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
    /// `systems` 工作表造成的資產說明新增與改名（16 design D5）
    pub systems: Vec<SystemChange>,
    /// 不中止匯入的提醒（舊版 category 值、停用欄位；13 design D3），在版本警告之後列出
    pub notices: Vec<ImportNotice>,
}

impl ImportPlan {
    /// 是否有影響比對的變更：新增資產、版本或元件增減、重新啟用、曝露度（影響風險等級）。
    /// 名稱、位置、備註、類別、C/I/A（資產價值只影響排序，查詢時讀取）、ISMS 編號、
    /// 資產說明與 ISMS 管理欄位不需要重新比對。
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

    /// 匯入後檔案中版本待補的目錄產品元件數（未辨識產品不計；18 design D3）。
    pub fn pending_components(&self) -> usize {
        self.assets
            .iter()
            .flat_map(|a| &a.components)
            .filter(|c| is_pending_version(&c.version_raw))
            .count()
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

/// `systems` 為資料庫中的資產說明；`systems` 工作表的內容合併後用於解析各資產的資產說明。
pub fn plan_import(
    inventory: &Inventory,
    catalog: &Catalog,
    current: &Snapshot,
    systems: &SystemDirectory,
) -> Result<ImportPlan, CoreError> {
    let mut errors = Vec::new();
    let (system_changes, merged) = plan_systems(&inventory.systems, systems, &mut errors);
    let mut plan = ImportPlan {
        notices: inventory.notices.clone(),
        systems: system_changes,
        ..ImportPlan::default()
    };
    let folded = folded_tags(current);
    let isms_owners = isms_owners(inventory, current);
    let mut kept_isms = false;
    let mut any_existing = false;
    for incoming in &inventory.assets {
        let mut planned = resolve_asset(incoming, catalog, &mut plan, &mut errors);
        let existing = current.get(&planned.record.asset_tag);
        if let Some(existing) = existing {
            any_existing = true;
            kept_isms |=
                inventory.keeps(StandardField::IsmsId) && existing.record.isms_id.is_some();
            planned.record = keep_existing(&planned.record, &existing.record, inventory);
        }
        match assign_system(
            incoming,
            planned.record.clone(),
            existing,
            inventory,
            &merged,
        ) {
            Ok(record) => planned.record = record,
            Err(message) => errors.push(message),
        }
        let tag = &planned.record.asset_tag;
        if let Some(message) = isms_conflict(&planned.record, &isms_owners) {
            errors.push(format!("{}：{message}", incoming.row));
        }
        match current.get(tag) {
            None => match folded.get(&tag.to_ascii_lowercase()) {
                // 編號在資料庫區分大小寫，但新增與更名不分大小寫檢查重複（12 code review）
                Some(existing) => errors.push(format!(
                    "{}：asset_tag「{tag}」與既有資產 {existing} 只差大小寫；請改用相同寫法，                     或以 cvewatch asset rename 更名",
                    incoming.row
                )),
                None => plan.added.push(tag.clone()),
            },
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
    if kept_isms {
        plan.notices.push(ImportNotice {
            row: None,
            asset_tag: None,
            message: ISMS_KEPT_NOTICE.to_string(),
        });
    }
    plan.notices
        .extend(kept_columns_notice(inventory, any_existing));
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

/// 匯入後仍保留 ISMS 編號的既有資產（不在這次清冊中者，含 retired）：
/// 編號（不分大小寫）→（asset_tag, 是否退役）。清冊中的資產會改用清冊的編號，不列入。
fn isms_owners(inventory: &Inventory, current: &Snapshot) -> HashMap<String, (String, bool)> {
    let incoming: HashSet<&str> = inventory
        .assets
        .iter()
        .map(|a| a.record.asset_tag.as_str())
        .collect();
    current
        .values()
        .filter(|a| !incoming.contains(a.record.asset_tag.as_str()))
        .filter_map(|a| {
            let id = a.record.isms_id.as_ref()?;
            Some((
                id.to_ascii_lowercase(),
                (a.record.asset_tag.clone(), a.status == AssetStatus::Retired),
            ))
        })
        .collect()
}

/// ISMS 編號已被清冊以外的既有資產使用（含 retired，不分大小寫）時的錯誤訊息。
fn isms_conflict(record: &AssetRecord, owners: &HashMap<String, (String, bool)>) -> Option<String> {
    let id = record.isms_id.as_ref()?;
    let (owner, retired) = owners.get(&id.to_ascii_lowercase())?;
    Some(isms_in_use_message(id, owner, *retired))
}

/// 「ISMS 編號 sys-hw-005 已被 NAS-02 使用（該資產已退役）」（匯入與畫面編修共用）。
pub fn isms_in_use_message(isms_id: &str, owner: &str, retired: bool) -> String {
    let status = if retired {
        "（該資產已退役）"
    } else {
        ""
    };
    format!("ISMS 編號 {isms_id} 已被 {owner} 使用{status}")
}

/// 既有資產編號（不分 ASCII 大小寫，與 SQLite `COLLATE NOCASE` 相同）→ 實際寫法。
fn folded_tags(current: &Snapshot) -> HashMap<String, &str> {
    current
        .keys()
        .map(|tag| (tag.to_ascii_lowercase(), tag.as_str()))
        .collect()
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
        let (norm, warning) = resolve_version(esxi, "8.0 latest", "SRV-01", None);
        assert_eq!(norm, None);
        let w = warning.expect("warning");
        assert_eq!(w.row, None);
        assert_eq!(
            w.describe(),
            "SRV-01 vmware-esxi「8.0 latest」不符合 esxi 格式"
        );

        let row = RowRef {
            sheet: SheetKind::Assets,
            number: 3,
        };
        let (norm, warning) = resolve_version(esxi, "8.0 U3c", "SRV-01", Some(row));
        assert!(norm.is_some());
        assert!(warning.is_none());
    }

    #[test]
    fn pending_version_is_not_parsed() {
        let catalog = Catalog::seed().unwrap();
        let esxi = catalog.get("vmware-esxi").unwrap();
        assert_eq!(resolve_version(esxi, "  ", "SRV-01", None), (None, None));
    }

    #[test]
    fn pending_side_of_a_version_change_reads_pending() {
        let cleared = VersionChange {
            product_key: "paloalto-panos".into(),
            from: "11.1.6".into(),
            to: String::new(),
        };
        assert_eq!(cleared.describe(), "11.1.6 → 待補");
        let filled = VersionChange {
            product_key: "synology-dsm".into(),
            from: String::new(),
            to: "7.2.2-72806".into(),
        };
        assert_eq!(filled.describe(), "待補 → 7.2.2-72806");
    }
}
