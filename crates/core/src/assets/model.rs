//! 資產與元件的資料模型。

use super::classes::{Cia, ClassCode};

string_enum!(
    /// 曝露度
    Exposure {
        Internet => "internet",
        Dmz => "dmz",
        Internal => "internal",
        Mgmt => "mgmt",
    }
);

#[allow(clippy::derivable_impls)] // 列舉由巨集產生，無法加上 #[default]
impl Default for Exposure {
    fn default() -> Self {
        Exposure::Internal
    }
}

string_enum!(
    AssetStatus {
        Active => "active",
        Retired => "retired",
    }
);

string_enum!(
    /// 元件版本資訊的來源
    ComponentSource {
        Excel => "excel",
        Sbom => "sbom",
        Dpkg => "dpkg",
        Rpm => "rpm",
    }
);

/// 由清冊決定的資產欄位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRecord {
    pub asset_tag: String,
    pub name: String,
    /// ISMS 子類別代碼（可能是設定檔已不再定義的代碼，見 13 design D2）
    pub category: ClassCode,
    pub location: Option<String>,
    pub exposure: Exposure,
    /// 機密性、完整性、可用性；資產價值為 [`Cia::value`]
    pub cia: Cia,
    /// ISMS 資產編號（例如 `SYS-HW-001`；不分大小寫唯一）
    pub isms_id: Option<String>,
    /// 資產說明代碼（ISMS 系統代碼，例如 `SYS`；16 design D1）
    pub system_code: Option<String>,
    /// 權責單位
    pub responsible_unit: Option<String>,
    /// 保管人（取代舊版 owner）
    pub custodian: Option<String>,
    /// 使用單位
    pub user_unit: Option<String>,
    /// 法規遵循性（1–4；None 為未評定）
    pub compliance: Option<u8>,
    /// 是否含個人資料（以解析後的值儲存，不保存「依類別」）
    pub personal_data: bool,
    pub notes: Option<String>,
}

impl AssetRecord {
    /// 資通系統防護等級：MAX(C, I, A, 法規遵循性)；法規遵循性未評定時為 None（16 design D6）。
    pub fn protection_level(&self) -> Option<u8> {
        super::record_admin::protection_level(self.cia.value(), self.compliance)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub id: i64,
    pub record: AssetRecord,
    pub status: AssetStatus,
    pub first_seen: String,
    pub last_seen: String,
    pub retired_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentKind {
    /// 目錄產品（Excel 匯入）
    Product {
        product_id: i64,
        product_key: String,
    },
    /// OS 套件；`purl` 為不含版本的識別，`purl_full` 為含版本的完整 purl
    Package { purl: String, purl_full: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub id: i64,
    pub asset_id: i64,
    pub kind: ComponentKind,
    pub version_raw: String,
    pub version_norm: Option<String>,
    pub source: ComponentSource,
    pub updated_at: String,
}

impl Component {
    /// 清冊管理的目錄產品元件（可在畫面編修；主機套件元件由套件清單管理）。
    pub fn is_inventory_product(&self) -> bool {
        self.source == ComponentSource::Excel && matches!(self.kind, ComponentKind::Product { .. })
    }

    /// 識別元件的字串：產品 key，或不含版本的套件 purl。
    pub fn key(&self) -> &str {
        match &self.kind {
            ComponentKind::Product { product_key, .. } => product_key,
            ComponentKind::Package { purl, .. } => purl,
        }
    }

    /// 產品 key 或套件 purl，用於顯示。
    pub fn label(&self) -> &str {
        match &self.kind {
            ComponentKind::Product { product_key, .. } => product_key,
            ComponentKind::Package { purl_full, .. } => purl_full,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub version_raw: String,
    pub changed_at: String,
}

/// 清冊中無法辨識的產品寫法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedEntry {
    /// `asset_unresolved.id`（同一資產可有多筆相同寫法，編修以 id 指定）
    pub id: i64,
    pub asset_tag: String,
    pub product_raw: String,
    pub version_raw: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_is_case_insensitive_and_trims() {
        assert_eq!(Exposure::parse(" DMZ "), Some(Exposure::Dmz));
        assert_eq!(Exposure::parse("public"), None);
        assert_eq!(Exposure::default(), Exposure::Internal);
    }

    #[test]
    fn allowed_lists_every_value() {
        assert_eq!(Exposure::allowed(), "internet、dmz、internal、mgmt");
    }
}
