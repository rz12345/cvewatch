//! 表單的生命週期欄位（22 design D11）：三個到期日、硬體 EoL 覆寫與元件的軟體 EoL 覆寫。
//! 日期寫法與清冊相同（`YYYY-MM-DD`、`YYYY/MM/DD`），解析與驗證交給 core。

use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::dates::{format_date, format_opt};
use cvewatch_core::assets::record_lifecycle::{parse_override, OverrideError};
use cvewatch_core::assets::{AssetRecord, EolOverride};

/// 日期欄格式不符或日期不存在。
pub const DATE_INVALID: &str = "日期無效（請用 YYYY-MM-DD 或 YYYY/MM/DD）";
/// 覆寫填了日期但沒有來源。
pub const OVERRIDE_SOURCE_REQUIRED: &str = "覆寫必須填寫來源";
/// 元件列錯誤中的 EoL 覆寫訊息前綴（與版本錯誤列在同一列）。
pub const EOL_ERROR_PREFIX: &str = "EoL 覆寫：";
/// 覆寫填了來源但沒有日期。
pub const OVERRIDE_DATE_REQUIRED: &str = "請填寫覆寫的 EoL 日期";

/// 三個到期日與硬體 EoL 覆寫的輸入框（原始字串）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LifecycleDraft {
    pub warranty_end: String,
    pub support_end: String,
    pub license_end: String,
    /// 硬體 EoL 覆寫的日期與來源；兩者皆空白為沒有覆寫
    pub hardware_date: String,
    pub hardware_source: String,
}

/// 覆寫的輸入框文字（日期、來源）。
pub fn override_texts(o: Option<&EolOverride>) -> (String, String) {
    o.map(|o| (format_date(o.date), o.source.clone()))
        .unwrap_or_default()
}

impl LifecycleDraft {
    pub fn from_record(r: &AssetRecord) -> LifecycleDraft {
        let (hardware_date, hardware_source) = override_texts(r.hw_eol_override.as_ref());
        LifecycleDraft {
            warranty_end: format_opt(r.warranty_end),
            support_end: format_opt(r.support_end),
            license_end: format_opt(r.license_end),
            hardware_date,
            hardware_source,
        }
    }

    /// 硬體 EoL 覆寫（錯誤以 `eol`／`source` 欄位回報）。
    pub fn hardware_override(&self) -> Result<Option<EolOverride>, (StandardField, String)> {
        parse_override(&self.hardware_date, &self.hardware_source).map_err(override_error)
    }

    /// 清除硬體 EoL 覆寫（儲存後改由目錄的型號清單決定）。
    pub fn clear_hardware_override(&mut self) {
        self.hardware_date.clear();
        self.hardware_source.clear();
    }
}

/// 元件的軟體 EoL 覆寫輸入（[`super::ComponentDraft`] 的欄位）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EolDraft {
    pub date: String,
    pub source: String,
    /// 編輯前的覆寫（既有元件）
    pub original: Option<EolOverride>,
}

impl EolDraft {
    pub fn from_override(o: Option<&EolOverride>) -> EolDraft {
        let (date, source) = override_texts(o);
        EolDraft {
            date,
            source,
            original: o.cloned(),
        }
    }

    pub fn parsed(&self) -> Result<Option<EolOverride>, (StandardField, String)> {
        parse_override(&self.date, &self.source).map_err(override_error)
    }

    /// 解析成功且與編輯前不同時為新的值（`Some(None)` 為清除）。
    pub fn change(&self) -> Option<Option<EolOverride>> {
        let parsed = self.parsed().ok()?;
        (parsed != self.original).then_some(parsed)
    }

    pub fn clear(&mut self) {
        self.date.clear();
        self.source.clear();
    }
}

/// 覆寫錯誤的欄位（日期問題標在 `eol`，缺來源標在 `source`）與介面用語。
fn override_error(e: OverrideError) -> (StandardField, String) {
    match e {
        OverrideError::InvalidDate(_) => (StandardField::Eol, DATE_INVALID.into()),
        OverrideError::MissingDate => (StandardField::Eol, OVERRIDE_DATE_REQUIRED.into()),
        OverrideError::MissingSource => (StandardField::Source, OVERRIDE_SOURCE_REQUIRED.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_override_errors_are_placed_on_their_fields() {
        let draft = LifecycleDraft {
            hardware_date: "2027-06-30".into(),
            ..LifecycleDraft::default()
        };
        assert_eq!(
            draft.hardware_override(),
            Err((StandardField::Source, OVERRIDE_SOURCE_REQUIRED.into()))
        );
        let draft = LifecycleDraft {
            hardware_date: "2027-13-01".into(),
            hardware_source: "x".into(),
            ..LifecycleDraft::default()
        };
        assert_eq!(
            draft.hardware_override(),
            Err((StandardField::Eol, DATE_INVALID.into()))
        );
        assert_eq!(LifecycleDraft::default().hardware_override(), Ok(None));
    }

    #[test]
    fn component_override_change_detects_clearing() {
        let original = EolOverride {
            date: chrono::NaiveDate::from_ymd_opt(2028, 12, 31).unwrap(),
            source: "延長支援合約".into(),
        };
        let mut draft = EolDraft::from_override(Some(&original));
        assert_eq!(draft.date, "2028-12-31");
        assert_eq!(draft.change(), None);
        draft.clear();
        assert_eq!(draft.change(), Some(None));
    }
}
