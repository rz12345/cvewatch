//! 資產編輯表單（design D9）：UI 無關的表單狀態、本地驗證與目錄產品選單資料。
//!
//! 送出前以與 core 相同的 [`validate_record`] 檢查欄位，並檢查重複產品與「至少一個元件」，
//! 讓常見錯誤在任何寫入前就被擋下；錯誤以欄位（[`StandardField`]）或元件列索引定位。
//! core 的編修操作仍會在交易內再檢查一次。

use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::validate::{validate_record, RecordInput};
use cvewatch_core::catalog::Catalog;

use crate::data::AssetEntry;

/// 新增或編輯既有資產。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormMode {
    New,
    /// 編輯 `asset_tag` 這個資產（不可修改編號）
    Edit,
}

/// 元件列（既有元件或新加入的目錄產品）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentDraft {
    /// 既有元件的 id；新加入時為 None
    pub id: Option<i64>,
    /// 目錄產品 key，套件元件為 purl
    pub product_key: String,
    pub version: String,
    /// 編輯前的版本（既有元件）
    pub original_version: Option<String>,
    /// 主機套件元件：唯讀（由套件清單管理）
    pub package: bool,
    /// 已按「移除」（儲存時才寫入）
    pub removed: bool,
}

impl ComponentDraft {
    pub fn new(product_key: &str) -> ComponentDraft {
        ComponentDraft {
            id: None,
            product_key: product_key.to_string(),
            version: String::new(),
            original_version: None,
            package: false,
            removed: false,
        }
    }

    /// 儲存後仍存在的清冊元件（不含套件與已移除者）。
    fn is_kept_product(&self) -> bool {
        !self.package && !self.removed
    }
}

/// 未辨識產品的處置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnresolvedAction {
    Keep,
    /// 改為此目錄產品（沿用原版本字串）
    MapTo(String),
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedDraft {
    /// `asset_unresolved.id`（同一資產可有多筆相同寫法）
    pub id: i64,
    pub product_raw: String,
    pub version_raw: String,
    pub action: UnresolvedAction,
}

/// 資產欄位與元件的表單狀態（輸入框的原始字串）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetForm {
    pub mode: FormMode,
    pub asset_tag: String,
    pub name: String,
    pub category: String,
    pub exposure: String,
    pub criticality: String,
    pub location: String,
    pub owner: String,
    pub notes: String,
    pub components: Vec<ComponentDraft>,
    pub unresolved: Vec<UnresolvedDraft>,
}

/// 新增時的預設值（與清冊匯入相同：曝露度 internal、重要性 2）。
const DEFAULT_EXPOSURE: &str = "internal";
const DEFAULT_CRITICALITY: &str = "2";

impl Default for AssetForm {
    fn default() -> Self {
        AssetForm::new()
    }
}

impl AssetForm {
    /// 空白的新增表單。
    pub fn new() -> AssetForm {
        AssetForm {
            mode: FormMode::New,
            asset_tag: String::new(),
            name: String::new(),
            category: String::new(),
            exposure: DEFAULT_EXPOSURE.into(),
            criticality: DEFAULT_CRITICALITY.into(),
            location: String::new(),
            owner: String::new(),
            notes: String::new(),
            components: Vec::new(),
            unresolved: Vec::new(),
        }
    }

    /// 以資料庫目前的資產建立編輯表單。
    pub fn from_entry(entry: &AssetEntry) -> AssetForm {
        let r = &entry.asset.record;
        let text = |v: &Option<String>| v.clone().unwrap_or_default();
        AssetForm {
            mode: FormMode::Edit,
            asset_tag: r.asset_tag.clone(),
            name: r.name.clone(),
            category: r.category.as_str().into(),
            exposure: r.exposure.as_str().into(),
            criticality: r.criticality.to_string(),
            location: text(&r.location),
            owner: text(&r.owner),
            notes: text(&r.notes),
            components: entry
                .components
                .iter()
                .map(|c| ComponentDraft {
                    id: Some(c.id),
                    product_key: c.label().to_string(),
                    version: c.version_raw.clone(),
                    original_version: Some(c.version_raw.clone()),
                    package: !c.is_inventory_product(),
                    removed: false,
                })
                .collect(),
            unresolved: entry
                .unresolved
                .iter()
                .map(|u| UnresolvedDraft {
                    id: u.id,
                    product_raw: u.product_raw.clone(),
                    version_raw: u.version_raw.clone(),
                    action: UnresolvedAction::Keep,
                })
                .collect(),
        }
    }

