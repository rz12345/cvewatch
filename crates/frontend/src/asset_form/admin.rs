//! 資產表單的 ISMS 管理欄位（16 design D8）：資產說明、法規遵循性、防護等級、是否含個人資料。
//! 畫面以這裡的函式即時顯示「依類別（否）」與防護等級；法規遵循性、防護等級與資產說明的
//! 解析直接呼叫 core 的函式，與驗證、儲存的結果一致。

use cvewatch_core::assets::validate::{compliance_value, protection_level, resolve_system};
use cvewatch_core::assets::{AssetClasses, AssetSystem, SystemDirectory};

use super::AssetForm;

/// 是否含個人資料的表單值（與清冊相同）。
pub const PERSONAL_DATA_YES: &str = "Y";
pub const PERSONAL_DATA_NO: &str = "N";
/// 法規遵循性與防護等級未評定。
pub const UNRATED: &str = "未評定";
/// 沒有資產說明。
pub const UNASSIGNED_SYSTEM: &str = "未歸屬";
/// ISMS 編號的系統代碼與資產說明不符。
pub const ISMS_SYSTEM_MISMATCH: &str = "系統代碼與資產說明不符";
/// ISMS 編號的系統代碼還沒有對應的資產說明。
pub const ISMS_SYSTEM_UNKNOWN: &str = "系統代碼尚未建立資產說明，請先按「新增資產說明」建立";

/// 法規遵循性與防護等級的顯示文字：數值，或「未評定」（CLI、GUI 共用）。
pub fn rated_text(value: Option<u8>) -> String {
    value.map_or_else(|| UNRATED.to_string(), |n| n.to_string())
}

/// 「是」「否」。
pub fn personal_data_label(v: bool) -> &'static str {
    if v {
        "是"
    } else {
        "否"
    }
}

impl AssetForm {
    /// 目前類別的是否含個人資料預設值（類別無效時為 None）。
    pub fn personal_data_default(&self, classes: &AssetClasses) -> Option<bool> {
        self.class(classes).map(|c| c.personal_data)
    }

    /// 儲存後的是否含個人資料：選「依類別」時為類別預設值。
    pub fn effective_personal_data(&self, classes: &AssetClasses) -> Option<bool> {
        match self.personal_data.trim() {
            "" => self.personal_data_default(classes),
            v if v.eq_ignore_ascii_case(PERSONAL_DATA_YES) => Some(true),
            v if v.eq_ignore_ascii_case(PERSONAL_DATA_NO) => Some(false),
            _ => None,
        }
    }

    /// 法規遵循性（1–4）；未評定或不合法時為 None。
    pub fn compliance_value(&self) -> Option<u8> {
        compliance_value(&self.compliance)
    }

    /// 即時的防護等級 MAX(C, I, A, 法規遵循性)；法規遵循性未評定或 C/I/A 不完整時為 None。
    pub fn protection_level(&self, classes: &AssetClasses) -> Option<u8> {
        protection_level(self.asset_value(classes)?, self.compliance_value())
    }

    /// 儲存後的資產說明：有選時為所選者，未選時由 ISMS 編號的系統代碼帶出（找不到為 None）。
    /// 與 core 的 `resolve_system` 相同；不一致或尚未定義時為 None（錯誤由驗證回報）。
    pub fn effective_system<'d>(&self, systems: &'d SystemDirectory) -> Option<&'d AssetSystem> {
        let isms = Some(self.isms_id.trim()).filter(|v| !v.is_empty());
        let code = resolve_system(&self.system, isms, systems).ok()??;
        systems.get(&code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cvewatch_core::assets::SystemCode;

    fn systems() -> SystemDirectory {
        SystemDirectory::new(vec![AssetSystem {
            code: SystemCode::parse("NET").unwrap(),
            name: "網路服務".into(),
        }])
    }

    #[test]
    fn protection_level_follows_compliance() {
        let classes = AssetClasses::builtin();
        let mut f = AssetForm {
            category: "HW01".into(),
            confidentiality: "2".into(),
            integrity: "3".into(),
            availability: "3".into(),
            ..AssetForm::new()
        };
        assert_eq!(f.asset_value(&classes), Some(3));
        assert_eq!(f.protection_level(&classes), None, "未評定");
        f.compliance = "4".into();
        assert_eq!(f.asset_value(&classes), Some(3));
        assert_eq!(f.protection_level(&classes), Some(4));
        f.compliance = "9".into();
        assert_eq!(f.protection_level(&classes), None);
    }

    #[test]
    fn personal_data_follows_the_class() {
        let classes = AssetClasses::builtin();
        let mut f = AssetForm {
            category: "HW05".into(),
            ..AssetForm::new()
        };
        assert_eq!(f.personal_data_default(&classes), Some(false));
        assert_eq!(f.effective_personal_data(&classes), Some(false));
        f.personal_data = PERSONAL_DATA_YES.into();
        assert_eq!(f.effective_personal_data(&classes), Some(true));
        f.category.clear();
        f.personal_data.clear();
        assert_eq!(f.effective_personal_data(&classes), None, "類別未選");
    }

    #[test]
    fn system_from_choice_or_isms_id() {
        let dir = systems();
        let mut f = AssetForm {
            isms_id: "NET-HW-001".into(),
            ..AssetForm::new()
        };
        assert_eq!(f.effective_system(&dir).unwrap().code.as_str(), "NET");
        f.isms_id = "SYS-HW-001".into();
        assert!(f.effective_system(&dir).is_none());
        f.system = "NET".into();
        assert!(
            f.effective_system(&dir).is_none(),
            "與 ISMS 編號不符（與 core 相同）"
        );
        f.isms_id.clear();
        assert_eq!(f.effective_system(&dir).unwrap().name, "網路服務");
    }
}
