//! 元件角色：產品作為資產元件時的角色，用於畫面與 CLI 依角色分組顯示。
//!
//! 列舉順序即顯示順序：硬體相關（韌體、BIOS、BMC）在前，接著依 ISMS 子類別 SW02–SW07，
//! 最後是應用程式元件與其他。角色只影響顯示，不影響比對。
//!
//! 不使用 `string_enum!`：該巨集的 `parse` 會 trim 並忽略大小寫，而目錄的 `role` 與
//! `version_scheme` 一樣須完全比對（design D1），另需 `Ord`（組順序）與 `Default`（其他）。

/// 產品目錄宣告的元件角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ComponentRole {
    Firmware,
    Bios,
    Bmc,
    Os,
    Dbms,
    Web,
    Tool,
    Security,
    Suite,
    App,
    #[default]
    Other,
}

impl ComponentRole {
    /// 依顯示順序排列的全部角色。
    pub const ALL: [ComponentRole; 11] = [
        ComponentRole::Firmware,
        ComponentRole::Bios,
        ComponentRole::Bmc,
        ComponentRole::Os,
        ComponentRole::Dbms,
        ComponentRole::Web,
        ComponentRole::Tool,
        ComponentRole::Security,
        ComponentRole::Suite,
        ComponentRole::App,
        ComponentRole::Other,
    ];

    /// 目錄檔中的值。
    pub fn as_str(self) -> &'static str {
        match self {
            ComponentRole::Firmware => "firmware",
            ComponentRole::Bios => "bios",
            ComponentRole::Bmc => "bmc",
            ComponentRole::Os => "os",
            ComponentRole::Dbms => "dbms",
            ComponentRole::Web => "web",
            ComponentRole::Tool => "tool",
            ComponentRole::Security => "security",
            ComponentRole::Suite => "suite",
            ComponentRole::App => "app",
            ComponentRole::Other => "other",
        }
    }

    /// 中文顯示名稱（固定常數，不隨 `[asset_classes]` 改名）。
    pub fn label(self) -> &'static str {
        match self {
            ComponentRole::Firmware => "韌體",
            ComponentRole::Bios => "BIOS",
            ComponentRole::Bmc => "BMC",
            ComponentRole::Os => "作業系統",
            ComponentRole::Dbms => "資料庫管理系統",
            ComponentRole::Web => "網站服務系統",
            ComponentRole::Tool => "應用工具軟體",
            ComponentRole::Security => "資通安全系統",
            ComponentRole::Suite => "一般套裝軟體",
            ComponentRole::App => "應用程式元件",
            ComponentRole::Other => "其他",
        }
    }

    /// 完全比對目錄檔中的值（不 trim、不轉換大小寫）。
    pub fn parse(value: &str) -> Option<ComponentRole> {
        Self::ALL.into_iter().find(|r| r.as_str() == value)
    }

    /// 以「、」串起所有可用值，用於錯誤訊息（與 `string_enum!` 的 `allowed` 相同）。
    pub fn allowed() -> String {
        Self::ALL
            .iter()
            .map(|r| r.as_str())
            .collect::<Vec<_>>()
            .join("、")
    }
}

impl std::fmt::Display for ComponentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// `ALL` 必須列出每個值：`Other` 是最後一個值，數量不符時編譯失敗。
const _: () = assert!(ComponentRole::ALL.len() == ComponentRole::Other as usize + 1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_and_labels_follow_display_order() {
        let pairs: Vec<_> = ComponentRole::ALL
            .iter()
            .map(|r| (r.as_str(), r.label()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("firmware", "韌體"),
                ("bios", "BIOS"),
                ("bmc", "BMC"),
                ("os", "作業系統"),
                ("dbms", "資料庫管理系統"),
                ("web", "網站服務系統"),
                ("tool", "應用工具軟體"),
                ("security", "資通安全系統"),
                ("suite", "一般套裝軟體"),
                ("app", "應用程式元件"),
                ("other", "其他"),
            ]
        );
    }

    #[test]
    fn all_is_sorted_by_display_order() {
        let mut sorted = ComponentRole::ALL;
        sorted.sort();
        assert_eq!(sorted, ComponentRole::ALL);
    }

    #[test]
    fn parse_is_exact() {
        for role in ComponentRole::ALL {
            assert_eq!(ComponentRole::parse(role.as_str()), Some(role));
        }
        assert_eq!(ComponentRole::parse("OS"), None);
        assert_eq!(ComponentRole::parse("database"), None);
        assert_eq!(ComponentRole::parse(" os"), None);
    }

    #[test]
    fn default_is_other() {
        assert_eq!(ComponentRole::default(), ComponentRole::Other);
        assert_eq!(
            ComponentRole::allowed(),
            "firmware、bios、bmc、os、dbms、web、tool、security、suite、app、other"
        );
        assert_eq!(ComponentRole::Bmc.to_string(), "bmc");
    }
}
