//! 匯入差異中的欄位變更（18 design D5）：欄位、舊值、新值，以畫面用語呈現。
//!
//! 一般清冊匯入的預覽只列欄位名稱（[`FieldChange::name`]）；ISMS 匯入的預覽列出新舊值
//! （[`FieldChange::describe`]）。

use super::classes::{AssetClasses, ClassCode};
use super::columns::StandardField;
use super::model::AssetRecord;

/// 未填的值。
pub const BLANK_VALUE: &str = "-";
/// 法規遵循性未評定。
pub const UNRATED_VALUE: &str = "未評定";

/// 一個欄位的變更。類別存代碼，呈現時加上子類別名稱（[`FieldChange::describe`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    pub field: StandardField,
    pub from: String,
    pub to: String,
}

impl FieldChange {
    pub fn new(field: StandardField, from: impl Into<String>, to: impl Into<String>) -> Self {
        FieldChange {
            field,
            from: from.into(),
            to: to.into(),
        }
    }

    /// 標準欄名（一般清冊匯入的預覽）。
    pub fn name(&self) -> &'static str {
        self.field.as_str()
    }

    /// 畫面上的欄位名稱。
    pub fn label(&self) -> &'static str {
        field_label(self.field)
    }

    /// 「機密性 2 → 3」「類別 HW03 資安設備 → HW05 儲存設備」。
    pub fn describe(&self, classes: &AssetClasses) -> String {
        let show = |v: &str| match (self.field, ClassCode::parse(v)) {
            (StandardField::Category, Some(code)) => classes.label(&code),
            _ => v.to_string(),
        };
        format!("{} {} → {}", self.label(), show(&self.from), show(&self.to))
    }
}

/// 畫面上的欄位名稱（與 GUI 編輯對話框相同的用語）。
pub fn field_label(field: StandardField) -> &'static str {
    match field {
        StandardField::AssetTag => "資產編號",
        StandardField::Name => "名稱",
        StandardField::Category => "類別",
        StandardField::Product => "產品",
        StandardField::Version => "版本",
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
        StandardField::Location => "地點",
        StandardField::Notes => "備註",
        StandardField::Criticality => "重要性",
        StandardField::Code => "代碼",
        StandardField::Ip => "IP",
        StandardField::Purpose => "用途",
    }
}

fn text(v: &Option<String>) -> String {
    v.clone().unwrap_or_else(|| BLANK_VALUE.to_string())
}

/// 一個欄位的比較值（原值，`None` 與 `Some("-")` 不同）與顯示文字。
fn value(field: StandardField, key: String, shown: String) -> (StandardField, String, String) {
    (field, key, shown)
}

fn opt(field: StandardField, v: &Option<String>) -> (StandardField, String, String) {
    value(field, format!("{v:?}"), text(v))
}

fn yes_no(v: bool) -> String {
    if v { "是" } else { "否" }.to_string()
}

/// 兩筆紀錄間有變更的欄位（依清冊欄位順序）。以原值比較（與寫入時判斷是否變更的方式相同），
/// 新舊值以畫面用語呈現。
pub(super) fn changed_fields(old: &AssetRecord, new: &AssetRecord) -> Vec<FieldChange> {
    use StandardField as F;
    let rated = |v: Option<u8>| v.map_or_else(|| UNRATED_VALUE.to_string(), |n| n.to_string());
    let same = |f: F, s: String| value(f, s.clone(), s);
    let values = |r: &AssetRecord| -> [(F, String, String); 21] {
        [
            same(F::Name, r.name.clone()),
            same(F::Category, r.category.as_str().to_string()),
            opt(F::Location, &r.location),
            same(F::Exposure, r.exposure.as_str().to_string()),
            same(F::Confidentiality, r.cia.confidentiality.to_string()),
            same(F::Integrity, r.cia.integrity.to_string()),
            same(F::Availability, r.cia.availability.to_string()),
            opt(F::IsmsId, &r.isms_id),
            opt(F::System, &r.system_code),
            opt(F::ResponsibleUnit, &r.responsible_unit),
            opt(F::Custodian, &r.custodian),
            opt(F::UserUnit, &r.user_unit),
            value(
                F::Compliance,
                format!("{:?}", r.compliance),
                rated(r.compliance),
            ),
            same(F::PersonalData, yes_no(r.personal_data)),
            opt(F::Brand, &r.brand),
            opt(F::Model, &r.model),
            opt(F::SerialNumber, &r.serial_number),
            opt(F::PropertyNumber, &r.property_number),
            opt(F::Hostname, &r.hostname),
            opt(F::ManagementUrl, &r.management_url),
            opt(F::Notes, &r.notes),
        ]
    };
    values(old)
        .into_iter()
        .zip(values(new))
        .filter(|((_, a, _), (_, b, _))| a != b)
        .map(|((field, _, from), (_, _, to))| FieldChange { field, from, to })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::classes::Cia;
    use crate::assets::model::Exposure;

    fn record() -> AssetRecord {
        AssetRecord {
            asset_tag: "FW-01".into(),
            name: "fw01".into(),
            category: ClassCode::parse("HW03").unwrap(),
            location: None,
            exposure: Exposure::Internet,
            cia: Cia::new(2, 3, 4),
            isms_id: Some("NET-HW-002".into()),
            system_code: Some("NET".into()),
            responsible_unit: None,
            custodian: Some("王小明".into()),
            user_unit: None,
            compliance: None,
            personal_data: false,
            brand: None,
            model: None,
            serial_number: None,
            property_number: None,
            hostname: None,
            management_url: None,
            notes: None,
        }
    }

    #[test]
    fn isms_fields_changed_for_an_existing_asset() {
        let old = record();
        let new = AssetRecord {
            cia: Cia::new(3, 3, 4),
            custodian: Some("李小華".into()),
            compliance: Some(2),
            category: ClassCode::parse("HW05").unwrap(),
            ..record()
        };
        let classes = AssetClasses::builtin();
        let lines: Vec<String> = changed_fields(&old, &new)
            .iter()
            .map(|c| c.describe(&classes))
            .collect();
        assert_eq!(
            lines,
            [
                "類別 HW03 資安設備 → HW05 儲存設備",
                "機密性 2 → 3",
                "保管人 王小明 → 李小華",
                "法規遵循性 未評定 → 2",
            ]
        );
        let names: Vec<&str> = changed_fields(&old, &new)
            .iter()
            .map(FieldChange::name)
            .collect();
        assert_eq!(
            names,
            ["category", "confidentiality", "custodian", "compliance"]
        );
    }

    #[test]
    fn blank_values_read_as_dash() {
        let new = AssetRecord {
            location: Some("機房 A".into()),
            ..record()
        };
        let change = &changed_fields(&record(), &new)[0];
        assert_eq!(change.describe(&AssetClasses::builtin()), "地點 - → 機房 A");
        assert!(changed_fields(&record(), &record()).is_empty());
        // 空白與填「-」是不同的值：預覽要列出這個變更（寫入時也會寫入）
        let dash = AssetRecord {
            location: Some(BLANK_VALUE.into()),
            ..record()
        };
        assert_eq!(changed_fields(&record(), &dash).len(), 1);
    }
}
