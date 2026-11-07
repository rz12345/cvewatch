//! 資產與元件的資料模型。

use std::fmt;

/// 定義允許值的字串列舉：`as_str`、不分大小寫的 `parse`、`ALL`。
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }

            pub fn parse(value: &str) -> Option<Self> {
                let v = value.trim();
                Self::ALL.iter().copied().find(|x| x.as_str().eq_ignore_ascii_case(v))
            }

            /// 以「、」串起所有允許值，用於錯誤訊息。
            pub fn allowed() -> String {
                Self::ALL.iter().map(|x| x.as_str()).collect::<Vec<_>>().join("、")
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

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
    /// 資產類別
    Category {
        Network => "network",
        Security => "security",
        Virtualization => "virtualization",
        Server => "server",
        Storage => "storage",
        Host => "host",
    }
);

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

pub const MIN_CRITICALITY: u8 = 1;
pub const MAX_CRITICALITY: u8 = 3;
pub const DEFAULT_CRITICALITY: u8 = 2;

/// 由清冊決定的資產欄位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRecord {
    pub asset_tag: String,
    pub name: String,
    pub category: Category,
    pub location: Option<String>,
    pub owner: Option<String>,
    pub exposure: Exposure,
    pub criticality: u8,
    pub notes: Option<String>,
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
        assert_eq!(Category::parse("Storage"), Some(Category::Storage));
        assert_eq!(Exposure::default(), Exposure::Internal);
    }

    #[test]
    fn allowed_lists_every_value() {
        assert_eq!(Exposure::allowed(), "internet、dmz、internal、mgmt");
    }
}
