//! 資產編輯表單（design D9）：UI 無關的表單狀態、本地驗證與目錄產品選單資料。
//!
//! 送出前以與 core 相同的 [`validate_record`] 檢查欄位，並檢查重複產品與「至少一個元件」，
//! 讓常見錯誤在任何寫入前就被擋下；錯誤以欄位（[`StandardField`]）或元件列索引定位。
//! core 的編修操作仍會在交易內再檢查一次。ISMS 管理欄位（資產說明、防護等級、個資）見
//! [`admin`]。

mod admin;
mod conflicts;
mod errors;
mod identity;
mod lifecycle;

pub use conflicts::{isms_conflict, rename_conflict};

pub use errors::{
    field_label, FormErrors, CHOOSE_FROM_LIST, COMPONENT_REQUIRED, ISMS_CLASS_MISMATCH,
    ISMS_FORMAT, REQUIRED, VERSION_REQUIRED,
};

use errors::field_message;

pub use identity::{
    hostname_too_long, property_number_conflict, IpDraft, HOSTNAME_FORMAT, IP_INVALID, IP_REQUIRED,
    IP_SINGLE_ADDRESS, MANAGEMENT_URL_FORMAT, URL_CREDENTIALS,
};

pub use lifecycle::{
    override_texts, EolDraft, LifecycleDraft, DATE_INVALID, EOL_ERROR_PREFIX,
    OVERRIDE_DATE_REQUIRED, OVERRIDE_SOURCE_REQUIRED,
};

pub use admin::{
    personal_data_label, rated_text, ISMS_SYSTEM_MISMATCH, ISMS_SYSTEM_UNKNOWN, PERSONAL_DATA_NO,
    PERSONAL_DATA_YES, UNASSIGNED_SYSTEM, UNRATED,
};

use cvewatch_core::assets::classes::{MAX_RATING, MIN_RATING};
use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::validate::{validate_record, RecordContext, RecordInput, RATING_FIELDS};
use cvewatch_core::assets::{is_pending_version, AssetClass, AssetClasses, Cia, SystemDirectory};
use cvewatch_core::catalog::Catalog;

use crate::data::AssetEntry;
use crate::display::product_name;

/// 新增或編輯既有資產。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormMode {
    New,
    /// 編輯 `original_tag` 這個資產；表單的 `asset_tag` 與它不同時表示更名（12 design D3）
    Edit {
        original_tag: String,
    },
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
    /// 軟體 EoL 覆寫（22 design D11；套件元件不使用）
    pub eol: EolDraft,
}

impl ComponentDraft {
    /// 既有的版本待補元件、版本仍空白（未修改；儲存時不必填，18 design D8）。
    pub fn is_untouched_pending(&self) -> bool {
        self.id.is_some()
            && self
                .original_version
                .as_deref()
                .is_some_and(is_pending_version)
            && is_pending_version(&self.version)
    }

