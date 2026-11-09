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
    /// 硬體型號的 EoL（只在 `catalog.d`；種子不得有，22 design D4）
    #[serde(default)]
    pub hardware: Vec<HardwareDef>,
}

/// 目錄宣告的版本分支 EoL（22 design D4）。欄位選填以便載入時指出缺少的欄位。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EolCycleDef {
    pub cycle: String,
    pub eol: Option<String>,
    pub source: Option<String>,
}

/// 硬體型號的 EoL（22 design D4、D5）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HardwareDef {
    pub vendor: String,
    #[serde(default)]
    pub vendor_aliases: Vec<String>,
    pub model: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub eol: Option<String>,
    pub source: Option<String>,
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
    /// 元件角色（`role.rs` 的值）；未寫時為 `other`，覆寫時沿用先前來源
    pub role: Option<String>,
    pub advisory_source: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub cpe: Option<Vec<CpeDef>>,
    pub cna: Option<Vec<NameDef>>,
    pub csaf: Option<Vec<NameDef>>,
    /// 英文描述關鍵字（關聯性篩選「僅關鍵字相符」）
    pub keywords: Option<Vec<String>>,
    /// endoflife.date 產品代稱；`catalog.d` 寫 `""` 取消（22 design D4）
    pub endoflife: Option<String>,
    /// 目錄宣告的版本分支 EoL；與先前來源依 `cycle` 合併
    pub eol_cycles: Option<Vec<EolCycleDef>>,
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
