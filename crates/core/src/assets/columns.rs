//! 清冊欄位：標準欄名、自訂欄名對應（`[import.columns]`）與表頭解析。

use std::collections::{BTreeMap, HashMap};

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StandardField {
    AssetTag,
    Name,
    Category,
    Product,
    Version,
    Exposure,
    Confidentiality,
    Integrity,
    Availability,
    IsmsId,
    /// 資產說明代碼或名稱（16 design D3）
    System,
    ResponsibleUnit,
    Custodian,
    UserUnit,
    Compliance,
    PersonalData,
    /// 識別欄位（20 design D1）
    Brand,
    Model,
    SerialNumber,
    PropertyNumber,
    Hostname,
    ManagementUrl,
    /// 生命週期日期（22 design D6）
    WarrantyEnd,
    SupportEnd,
    LicenseEnd,
    Location,
    Notes,
    /// 舊欄名（16 design D3）：表頭視同 [`StandardField::Custodian`]，匯入時給出一次警告
    Owner,
    /// 停用欄位（13 design D3）：表頭仍可辨識，匯入時忽略並給出一次 notice
    Criticality,
    /// `systems` 工作表的代碼
    Code,
    /// `ips` 工作表的位址與用途（20 design D5）
    Ip,
    Purpose,
    /// `eol` 工作表的 EoL 日期與來源（22 design D7）
    Eol,
    Source,
}

/// `assets` 工作表的欄位順序（範本、匯出與表頭解析；不含停用欄位與舊欄名）。
const ASSET_COLUMNS: [StandardField; 27] = [
    StandardField::AssetTag,
    StandardField::Name,
    StandardField::Category,
    StandardField::Product,
    StandardField::Version,
    StandardField::Exposure,
    StandardField::Confidentiality,
    StandardField::Integrity,
    StandardField::Availability,
    StandardField::IsmsId,
    StandardField::System,
    StandardField::ResponsibleUnit,
    StandardField::Custodian,
    StandardField::UserUnit,
    StandardField::Compliance,
    StandardField::PersonalData,
    StandardField::Brand,
    StandardField::Model,
    StandardField::SerialNumber,
    StandardField::PropertyNumber,
    StandardField::Hostname,
    StandardField::ManagementUrl,
    StandardField::WarrantyEnd,
    StandardField::SupportEnd,
    StandardField::LicenseEnd,
    StandardField::Location,
    StandardField::Notes,
];

/// `systems` 工作表的欄位（16 design D3）。
const SYSTEM_COLUMNS: [StandardField; 2] = [StandardField::Code, StandardField::Name];

/// `ips` 工作表的欄位（20 design D5）：`asset_tag`、`ip` 必填，`purpose` 選填。
const IP_COLUMNS: [StandardField; 3] = [
    StandardField::AssetTag,
    StandardField::Ip,
    StandardField::Purpose,
];

/// `eol` 工作表的欄位（22 design D7）：`product` 空白為硬體覆寫。
const EOL_COLUMNS: [StandardField; 4] = [
    StandardField::AssetTag,
    StandardField::Product,
    StandardField::Eol,
    StandardField::Source,
];

/// 清冊沒有該欄時保留既有資產值的欄位（13 的 isms_id、16 的新欄位與 20 的識別欄位，
/// 16 design D4、20 design D7、22 的生命週期日期）。
pub const KEEPABLE_FIELDS: [StandardField; 16] = [
    StandardField::IsmsId,
    StandardField::System,
    StandardField::ResponsibleUnit,
    StandardField::Custodian,
    StandardField::UserUnit,
    StandardField::Compliance,
    StandardField::PersonalData,
    StandardField::Brand,
    StandardField::Model,
    StandardField::SerialNumber,
    StandardField::PropertyNumber,
    StandardField::Hostname,
    StandardField::ManagementUrl,
    StandardField::WarrantyEnd,
    StandardField::SupportEnd,
    StandardField::LicenseEnd,
];