    pub fn new(product_key: &str) -> ComponentDraft {
        ComponentDraft {
            id: None,
            product_key: product_key.to_string(),
            version: String::new(),
            original_version: None,
            package: false,
            removed: false,
            eol: EolDraft::default(),
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
    /// 子類別代碼
    pub category: String,
    pub exposure: String,
    /// C/I/A：空字串 = 依類別（範本值）
    pub confidentiality: String,
    pub integrity: String,
    pub availability: String,
    pub isms_id: String,
    /// 資產說明代碼：空字串 = 未歸屬（有 ISMS 編號時由其系統代碼帶出）
    pub system: String,
    pub responsible_unit: String,
    pub custodian: String,
    pub user_unit: String,
    /// 法規遵循性：空字串 = 未評定
    pub compliance: String,
    /// 是否含個人資料：空字串 = 依類別，`Y` 或 `N`
    pub personal_data: String,
    /// 識別欄位（20 design D10）
    pub brand: String,
    pub model: String,
    pub serial_number: String,
    pub property_number: String,
    pub hostname: String,
    pub management_url: String,
    pub location: String,
    pub notes: String,
    pub components: Vec<ComponentDraft>,
    pub unresolved: Vec<UnresolvedDraft>,
    /// IP 列（完全空白的列儲存時忽略）
    pub ips: Vec<IpDraft>,
    /// 生命週期日期與硬體 EoL 覆寫（22 design D11）
    pub lifecycle: LifecycleDraft,
}

/// 新增時的預設值（與清冊匯入相同：曝露度 internal、C/I/A 依類別）。
const DEFAULT_EXPOSURE: &str = "internal";

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
            confidentiality: String::new(),
            integrity: String::new(),
            availability: String::new(),
            isms_id: String::new(),
            system: String::new(),
            responsible_unit: String::new(),
            custodian: String::new(),
            user_unit: String::new(),
            compliance: String::new(),
            personal_data: String::new(),
            brand: String::new(),
            model: String::new(),
            serial_number: String::new(),
            property_number: String::new(),
            hostname: String::new(),
            management_url: String::new(),
            location: String::new(),
            notes: String::new(),
            components: Vec::new(),
            unresolved: Vec::new(),
            ips: Vec::new(),
            lifecycle: LifecycleDraft::default(),
        }
    }

    /// 以資料庫目前的資產建立編輯表單。
    pub fn from_entry(entry: &AssetEntry) -> AssetForm {
        let r = &entry.asset.record;
        let text = |v: &Option<String>| v.clone().unwrap_or_default();
        AssetForm {
            mode: FormMode::Edit {
                original_tag: r.asset_tag.clone(),
            },
            asset_tag: r.asset_tag.clone(),
            name: r.name.clone(),
            category: r.category.as_str().into(),
            exposure: r.exposure.as_str().into(),
            // 既有資產帶入已設定的值：改類別不會改變 C/I/A（spec Changing class keeps ratings）
            confidentiality: r.cia.confidentiality.to_string(),
            integrity: r.cia.integrity.to_string(),
            availability: r.cia.availability.to_string(),
            isms_id: text(&r.isms_id),
            system: text(&r.system_code),
            responsible_unit: text(&r.responsible_unit),
            custodian: text(&r.custodian),
            user_unit: text(&r.user_unit),
            compliance: r.compliance.map(|d| d.to_string()).unwrap_or_default(),
            // 既有資產帶入已設定的值：改類別不會改變是否含個人資料
            personal_data: if r.personal_data {
                PERSONAL_DATA_YES
            } else {
                PERSONAL_DATA_NO
            }
            .into(),
            brand: text(&r.brand),
            model: text(&r.model),
            serial_number: text(&r.serial_number),
            property_number: text(&r.property_number),
            hostname: text(&r.hostname),
            management_url: text(&r.management_url),
            location: text(&r.location),
            notes: text(&r.notes),
            ips: entry.ips.iter().map(IpDraft::from_ip).collect(),
            lifecycle: LifecycleDraft::from_record(r),
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
                    eol: EolDraft::from_override(c.eol_override.as_ref()),
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

    /// 編輯模式下要更名的新編號（去除前後空白）：空白或與原編號完全相同時為 None。
    /// 只改大小寫也算更名（資料庫的編號區分大小寫）。
    pub fn renamed_to(&self) -> Option<&str> {
        let FormMode::Edit { original_tag } = &self.mode else {
            return None;
        };
        let tag = self.asset_tag.trim();
        (!tag.is_empty() && tag != original_tag).then_some(tag)
    }

    /// 編輯模式的原編號（新增模式為 None）。
    pub fn original_tag(&self) -> Option<&str> {
        match &self.mode {
            FormMode::Edit { original_tag } => Some(original_tag),
            FormMode::New => None,
        }
    }

    /// 欄位的原始輸入（交給 core 驗證）。
    pub fn record_input(&self) -> RecordInput<'_> {
        RecordInput {
            asset_tag: &self.asset_tag,
            name: &self.name,
            category: &self.category,
            exposure: &self.exposure,
            confidentiality: &self.confidentiality,
            integrity: &self.integrity,
            availability: &self.availability,
            isms_id: &self.isms_id,
            system: &self.system,
            responsible_unit: &self.responsible_unit,
            custodian: &self.custodian,
            user_unit: &self.user_unit,
            compliance: &self.compliance,
            personal_data: &self.personal_data,
            brand: &self.brand,
            model: &self.model,
            serial_number: &self.serial_number,
            property_number: &self.property_number,
            hostname: &self.hostname,
            management_url: &self.management_url,
            warranty_end: &self.lifecycle.warranty_end,
            support_end: &self.lifecycle.support_end,
            license_end: &self.lifecycle.license_end,
            location: &self.location,
            notes: &self.notes,
        }
    }

    /// C/I/A 欄位的輸入（`field` 不是 C/I/A 時為空字串）。
    pub fn rating(&self, field: StandardField) -> &str {
        match field {
            StandardField::Confidentiality => &self.confidentiality,
            StandardField::Integrity => &self.integrity,
            StandardField::Availability => &self.availability,
            _ => "",
        }
    }

    /// C/I/A 欄位的可寫參照（下拉選單用）；`field` 不是 C/I/A 時為 None。
    pub fn rating_mut(&mut self, field: StandardField) -> Option<&mut String> {
        match field {
            StandardField::Confidentiality => Some(&mut self.confidentiality),
            StandardField::Integrity => Some(&mut self.integrity),
            StandardField::Availability => Some(&mut self.availability),
            _ => None,
        }
    }

    /// 目前選擇的子類別（只接受已定義子類別的代碼或名稱，與畫面編修相同）。
    pub fn class<'c>(&self, classes: &'c AssetClasses) -> Option<&'c AssetClass> {
        let resolved = classes.resolve(&self.category, false)?;
        classes.get(&resolved.code)
    }

    /// 某個 C/I/A 欄位的實際值：有填時為填寫值，空白時為 `template` 中的值；
    /// 不是 1–4 或沒有範本值時為 None。`template` 為目前子類別的範本值（呼叫端解析一次）。
    pub fn effective_rating_with(&self, field: StandardField, template: Option<Cia>) -> Option<u8> {
        let raw = self.rating(field).trim();
        if raw.is_empty() {
            return template?.rating(field);
        }
        raw.parse::<u8>()
            .ok()
            .filter(|n| (MIN_RATING..=MAX_RATING).contains(n))
    }

    /// 與 [`effective_rating_with`](Self::effective_rating_with) 相同，範本值取自目前的子類別。
    pub fn effective_rating(&self, field: StandardField, classes: &AssetClasses) -> Option<u8> {
        self.effective_rating_with(field, self.class(classes).and_then(|c| c.template))
    }

    /// 即時的資產價值 MAX(C, I, A)；任何一項沒有值時為 None（畫面顯示「—」）。
    pub fn asset_value_with(&self, template: Option<Cia>) -> Option<u8> {
        let [c, i, a] = RATING_FIELDS.map(|(f, _)| self.effective_rating_with(f, template));
        Some(Cia::new(c?, i?, a?).value())
    }

    /// 與 [`asset_value_with`](Self::asset_value_with) 相同，範本值取自目前的子類別。
    pub fn asset_value(&self, classes: &AssetClasses) -> Option<u8> {
        self.asset_value_with(self.class(classes).and_then(|c| c.template))
    }

    /// 加入一個目錄產品元件列（版本待填）。
    pub fn add_product(&mut self, key: &str) {
        self.components.push(ComponentDraft::new(key));
    }

    /// 本地驗證（不需資料庫；資產說明取自畫面快照）。錯誤以介面用語回報（design 11 D2）：
    /// 不出現清冊的英文欄位名，產品以目錄的顯示名稱呈現。
    pub fn validate(
        &self,
        catalog: &Catalog,
        classes: &AssetClasses,
        systems: &SystemDirectory,
    ) -> Result<(), FormErrors> {
        let mut errors = FormErrors::default();
        let ctx = RecordContext { classes, systems };
        if let Err(fields) = validate_record(&self.record_input(), &ctx) {
            errors.fields = fields.iter().map(|e| (e.field, field_message(e))).collect();
        }
        errors.ips = self.ip_errors();
        if let Err(e) = self.lifecycle.hardware_override() {
            errors.fields.push(e);
        }
        let mut seen: Vec<&str> = Vec::new();
        for (i, c) in self.components.iter().enumerate() {
            if !c.is_kept_product() {
                continue;
            }
            if let Err((_, message)) = c.eol.parsed() {
                errors
                    .components
                    .push((i, format!("{EOL_ERROR_PREFIX}{message}")));
            }
            // 已知版本不能清成空白、新加入的元件須填版本；未修改的待補元件可以儲存
            if c.version.trim().is_empty() && !c.is_untouched_pending() {
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

pub use crate::product_search::{search_products, ProductChoice};

/// 目錄中的全部產品（依顯示名稱）。
pub fn product_choices(catalog: &Catalog) -> Vec<ProductChoice> {
    search_products(catalog, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::seed().unwrap()
    }

    fn systems() -> SystemDirectory {
        let sys = |code: &str, name: &str| cvewatch_core::assets::AssetSystem {
            code: cvewatch_core::assets::SystemCode::parse(code).unwrap(),
            name: name.into(),
        };
        SystemDirectory::new(vec![sys("NET", "網路服務"), sys("SYS", "核心系統")])
    }

    fn check(f: &AssetForm) -> Result<(), FormErrors> {
        f.validate(&catalog(), &AssetClasses::builtin(), &systems())
    }

    #[test]
    fn isms_id_from_another_system() {
        let mut f = filled();
        f.system = "NET".into();
        f.isms_id = "SYS-HW-001".into();
        let e = check(&f).unwrap_err();
        assert_eq!(e.field(StandardField::IsmsId), Some(ISMS_SYSTEM_MISMATCH));
        f.system.clear();
        f.isms_id = "LAB-HW-001".into();
        let e = check(&f).unwrap_err();
        assert_eq!(e.field(StandardField::IsmsId), Some(ISMS_SYSTEM_UNKNOWN));
        f.isms_id = "NET-HW-001".into();
        assert_eq!(check(&f), Ok(()));
        assert_eq!(field_label(StandardField::System), "資產說明");
        assert_eq!(field_label(StandardField::Custodian), "保管人");
    }

    #[test]
    fn admin_field_errors_use_interface_wording() {
        let mut f = filled();
        f.compliance = "5".into();
        f.personal_data = "maybe".into();
        let e = check(&f).unwrap_err();
        assert_eq!(
            e.field(StandardField::Compliance),
            Some("法規遵循性只能是 1–4")
        );
        assert_eq!(e.field(StandardField::PersonalData), Some(CHOOSE_FROM_LIST));
    }

    #[test]
    fn editing_keeps_the_stored_personal_data() {
        let mut f = filled();
        f.personal_data = PERSONAL_DATA_YES.into();
        f.category = "HW05".into();
        assert_eq!(
            f.effective_personal_data(&AssetClasses::builtin()),
            Some(true),
            "改類別不改變已設定的值"
        );
    }

    fn filled() -> AssetForm {
        let mut f = AssetForm::new();
        f.asset_tag = "FW-02".into();
        f.name = "fw02".into();
        f.category = "HW03".into();
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
        f.availability = "5".into();
        f.isms_id = "SYSHW001".into();
        f.add_product("paloalto-panos");
        f.add_product("paloalto-panos");
        let e = check(&f).unwrap_err();
        use StandardField as F;
        assert_eq!(e.field(F::AssetTag), Some(REQUIRED));
        assert_eq!(e.field(F::Name), Some(REQUIRED));
        assert_eq!(e.field(F::Category), Some(CHOOSE_FROM_LIST));
        assert_eq!(e.field(F::Exposure), Some(CHOOSE_FROM_LIST));
        assert_eq!(e.field(F::Availability), Some("可用性只能是 1–4"));
        assert_eq!(e.field(F::IsmsId), Some(ISMS_FORMAT));
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
            "availability",
            "isms_id",
            "paloalto-panos",
            "version",
        ] {
            assert!(!all.contains(internal), "{internal}: {all}");
        }
    }

    #[test]
    fn ratings_follow_the_class_template() {
        let classes = AssetClasses::builtin();
        let mut f = filled();
        f.category = "HW05".into();
        assert_eq!(
            RATING_FIELDS.map(|(field, _)| f.effective_rating(field, &classes)),
            [Some(2), Some(3), Some(4)]
        );
        assert_eq!(f.asset_value(&classes), Some(4));
        f.confidentiality = "4".into();
        assert_eq!(
            f.effective_rating(StandardField::Confidentiality, &classes),
            Some(4)
        );
        f.category = "SW01".into();
        f.confidentiality.clear();
        assert_eq!(f.asset_value(&classes), None, "SW01 沒有範本值");
        let e = check(&f).unwrap_err();
        for (field, _) in RATING_FIELDS {
            assert_eq!(e.field(field), Some(REQUIRED), "{field:?}");
        }
        f.category = "security".into();
        assert_eq!(f.class(&classes), None, "畫面編修不接受舊值");
        assert_eq!(
            check(&f).unwrap_err().field(StandardField::Category),
            Some(CHOOSE_FROM_LIST)
        );
    }

    #[test]
    fn isms_id_with_the_wrong_major_class() {
        let mut f = filled();
        f.isms_id = "SYS-SW-001".into();
        let e = check(&f).unwrap_err();
        assert_eq!(e.field(StandardField::IsmsId), Some(ISMS_CLASS_MISMATCH));
        f.isms_id = "SYS-HW-001".into();
        assert_eq!(check(&f), Ok(()));
        assert_eq!(field_label(StandardField::IsmsId), "ISMS 編號");
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

    #[test]
    fn renamed_to_reports_only_a_real_new_tag() {
        let mut f = filled();
        assert_eq!(f.renamed_to(), None, "新增模式沒有更名");
        f.mode = FormMode::Edit {
            original_tag: "NAS-01".into(),
        };
        for unchanged in ["NAS-01", " NAS-01 ", "", "   "] {
            f.asset_tag = unchanged.into();
            assert_eq!(f.renamed_to(), None, "{unchanged:?}");
        }
        f.asset_tag = " NAS-11 ".into();
        assert_eq!(f.renamed_to(), Some("NAS-11"));
        f.asset_tag = "nas-01".into();
        assert_eq!(f.renamed_to(), Some("nas-01"), "只改大小寫也算更名");
        assert_eq!(f.original_tag(), Some("NAS-01"));
    }
}
