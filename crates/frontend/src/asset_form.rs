//! 資產編輯表單（design D9）：UI 無關的表單狀態、本地驗證與目錄產品選單資料。
//!
//! 送出前以與 core 相同的 [`validate_record`] 檢查欄位，並檢查重複產品與「至少一個元件」，
//! 讓常見錯誤在任何寫入前就被擋下；錯誤以欄位（[`StandardField`]）或元件列索引定位。
//! core 的編修操作仍會在交易內再檢查一次。

use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::validate::{validate_record, FieldErrorKind, RecordInput};
use cvewatch_core::assets::{MAX_CRITICALITY, MIN_CRITICALITY};
use cvewatch_core::catalog::Catalog;

use crate::data::AssetEntry;
use crate::display::product_name;

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
    /// 目錄產品 key；套件元件為不含版本的 purl（顯示名稱見 [`ComponentDraft::name`]）
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

    /// 顯示名稱（目錄產品的 display_name；套件為 purl）。
    pub fn name(&self, catalog: &Catalog) -> String {
        product_name(catalog, &self.product_key)
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
                    product_key: c.key().to_string(),
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

    /// 本地驗證（不需資料庫）。錯誤以介面用語回報（design 11 D2）：不出現清冊的英文欄位名，
    /// 產品以目錄的顯示名稱呈現。
    pub fn validate(&self, catalog: &Catalog) -> Result<(), FormErrors> {
        let mut errors = FormErrors::default();
        if let Err(fields) = validate_record(&self.record_input()) {
            errors.fields = fields
                .into_iter()
                .map(|e| (e.field, field_message(e.kind)))
                .collect();
        }
        let mut seen: Vec<&str> = Vec::new();
        for (i, c) in self.components.iter().enumerate() {
            if !c.is_kept_product() {
                continue;
            }
            if c.version.trim().is_empty() {
                errors.components.push((i, VERSION_REQUIRED.into()));
            }
            if seen.contains(&c.product_key.as_str()) {
                let name = product_name(catalog, &c.product_key);
                errors
                    .components
                    .push((i, format!("已有產品 {name}，請改為修改版本")));
            }
            seen.push(&c.product_key);
        }
        for (i, u) in self.unresolved.iter().enumerate() {
            if let UnresolvedAction::MapTo(key) = &u.action {
                if seen.contains(&key.as_str()) {
                    let name = product_name(catalog, key);
                    errors
                        .unresolved
                        .push((i, format!("已有產品 {name}，請刪除此筆或改選其他產品")));
                }
                seen.push(key);
            }
        }
        let kept_unresolved = self
            .unresolved
            .iter()
            .any(|u| u.action != UnresolvedAction::Delete);
        if seen.is_empty() && !kept_unresolved {
            errors.general.push(COMPONENT_REQUIRED.into());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// 欄位的介面名稱（對話框標籤與錯誤摘要用；清冊與 CLI 仍用英文欄名）。
pub fn field_label(field: StandardField) -> &'static str {
    match field {
        StandardField::AssetTag => "資產編號",
        StandardField::Name => "名稱",
        StandardField::Category => "類別",
        StandardField::Exposure => "曝露度",
        StandardField::Criticality => "重要性",
        StandardField::Location => "位置",
        StandardField::Owner => "負責人",
        StandardField::Notes => "備註",
        StandardField::Product => "產品",
        StandardField::Version => "版本",
    }
}

/// core 的欄位錯誤種類轉為介面用語（不解析 core 的訊息字串；core 與 CLI 的文字不變）。
fn field_message(kind: FieldErrorKind) -> String {
    match kind {
        FieldErrorKind::Missing => REQUIRED.into(),
        FieldErrorKind::NotAllowed => CHOOSE_FROM_LIST.into(),
        FieldErrorKind::OutOfRange => {
            format!("重要性只能是 {MIN_CRITICALITY}–{MAX_CRITICALITY}")
        }
    }
}

/// 必填欄位留空。
pub const REQUIRED: &str = "必填";
/// 下拉選單的值不在允許清單中。
pub const CHOOSE_FROM_LIST: &str = "請從清單選擇";
/// 元件沒有版本。
pub const VERSION_REQUIRED: &str = "請填寫版本";
/// 沒有任何元件。
pub const COMPONENT_REQUIRED: &str =
    "至少需要一個元件（產品與版本）；不再使用此資產時請改用「退役…」";

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

    /// 全部訊息（不在對話框中顯示時的摘要；欄位錯誤前加上欄位名稱）。
    pub fn summary(&self) -> String {
        self.fields
            .iter()
            .map(|(f, m)| format!("{}：{m}", field_label(*f)))
            .chain(self.components.iter().map(|(_, m)| m.clone()))
            .chain(self.unresolved.iter().map(|(_, m)| m.clone()))
            .chain(self.general.iter().cloned())
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

    fn catalog() -> Catalog {
        Catalog::seed().unwrap()
    }

    fn check(f: &AssetForm) -> Result<(), FormErrors> {
        f.validate(&catalog())
    }

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
        assert_eq!(check(&filled()), Ok(()));
    }

    #[test]
    fn empty_asset_tag_is_reported_on_its_field() {
        let mut f = filled();
        f.asset_tag.clear();
        let e = check(&f).unwrap_err();
        assert_eq!(e.field(StandardField::AssetTag), Some(REQUIRED));
        assert!(e.components.is_empty() && e.general.is_empty());
    }

    #[test]
    fn errors_use_interface_wording() {
        let mut f = AssetForm::new();
        f.category = "router".into();
        f.exposure = "public".into();
        f.criticality = "5".into();
        f.add_product("paloalto-panos");
        f.add_product("paloalto-panos");
        let e = check(&f).unwrap_err();
        use StandardField as F;
        assert_eq!(e.field(F::AssetTag), Some(REQUIRED));
        assert_eq!(e.field(F::Name), Some(REQUIRED));
        assert_eq!(e.field(F::Category), Some(CHOOSE_FROM_LIST));
        assert_eq!(e.field(F::Exposure), Some(CHOOSE_FROM_LIST));
        assert_eq!(e.field(F::Criticality), Some("重要性只能是 1–3"));
        assert_eq!(e.component(0), vec![VERSION_REQUIRED]);
        assert!(e.component(1).contains(&"已有產品 PAN-OS，請改為修改版本"));
        f.category.clear();
        let e = check(&f).unwrap_err();
        assert_eq!(e.field(F::Category), Some(REQUIRED));
        f.components.clear();
        let all = format!("{}；{}", e.summary(), check(&f).unwrap_err().summary());
        for internal in [
            "asset_tag",
            "name",
            "category",
            "exposure",
            "criticality",
            "paloalto-panos",
            "version",
        ] {
            assert!(!all.contains(internal), "{internal}: {all}");
        }
    }

    #[test]
    fn duplicate_product_is_reported_on_its_row() {
        let mut f = filled();
        f.add_product("paloalto-panos");
        f.components[1].version = "11.2.4".into();
        let e = check(&f).unwrap_err();
        assert_eq!(e.components.len(), 1);
        assert_eq!(e.components[0].0, 1);
        assert!(e.component(1)[0].contains("已有產品"), "{e:?}");
        // 移除重複的那列後通過
        f.components[1].removed = true;
        assert_eq!(check(&f), Ok(()));
    }

    #[test]
    fn missing_version_and_no_component() {
        let mut f = filled();
        f.components[0].version = " ".into();
        assert_eq!(check(&f).unwrap_err().component(0), vec![VERSION_REQUIRED]);
        f.components[0].removed = true;
        let e = check(&f).unwrap_err();
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
        assert_eq!(check(&f), Ok(()));
        f.components[0].removed = false;
        f.unresolved[0].action = UnresolvedAction::MapTo("paloalto-panos".into());
        assert_eq!(
            check(&f).unwrap_err().unresolved_row(0),
            vec!["已有產品 PAN-OS，請刪除此筆或改選其他產品"]
        );
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