impl StandardField {
    /// 所有可辨識的欄名（含停用欄位與舊欄名）。
    pub const ALL: [StandardField; 34] = [
        StandardField::AssetTag,
        StandardField::Name,
        StandardField::Category,
        StandardField::Product,
        StandardField::Version,
        StandardField::Exposure,
        StandardField::Confidentiality,
        StandardField::Integrity,
        StandardField::Availability,
        StandardField::IsmsId,
        StandardField::System,
        StandardField::ResponsibleUnit,
        StandardField::Custodian,
        StandardField::UserUnit,
        StandardField::Compliance,
        StandardField::PersonalData,
        StandardField::Brand,
        StandardField::Model,
        StandardField::SerialNumber,
        StandardField::PropertyNumber,
        StandardField::Hostname,
        StandardField::ManagementUrl,
        StandardField::WarrantyEnd,
        StandardField::SupportEnd,
        StandardField::LicenseEnd,
        StandardField::Location,
        StandardField::Notes,
        StandardField::Owner,
        StandardField::Criticality,
        StandardField::Code,
        StandardField::Ip,
        StandardField::Purpose,
        StandardField::Eol,
        StandardField::Source,
    ];

    /// 已停用、匯入時忽略的欄位。
    pub fn is_deprecated(self) -> bool {
        self == StandardField::Criticality
    }

    pub fn as_str(self) -> &'static str {
        match self {
            StandardField::AssetTag => "asset_tag",
            StandardField::Name => "name",
            StandardField::Category => "category",
            StandardField::Product => "product",
            StandardField::Version => "version",
            StandardField::Exposure => "exposure",
            StandardField::Confidentiality => "confidentiality",
            StandardField::Integrity => "integrity",
            StandardField::Availability => "availability",
            StandardField::IsmsId => "isms_id",
            StandardField::System => "system",
            StandardField::ResponsibleUnit => "responsible_unit",
            StandardField::Custodian => "custodian",
            StandardField::UserUnit => "user_unit",
            StandardField::Compliance => "compliance",
            StandardField::PersonalData => "personal_data",
            StandardField::Brand => "brand",
            StandardField::Model => "model",
            StandardField::SerialNumber => "serial_number",
            StandardField::PropertyNumber => "property_number",
            StandardField::Hostname => "hostname",
            StandardField::ManagementUrl => "management_url",
            StandardField::WarrantyEnd => "warranty_end",
            StandardField::SupportEnd => "support_end",
            StandardField::LicenseEnd => "license_end",
            StandardField::Eol => "eol",
            StandardField::Source => "source",
            StandardField::Criticality => "criticality",
            StandardField::Location => "location",
            StandardField::Owner => "owner",
            StandardField::Notes => "notes",
            StandardField::Code => "code",
            StandardField::Ip => "ip",
            StandardField::Purpose => "purpose",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        let n = name.trim();
        Self::ALL
            .into_iter()
            .find(|f| f.as_str().eq_ignore_ascii_case(n))
    }
}

/// 工作表種類與各自的欄位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetKind {
    Assets,
    Components,
    /// 資產說明（16 design D3；選填）
    Systems,
    /// 資產的 IP（20 design D5；選填）
    Ips,
    /// EoL 覆寫（22 design D7；選填）
    Eol,
}