    /// 欄位的原始輸入（交給 core 驗證）。
    pub fn record_input(&self) -> RecordInput<'_> {
        RecordInput {
            asset_tag: &self.asset_tag,
            name: &self.name,
            category: &self.category,
            exposure: &self.exposure,
            criticality: &self.criticality,
            location: &self.location,
            owner: &self.owner,
            notes: &self.notes,
        }
    }

    /// 加入一個目錄產品元件列（版本待填）。
    pub fn add_product(&mut self, key: &str) {
        self.components.push(ComponentDraft::new(key));
    }

    /// 本地驗證（不需資料庫）。
    pub fn validate(&self) -> Result<(), FormErrors> {
        let mut errors = FormErrors::default();
        if let Err(fields) = validate_record(&self.record_input()) {
            errors.fields = fields.into_iter().map(|e| (e.field, e.message)).collect();
        }
        let mut seen: Vec<&str> = Vec::new();
        for (i, c) in self.components.iter().enumerate() {
            if !c.is_kept_product() {
                continue;
            }
            if c.version.trim().is_empty() {
                errors.components.push((i, "缺少 version".into()));
            }
            if seen.contains(&c.product_key.as_str()) {
                errors
                    .components
                    .push((i, format!("已有產品 {}，請改為修改版本", c.product_key)));
            }
            seen.push(&c.product_key);
        }
        for (i, u) in self.unresolved.iter().enumerate() {
            if let UnresolvedAction::MapTo(key) = &u.action {
                if seen.contains(&key.as_str()) {
                    errors
                        .unresolved
                        .push((i, format!("已有產品 {key}，請刪除此筆或改選其他產品")));
                }
                seen.push(key);
            }
        }
        let kept_unresolved = self
            .unresolved
            .iter()
            .any(|u| u.action != UnresolvedAction::Delete);
        if seen.is_empty() && !kept_unresolved {
            errors
                .general
                .push("至少需要一個元件（產品與版本）；不再使用此資產時請改為刪除資產".into());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// 本地驗證錯誤，依位置分類。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormErrors {
    pub fields: Vec<(StandardField, String)>,
    /// (元件列索引, 訊息)
    pub components: Vec<(usize, String)>,
    /// (未辨識產品列索引, 訊息)
    pub unresolved: Vec<(usize, String)>,
    /// 不屬於任何欄位的錯誤（例如沒有任何元件）
    pub general: Vec<String>,
}

impl FormErrors {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
            && self.components.is_empty()
            && self.unresolved.is_empty()
            && self.general.is_empty()
    }

    pub fn field(&self, field: StandardField) -> Option<&str> {
        self.fields
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, m)| m.as_str())
    }

    pub fn component(&self, index: usize) -> Vec<&str> {
        self.components
            .iter()
            .filter(|(i, _)| *i == index)
            .map(|(_, m)| m.as_str())
            .collect()
    }

    pub fn unresolved_row(&self, index: usize) -> Vec<&str> {
        self.unresolved
            .iter()
            .filter(|(i, _)| *i == index)
            .map(|(_, m)| m.as_str())
            .collect()
    }

    /// 全部訊息（狀態列摘要用）。
    pub fn summary(&self) -> String {
        self.fields
            .iter()
            .map(|(_, m)| m.as_str())
            .chain(self.components.iter().map(|(_, m)| m.as_str()))
            .chain(self.unresolved.iter().map(|(_, m)| m.as_str()))
            .chain(self.general.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("；")
    }
}

/// 目錄產品選單的一個選項。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductChoice {
    pub key: String,
    pub display_name: String,
    pub vendor_display: String,
}

