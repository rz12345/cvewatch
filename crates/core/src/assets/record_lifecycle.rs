//! 資產生命週期日期與 EoL 覆寫的驗證（22 design D7、D8）：清冊匯入與畫面編修共用。

use chrono::NaiveDate;

use super::columns::StandardField;
use super::dates::{invalid_date_message, parse_date};
use super::model::EolOverride;
use super::record::{FieldError, FieldErrorKind, RecordInput};

/// 覆寫缺少來源時的訊息。
pub const OVERRIDE_SOURCE_REQUIRED: &str = "EoL 覆寫必須填寫來源";
/// 覆寫有來源但缺少日期時的訊息。
pub const OVERRIDE_DATE_REQUIRED: &str = "EoL 覆寫必須填寫日期";

/// 驗證通過的三個到期日（空白為 None）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct LifecycleDates {
    pub warranty_end: Option<NaiveDate>,
    pub support_end: Option<NaiveDate>,
    pub license_end: Option<NaiveDate>,
}

/// 解析一個日期欄；格式不符或日期不存在時記錄錯誤並回傳 None。
pub(super) fn date_field(
    field: StandardField,
    raw: &str,
    errors: &mut Vec<FieldError>,
) -> Option<NaiveDate> {
    parse_date(raw).unwrap_or_else(|()| {
        errors.push(FieldError::new(
            field,
            FieldErrorKind::Format,
            invalid_date_message(field.as_str(), raw),
        ));
        None
    })
}

pub(super) fn parse_dates(input: &RecordInput<'_>, errors: &mut Vec<FieldError>) -> LifecycleDates {
    LifecycleDates {
        warranty_end: date_field(StandardField::WarrantyEnd, input.warranty_end, errors),
        support_end: date_field(StandardField::SupportEnd, input.support_end, errors),
        license_end: date_field(StandardField::LicenseEnd, input.license_end, errors),
    }
}

/// 覆寫的錯誤（畫面標在日期或來源欄旁）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideError {
    /// 日期格式不符或不存在
    InvalidDate(String),
    /// 有來源但沒有日期
    MissingDate,
    /// 有日期但來源空白
    MissingSource,
}

impl OverrideError {
    /// `field` 為錯誤訊息中的欄名（清冊為 `eol`，畫面為「EoL 覆寫」）。
    pub fn message(&self, field: &str) -> String {
        match self {
            OverrideError::InvalidDate(raw) => invalid_date_message(field, raw),
            OverrideError::MissingDate => OVERRIDE_DATE_REQUIRED.to_string(),
            OverrideError::MissingSource => OVERRIDE_SOURCE_REQUIRED.to_string(),
        }
    }
}

/// 解析覆寫的日期與來源：兩者皆空白為沒有覆寫；只填其中之一或日期無效為錯誤。
pub fn parse_override(date: &str, source: &str) -> Result<Option<EolOverride>, OverrideError> {
    let source = source.trim();
    let parsed = parse_date(date).map_err(|()| OverrideError::InvalidDate(date.trim().into()))?;
    match (parsed, source.is_empty()) {
        (None, true) => Ok(None),
        (None, false) => Err(OverrideError::MissingDate),
        (Some(_), true) => Err(OverrideError::MissingSource),
        (Some(date), false) => Ok(Some(EolOverride {
            date,
            source: source.to_string(),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn recording_warranty_and_support_dates() {
        let mut errors = Vec::new();
        let dates = parse_dates(
            &RecordInput {
                warranty_end: "2027/12/31",
                support_end: "2027-06-30",
                license_end: " ",
                ..RecordInput::default()
            },
            &mut errors,
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(dates.warranty_end, Some(ymd(2027, 12, 31)));
        assert_eq!(dates.support_end, Some(ymd(2027, 6, 30)));
        assert_eq!(dates.license_end, None);
    }

    #[test]
    fn invalid_date_names_the_field_and_raw_text() {
        let mut errors = Vec::new();
        parse_dates(
            &RecordInput {
                support_end: "2027-02-30",
                ..RecordInput::default()
            },
            &mut errors,
        );
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field, StandardField::SupportEnd);
        assert_eq!(errors[0].kind, FieldErrorKind::Format);
        assert!(errors[0].message.contains("support_end"));
        assert!(errors[0].message.contains("2027-02-30"));
    }

    #[test]
    fn override_without_a_source_is_rejected() {
        assert_eq!(
            parse_override("2027-06-30", "  "),
            Err(OverrideError::MissingSource)
        );
        assert_eq!(
            OverrideError::MissingSource.message("eol"),
            "EoL 覆寫必須填寫來源"
        );
        assert_eq!(
            parse_override("", "延長合約"),
            Err(OverrideError::MissingDate)
        );
        assert_eq!(
            parse_override("2027-13-01", "x"),
            Err(OverrideError::InvalidDate("2027-13-01".into()))
        );
        assert_eq!(parse_override(" ", " "), Ok(None));
        assert_eq!(
            parse_override("2027/6/30", " 原廠延長支援函 "),
            Ok(Some(EolOverride {
                date: ymd(2027, 6, 30),
                source: "原廠延長支援函".into()
            }))
        );
    }
}
