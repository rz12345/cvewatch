//! 目錄 TOML 檔格式。種子與 `catalog.d\*.toml` 使用相同格式。
//!
//! 除了 `key` 之外的欄位都可省略：使用者檔中與既有產品相同 `key` 的條目，
//! 有寫的欄位覆寫、沒寫的沿用；`aliases` 則與既有別名合併。

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogFile {
    #[serde(default)]
    pub product: Vec<ProductDef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductDef {
    pub key: String,
    pub display_name: Option<String>,
    pub vendor_display: Option<String>,
    pub version_scheme: Option<String>,
    pub advisory_source: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub cpe: Option<Vec<CpeDef>>,
    pub cna: Option<Vec<NameDef>>,
    pub csaf: Option<Vec<NameDef>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CpeDef {
    pub part: String,
    pub vendor: String,
    pub product: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameDef {
    pub vendor: String,
    pub product: String,
}