impl ProductChoice {
    /// 選單顯示文字。
    pub fn label(&self) -> String {
        format!("{}（{}）", self.display_name, self.vendor_display)
    }
}

/// 目錄中的全部產品（依目錄順序）。
pub fn product_choices(catalog: &Catalog) -> Vec<ProductChoice> {
    search_products(catalog, "")
}

/// 以 key、display_name 或別名搜尋（不分大小寫、部分相符）；空字串列出全部。
pub fn search_products(catalog: &Catalog, query: &str) -> Vec<ProductChoice> {
    let q = query.trim().to_lowercase();
    catalog
        .products()
        .iter()
        .filter(|p| {
            q.is_empty()
                || std::iter::once(&p.key)
                    .chain(std::iter::once(&p.display_name))
                    .chain(p.aliases.iter())
                    .any(|name| name.to_lowercase().contains(&q))
        })
        .map(|p| ProductChoice {
            key: p.key.clone(),
            display_name: p.display_name.clone(),
            vendor_display: p.vendor_display.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled() -> AssetForm {
        let mut f = AssetForm::new();
        f.asset_tag = "FW-02".into();
        f.name = "fw02".into();
        f.category = "security".into();
        f.add_product("paloalto-panos");
        f.components[0].version = "11.2.3".into();
        f
    }

    #[test]
    fn complete_new_form_is_valid() {
        assert_eq!(filled().validate(), Ok(()));
    }

    #[test]
    fn empty_asset_tag_is_reported_on_its_field() {
        let mut f = filled();
        f.asset_tag.clear();
        let e = f.validate().unwrap_err();
        assert_eq!(e.field(StandardField::AssetTag), Some("缺少 asset_tag"));
        assert!(e.components.is_empty() && e.general.is_empty());
    }

    #[test]
    fn duplicate_product_is_reported_on_its_row() {
        let mut f = filled();
        f.add_product("paloalto-panos");
        f.components[1].version = "11.2.4".into();
        let e = f.validate().unwrap_err();
        assert_eq!(e.components.len(), 1);
        assert_eq!(e.components[0].0, 1);
        assert!(e.component(1)[0].contains("已有產品"), "{e:?}");
        // 移除重複的那列後通過
        f.components[1].removed = true;
        assert_eq!(f.validate(), Ok(()));
    }

    #[test]
    fn missing_version_and_no_component() {
        let mut f = filled();
        f.components[0].version = " ".into();
        assert_eq!(f.validate().unwrap_err().component(0), vec!["缺少 version"]);
        f.components[0].removed = true;
        let e = f.validate().unwrap_err();
        assert!(e.general[0].contains("至少需要一個元件"), "{e:?}");
    }

    #[test]
    fn unresolved_kept_counts_as_a_component_and_mapping_checks_duplicates() {
        let mut f = filled();
        f.components[0].removed = true;
        f.unresolved.push(UnresolvedDraft {
            id: 1,
            product_raw: "PVE 叢集".into(),
            version_raw: "8.2.4".into(),
            action: UnresolvedAction::Keep,
        });
        assert_eq!(f.validate(), Ok(()));
        f.components[0].removed = false;
        f.unresolved[0].action = UnresolvedAction::MapTo("paloalto-panos".into());
        assert_eq!(f.validate().unwrap_err().unresolved_row(0).len(), 1);
    }

    #[test]
    fn search_matches_display_name_and_aliases_case_insensitively() {
        let catalog = Catalog::seed().unwrap();
        let keys = |q: &str| -> Vec<String> {
            search_products(&catalog, q)
                .into_iter()
                .map(|p| p.key)
                .collect()
        };
        assert!(keys("pan").contains(&"paloalto-panos".to_string()));
        assert!(keys("ESX").contains(&"vmware-esxi".to_string()));
        assert!(keys("pve").contains(&"proxmox-ve".to_string()), "別名");
        assert!(keys("no-such-thing").is_empty());
        assert_eq!(product_choices(&catalog).len(), catalog.products().len());
        let panos = &search_products(&catalog, "PAN-OS")[0];
        assert_eq!(panos.label(), "PAN-OS（Palo Alto Networks）");
    }
}
