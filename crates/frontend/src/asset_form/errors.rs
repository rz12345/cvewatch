//! 表單的驗證錯誤與介面用語（20 design D10：由 [`super`] 原樣移出，由 `asset_form` 重新匯出）。

use cvewatch_core::assets::classes::{MAX_RATING, MIN_RATING};
use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::validate::{FieldError, FieldErrorKind};

use super::admin::{ISMS_SYSTEM_MISMATCH, ISMS_SYSTEM_UNKNOWN};
use super::identity::{hostname_too_long, HOSTNAME_FORMAT, MANAGEMENT_URL_FORMAT, URL_CREDENTIALS};

/// 欄位的介面名稱（對話框標籤與錯誤摘要用；清冊與 CLI 仍用英文欄名）。
pub fn field_label(field: StandardField) -> &'static str {
    match field {
        StandardField::AssetTag => "資產編號",
        StandardField::Name => "名稱",
        StandardField::Category => "類別",
        StandardField::Exposure => "曝露度",
        StandardField::Confidentiality => "機密性",
        StandardField::Integrity => "完整性",
        StandardField::Availability => "可用性",
        StandardField::IsmsId => "ISMS 編號",
        StandardField::System => "資產說明",
        StandardField::ResponsibleUnit => "權責單位",
        StandardField::Custodian | StandardField::Owner => "保管人",
        StandardField::UserUnit => "使用單位",
        StandardField::Compliance => "法規遵循性",
        StandardField::PersonalData => "是否含個人資料",
        StandardField::Brand => "廠牌",
        StandardField::Model => "型號",
        StandardField::SerialNumber => "序號",
        StandardField::PropertyNumber => "財產編號",
        StandardField::Hostname => "主機名稱",
        StandardField::ManagementUrl => "管理介面位址",
        StandardField::Ip => "IP",
        StandardField::Purpose => "用途",
        StandardField::Criticality => "重要性（已停用）",
        StandardField::Location => "位置",
        StandardField::Notes => "備註",
        StandardField::Product => "產品",
        StandardField::Version => "版本",
        StandardField::Code => "代碼",
    }
}

/// core 的欄位錯誤種類轉為介面用語（不解析 core 的訊息字串；core 與 CLI 的文字不變）。
pub(super) fn field_message(e: &FieldError) -> String {
    match (e.kind, e.field) {
        (FieldErrorKind::Missing, _) => REQUIRED.into(),
        (FieldErrorKind::NotAllowed, StandardField::IsmsId) => ISMS_FORMAT.into(),
        (FieldErrorKind::NotAllowed, _) => CHOOSE_FROM_LIST.into(),
        (FieldErrorKind::OutOfRange, field) => {
            format!("{}只能是 {MIN_RATING}–{MAX_RATING}", field_label(field))
        }
        (FieldErrorKind::ClassMismatch, _) => ISMS_CLASS_MISMATCH.into(),
        (FieldErrorKind::SystemMismatch, _) => ISMS_SYSTEM_MISMATCH.into(),
        (FieldErrorKind::UnknownSystem, StandardField::IsmsId) => ISMS_SYSTEM_UNKNOWN.into(),
        (FieldErrorKind::UnknownSystem, _) => CHOOSE_FROM_LIST.into(),
        (FieldErrorKind::Format, StandardField::ManagementUrl) => MANAGEMENT_URL_FORMAT.into(),
        (FieldErrorKind::Format, _) => HOSTNAME_FORMAT.into(),
        (FieldErrorKind::Credentials, _) => URL_CREDENTIALS.into(),
        (FieldErrorKind::TooLong, _) => hostname_too_long(),
    }
}

/// 必填欄位留空。
pub const REQUIRED: &str = "必填";
/// 下拉選單的值不在允許清單中。
pub const CHOOSE_FROM_LIST: &str = "請從清單選擇";
/// ISMS 編號格式不符。
pub const ISMS_FORMAT: &str = "格式應為「系統代碼-大類-流水號」，例如 SYS-HW-001";
/// ISMS 編號的大類與類別不符。
pub const ISMS_CLASS_MISMATCH: &str = "大類（HW／SW）與類別不符";
/// 元件沒有版本。
pub const VERSION_REQUIRED: &str = "請填寫版本";
/// 沒有任何元件。
pub const COMPONENT_REQUIRED: &str =
    "至少需要一個元件（產品與版本）；不再使用此資產時請改用「退役…」";

/// 本地驗證錯誤，依位置分類。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormErrors {
    pub fields: Vec<(StandardField, String)>,
    /// (元件列索引, 訊息)
    pub components: Vec<(usize, String)>,
    /// (未辨識產品列索引, 訊息)
    pub unresolved: Vec<(usize, String)>,
    /// (IP 列索引, 訊息)
    pub ips: Vec<(usize, String)>,
    /// 不屬於任何欄位的錯誤（例如沒有任何元件）
    pub general: Vec<String>,
}

impl FormErrors {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
            && self.components.is_empty()
            && self.unresolved.is_empty()
            && self.ips.is_empty()
            && self.general.is_empty()
    }

    pub fn field(&self, field: StandardField) -> Option<&str> {
        self.fields
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, m)| m.as_str())
    }

    pub fn component(&self, index: usize) -> Vec<&str> {
        self.components
            .iter()
            .filter(|(i, _)| *i == index)
            .map(|(_, m)| m.as_str())
            .collect()
    }

    pub fn unresolved_row(&self, index: usize) -> Vec<&str> {
        self.unresolved
            .iter()
            .filter(|(i, _)| *i == index)
            .map(|(_, m)| m.as_str())
            .collect()
    }

    pub fn ip_row(&self, index: usize) -> Vec<&str> {
        self.ips
            .iter()
            .filter(|(i, _)| *i == index)
            .map(|(_, m)| m.as_str())
            .collect()
    }

    /// 全部訊息（不在對話框中顯示時的摘要；欄位錯誤前加上欄位名稱）。
    pub fn summary(&self) -> String {
        self.fields
            .iter()
            .map(|(f, m)| format!("{}：{m}", field_label(*f)))
            .chain(self.components.iter().map(|(_, m)| m.clone()))
            .chain(self.unresolved.iter().map(|(_, m)| m.clone()))
            .chain(
                self.ips
                    .iter()
                    .map(|(i, m)| format!("第 {} 個 IP：{m}", i + 1)),
            )
            .chain(self.general.iter().cloned())
            .collect::<Vec<_>>()
            .join("；")
    }
}