impl SheetKind {
    pub fn sheet_name(self) -> &'static str {
        match self {
            SheetKind::Assets => "assets",
            SheetKind::Components => "components",
            SheetKind::Systems => "systems",
            SheetKind::Ips => "ips",
            SheetKind::Eol => "eol",
        }
    }

    pub fn required(self) -> &'static [StandardField] {
        match self {
            SheetKind::Assets => &[
                StandardField::AssetTag,
                StandardField::Name,
                StandardField::Category,
                StandardField::Product,
                StandardField::Version,
            ],
            SheetKind::Components => &[
                StandardField::AssetTag,
                StandardField::Product,
                StandardField::Version,
            ],
            SheetKind::Systems => &SYSTEM_COLUMNS,
            SheetKind::Ips => &IP_COLUMNS[..2],
            SheetKind::Eol => &EOL_COLUMNS,
        }
    }

    /// 範本與表頭解析使用的欄位順序。
    pub fn columns(self) -> &'static [StandardField] {
        match self {
            SheetKind::Assets => &ASSET_COLUMNS,
            SheetKind::Ips => &IP_COLUMNS,
            SheetKind::Eol => &EOL_COLUMNS,
            SheetKind::Components | SheetKind::Systems => self.required(),
        }
    }
}

/// 自訂欄名 → 標準欄位。比對時忽略前後空白與英文大小寫。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColumnMap {
    custom: HashMap<String, StandardField>,
}

fn header_key(header: &str) -> String {
    header.trim().to_lowercase()
}

impl ColumnMap {
    /// `columns` 為設定檔 `[import.columns]`（載入設定時已驗證目標欄位）。
    pub fn from_config(columns: &BTreeMap<String, String>) -> Result<Self, CoreError> {
        let custom = columns
            .iter()
            .map(|(header, target)| {
                StandardField::parse(target)
                    // `code` 只屬於 systems 工作表（不套用自訂欄名），不是可對應的目標
                    .filter(|f| *f != StandardField::Code)
                    .map(|f| (header_key(header), f))
                    .ok_or_else(|| {
                        CoreError::Invalid(format!("[import.columns] 的目標欄位 `{target}` 不存在"))
                    })
            })
            .collect::<Result<_, _>>()?;
        Ok(ColumnMap { custom })
    }

    pub fn field_for(&self, header: &str) -> Option<StandardField> {
        self.custom
            .get(&header_key(header))
            .copied()
            .or_else(|| StandardField::parse(header))
    }

    /// 表頭中出現的停用欄位（每個欄位只列一次，依 [`StandardField::ALL`] 順序）。
    pub fn deprecated_in(&self, headers: &[String]) -> Vec<StandardField> {
        StandardField::ALL
            .into_iter()
            .filter(|f| f.is_deprecated())
            .filter(|f| headers.iter().any(|h| self.field_for(h) == Some(*f)))
            .collect()
    }

    /// 表頭是否使用舊欄名 `owner`（含自訂欄名對應到 owner）。
    pub fn uses_legacy_owner(&self, headers: &[String]) -> bool {
        headers
            .iter()
            .any(|h| self.field_for(h) == Some(StandardField::Owner))
    }

