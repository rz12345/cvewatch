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
    Location,
    Owner,
    Notes,
    /// 停用欄位（13 design D3）：表頭仍可辨識，匯入時忽略並給出一次 notice
    Criticality,
}

/// `assets` 工作表的欄位順序（範本、匯出與表頭解析；不含停用欄位）。
const ASSET_COLUMNS: [StandardField; 13] = [
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
    StandardField::Location,
    StandardField::Owner,
    StandardField::Notes,
];

impl StandardField {
    /// 所有可辨識的欄名（含停用欄位）。
    pub const ALL: [StandardField; 14] = [
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
        StandardField::Location,
        StandardField::Owner,
        StandardField::Notes,
        StandardField::Criticality,
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
            StandardField::Criticality => "criticality",
            StandardField::Location => "location",
            StandardField::Owner => "owner",
            StandardField::Notes => "notes",
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
}

impl SheetKind {
    pub fn sheet_name(self) -> &'static str {
        match self {
            SheetKind::Assets => "assets",
            SheetKind::Components => "components",
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
        }
    }

    /// 範本與表頭解析使用的欄位順序。
    pub fn columns(self) -> &'static [StandardField] {
        match self {
            SheetKind::Assets => &ASSET_COLUMNS,
            SheetKind::Components => self.required(),
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

    /// 表頭 → 欄位位置；缺少必要欄位或重複對應時回傳錯誤。
    pub fn locate(
        &self,
        headers: &[String],
        kind: SheetKind,
    ) -> Result<HashMap<StandardField, usize>, CoreError> {
        let mut found: HashMap<StandardField, usize> = HashMap::new();
        for (i, header) in headers.iter().enumerate() {
            let Some(field) = self.field_for(header) else {
                continue;
            };
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
    fn unknown_target_is_rejected() {
        let err = ColumnMap::from_config(&BTreeMap::from([("x".into(), "serial".into())]));
        assert!(err.is_err());
    }
}
