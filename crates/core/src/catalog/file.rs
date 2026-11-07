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
    /// ESXi build 編號 → 版本名稱（`esxi` 版本格式比較 build 用）
    #[serde(default)]
    pub esxi_build: Vec<EsxiBuildDef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EsxiBuildDef {
    pub build: u64,
    /// 名稱形式，例如 `8.0 U3c`
    pub version: String,
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
    /// 英文描述關鍵字（關聯性篩選「僅關鍵字相符」）
    pub keywords: Option<Vec<String>>,
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