    /// 表頭 → 欄位位置；缺少必要欄位或重複對應時回傳錯誤。`assets` 工作表的 `owner` 欄
    /// 視同 `custodian`，兩者並存時回傳錯誤。
    pub fn locate(
        &self,
        headers: &[String],
        kind: SheetKind,
    ) -> Result<HashMap<StandardField, usize>, CoreError> {
        let mut found: HashMap<StandardField, usize> = HashMap::new();
        let mut legacy_owner = false;
        for (i, header) in headers.iter().enumerate() {
            // `[import.columns]` 是為 assets／components 寫的；systems、ips 與 eol 工作表只認標準欄名，
            // 以免自訂欄名（例如把 code 對應到 asset_tag、把舊清冊的「IP」欄對應到 notes）讓範本與
            // 匯出檔無法匯入（16、20 code review）
            let field = match kind {
                SheetKind::Systems | SheetKind::Ips | SheetKind::Eol => {
                    StandardField::parse(header)
                }
                _ => self.field_for(header),
            };
            let Some(mut field) = field else {
                continue;
            };
            if field == StandardField::Owner && kind == SheetKind::Assets {
                legacy_owner = true;
                field = StandardField::Custodian;
            }
            if field == StandardField::Custodian && legacy_owner && found.contains_key(&field) {
                return Err(CoreError::Invalid(format!(
                    "{} 工作表的 owner 與 custodian 欄只能保留一欄（owner 已改名為 custodian 保管人）",
                    kind.sheet_name()
                )));
            }
            if kind.columns().contains(&field) && found.insert(field, i).is_some() {
                return Err(CoreError::Invalid(format!(
                    "{} 工作表有兩個欄位都對應到 `{}`",
                    kind.sheet_name(),
                    field.as_str()
                )));
            }
        }
        let missing: Vec<&str> = kind
            .required()
            .iter()
            .filter(|f| !found.contains_key(f))
            .map(|f| f.as_str())
            .collect();
        if !missing.is_empty() {
            return Err(CoreError::Invalid(format!(
                "{} 工作表缺少欄位：{}（可在設定檔 [import.columns] 對應自訂欄名）",
                kind.sheet_name(),
                missing.join("、")
            )));
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(h: &[&str]) -> Vec<String> {
        h.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn maps_custom_and_standard_headers() {
        let map = ColumnMap::from_config(&BTreeMap::from([
            ("財產編號".to_string(), "asset_tag".to_string()),
            ("版本".to_string(), "version".to_string()),
        ]))
        .unwrap();
        let found = map
            .locate(
                &headers(&["財產編號", " Name ", "category", "PRODUCT", "版本", "其他"]),
                SheetKind::Assets,
            )
            .unwrap();
        assert_eq!(found[&StandardField::AssetTag], 0);
        assert_eq!(found[&StandardField::Name], 1);
        assert_eq!(found[&StandardField::Version], 4);
    }

    #[test]
    fn missing_required_column_is_reported() {
        let err = ColumnMap::default()
            .locate(
                &headers(&["asset_tag", "name", "category", "product"]),
                SheetKind::Assets,
            )
            .unwrap_err();
        assert!(err.to_string().contains("version"), "{err}");
    }

    #[test]
    fn duplicate_mapping_is_rejected() {
        let map = ColumnMap::from_config(&BTreeMap::from([(
            "編號".to_string(),
            "asset_tag".to_string(),
        )]))
        .unwrap();
        let err = map
            .locate(
                &headers(&["編號", "asset_tag", "product", "version"]),
                SheetKind::Components,
            )
            .unwrap_err();
        assert!(err.to_string().contains("asset_tag"), "{err}");
    }

    #[test]
    fn criticality_is_recognised_but_not_a_column() {
        let map = ColumnMap::from_config(&BTreeMap::from([(
            "重要性".to_string(),
            "criticality".to_string(),
        )]))
        .unwrap();
        let h = headers(&[
            "asset_tag",
            "name",
            "category",
            "product",
            "version",
            "criticality",
            "重要性",
        ]);
        assert_eq!(map.deprecated_in(&h), vec![StandardField::Criticality]);
        let found = map.locate(&h, SheetKind::Assets).unwrap();
        assert!(!found.contains_key(&StandardField::Criticality));
        assert!(!SheetKind::Assets
            .columns()
            .contains(&StandardField::Criticality));
        assert!(SheetKind::Assets.columns().contains(&StandardField::IsmsId));
        assert!(map.deprecated_in(&headers(&["asset_tag"])).is_empty());
    }

    #[test]
    fn owner_header_is_read_as_custodian() {
        let h = headers(&[
            "asset_tag",
            "name",
            "category",
            "product",
            "version",
            "owner",
        ]);
        let map = ColumnMap::default();
        assert!(map.uses_legacy_owner(&h));
        let found = map.locate(&h, SheetKind::Assets).unwrap();
        assert_eq!(found[&StandardField::Custodian], 5);
        assert!(!SheetKind::Assets.columns().contains(&StandardField::Owner));
        assert!(SheetKind::Assets
            .columns()
            .contains(&StandardField::Custodian));
    }

    #[test]
    fn owner_and_custodian_together_are_rejected() {
        for h in [
            headers(&[
                "asset_tag",
                "name",
                "category",
                "product",
                "version",
                "owner",
                "custodian",
            ]),
            headers(&[
                "asset_tag",
                "name",
                "category",
                "product",
                "version",
                "custodian",
                "owner",
            ]),
        ] {
            let err = ColumnMap::default()
                .locate(&h, SheetKind::Assets)
                .unwrap_err();
            assert!(err.to_string().contains("只能保留一欄"), "{err}");
        }
    }

    #[test]
    fn custom_header_mapped_to_owner_is_an_alias() {
        let map = ColumnMap::from_config(&BTreeMap::from([(
            "負責人".to_string(),
            "owner".to_string(),
        )]))
        .unwrap();
        let h = headers(&[
            "asset_tag",
            "name",
            "category",
            "product",
            "version",
            "負責人",
        ]);
        assert!(map.uses_legacy_owner(&h));
        let found = map.locate(&h, SheetKind::Assets).unwrap();
        assert_eq!(found[&StandardField::Custodian], 5);
    }

    #[test]
    fn custom_mappings_do_not_apply_to_the_systems_sheet() {
        let map = ColumnMap::from_config(&BTreeMap::from([(
            "code".to_string(),
            "asset_tag".to_string(),
        )]))
        .unwrap();
        let found = map
            .locate(&headers(&["code", "name"]), SheetKind::Systems)
            .unwrap();
        assert_eq!(found[&StandardField::Code], 0);
        let err = ColumnMap::from_config(&BTreeMap::from([("x".into(), "code".into())]));
        assert!(err.is_err(), "code 不是可對應的目標");
    }

    #[test]
    fn systems_sheet_columns() {
        let found = ColumnMap::default()
            .locate(&headers(&["Code", "name"]), SheetKind::Systems)
            .unwrap();
        assert_eq!(found[&StandardField::Code], 0);
        assert_eq!(found[&StandardField::Name], 1);
        let err = ColumnMap::default()
            .locate(&headers(&["code"]), SheetKind::Systems)
            .unwrap_err();
        assert!(
            err.to_string().contains("systems 工作表缺少欄位：name"),
            "{err}"
        );
    }

    #[test]
    fn ips_sheet_columns_ignore_custom_names() {
        let map = ColumnMap::from_config(&BTreeMap::from([
            ("編號".to_string(), "asset_tag".to_string()),
            ("ip".to_string(), "notes".to_string()),
            ("財產編號".to_string(), "property_number".to_string()),
        ]))
        .unwrap();
        // 匯出檔的 ips 工作表（標準欄名）在有自訂欄名時仍可匯入
        let found = map
            .locate(&headers(&["asset_tag", "IP", "purpose"]), SheetKind::Ips)
            .unwrap();
        assert_eq!(found[&StandardField::AssetTag], 0);
        assert_eq!(found[&StandardField::Ip], 1);
        assert_eq!(found[&StandardField::Purpose], 2);
        let err = ColumnMap::default()
            .locate(&headers(&["asset_tag", "purpose"]), SheetKind::Ips)
            .unwrap_err();
        assert!(err.to_string().contains("ips 工作表缺少欄位：ip"), "{err}");
        let err = map
            .locate(&headers(&["編號", "ip"]), SheetKind::Ips)
            .unwrap_err();
        assert!(
            err.to_string().contains("asset_tag"),
            "自訂欄名不套用：{err}"
        );
        // assets 工作表仍套用自訂欄名
        let found = map
            .locate(
                &headers(&["編號", "name", "category", "product", "version", "財產編號"]),
                SheetKind::Assets,
            )
            .unwrap();
        assert_eq!(found[&StandardField::PropertyNumber], 5);
        assert_eq!(SheetKind::Assets.columns().len(), 27);
    }

    #[test]
    fn unknown_target_is_rejected() {
        let err = ColumnMap::from_config(&BTreeMap::from([("x".into(), "serial".into())]));
        assert!(err.is_err());
    }
}
