//! ISMS 管理欄位的驗證（16 design D2、D6）：資產說明歸屬、法規遵循性、是否含個人資料。
//! 清冊匯入與畫面編修共用；錯誤以 [`FieldError`] 回報，畫面可標在對應欄位旁。

use unicode_normalization::UnicodeNormalization;

use super::classes::{AssetClass, MAX_RATING, MIN_RATING};
use super::columns::StandardField;
use super::record::{FieldError, FieldErrorKind};
use super::systems::{SystemCode, SystemDirectory};

/// 是否含個人資料接受的寫法（不分大小寫、全形視同半形）。
const PERSONAL_DATA_YES: [&str; 2] = ["y", "是"];
const PERSONAL_DATA_NO: [&str; 2] = ["n", "否"];

/// 法規遵循性的值（1–4）；空白或不合法為 None。畫面即時顯示與驗證共用。
pub fn compliance_value(value: &str) -> Option<u8> {
    value
        .trim()
        .parse::<u8>()
        .ok()
        .filter(|n| (MIN_RATING..=MAX_RATING).contains(n))
}

/// 資通系統防護等級 MAX(資產價值, 法規遵循性)；法規遵循性未評定時為 None（16 design D6）。
pub fn protection_level(asset_value: u8, compliance: Option<u8>) -> Option<u8> {
    compliance.map(|d| asset_value.max(d))
}

/// 法規遵循性：空白為未評定；有填時必須是 1–4。
pub(super) fn parse_compliance(value: &str, errors: &mut Vec<FieldError>) -> Option<u8> {
    if value.is_empty() {
        return None;
    }
    match compliance_value(value) {
        Some(n) => Some(n),
        None => {
            errors.push(FieldError::new(
                StandardField::Compliance,
                FieldErrorKind::OutOfRange,
                format!("法規遵循性「{value}」必須是 {MIN_RATING}–{MAX_RATING}"),
            ));
            None
        }
    }
}

/// 是否含個人資料：Y／N／是／否；空白時帶入子類別預設值（類別無效時回傳 None，錯誤已另記錄）。
pub(super) fn parse_personal_data(
    value: &str,
    class: Option<&AssetClass>,
    errors: &mut Vec<FieldError>,
) -> Option<bool> {
    if value.is_empty() {
        return class.map(|c| c.personal_data);
    }
    let v: String = value.nfkc().collect::<String>().to_lowercase();
    if PERSONAL_DATA_YES.contains(&v.as_str()) {
        return Some(true);
    }
    if PERSONAL_DATA_NO.contains(&v.as_str()) {
        return Some(false);
    }
    errors.push(FieldError::new(
        StandardField::PersonalData,
        FieldErrorKind::NotAllowed,
        format!("是否含個人資料「{value}」只能填 Y、N、是或否"),
    ));
    None
}

/// ISMS 編號的系統代碼（前三碼，大寫）；格式已由 ISMS 編號驗證檢查。
pub fn isms_system_code(isms_id: &str) -> Option<SystemCode> {
    isms_id.split('-').next().and_then(SystemCode::parse)
}

/// 解析資產說明歸屬（16 spec Asset system membership）：
/// - 有填時依代碼或名稱查詢，找不到為錯誤（`System` 欄）
/// - 空白且有 ISMS 編號時由其系統代碼帶出，找不到為錯誤（`IsmsId` 欄）
/// - 兩者都有時系統代碼必須相同（`IsmsId` 欄）
///
/// 空白且沒有 ISMS 編號時為 `Ok(None)`。
pub fn resolve_system(
    raw: &str,
    isms_id: Option<&str>,
    systems: &SystemDirectory,
) -> Result<Option<SystemCode>, FieldError> {
    let raw = raw.trim();
    let from_isms = isms_id.and_then(isms_system_code);
    if raw.is_empty() {
        let Some(code) = from_isms else {
            return Ok(None);
        };
        return match systems.get(&code) {
            Some(s) => Ok(Some(s.code.clone())),
            None => Err(FieldError::new(
                StandardField::IsmsId,
                FieldErrorKind::UnknownSystem,
                format!(
                    "ISMS 編號「{}」的系統代碼 {code} 尚未定義為資產說明",
                    isms_id.unwrap_or_default()
                ),
            )),
        };
    }
    let Some(system) = systems.find(raw) else {
        return Err(FieldError::new(
            StandardField::System,
            FieldErrorKind::UnknownSystem,
            format!("資產說明「{raw}」尚未定義"),
        ));
    };
    if let Some(code) = from_isms.filter(|c| c != &system.code) {
        return Err(FieldError::new(
            StandardField::IsmsId,
            FieldErrorKind::SystemMismatch,
            format!(
                "ISMS 編號「{}」的系統代碼 {code} 與資產說明 {} 不符",
                isms_id.unwrap_or_default(),
                system.code
            ),
        ));
    }
    Ok(Some(system.code.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::systems::AssetSystem;

    fn dir() -> SystemDirectory {
        SystemDirectory::new(vec![
            AssetSystem {
                code: SystemCode::parse("NET").unwrap(),
                name: "網路服務".into(),
            },
            AssetSystem {
                code: SystemCode::parse("SYS").unwrap(),
                name: "核心系統".into(),
            },
        ])
    }

    fn code(r: Result<Option<SystemCode>, FieldError>) -> Option<String> {
        r.unwrap().map(|c| c.as_str().to_string())
    }

    #[test]
    fn system_derived_from_the_isms_id() {
        assert_eq!(
            code(resolve_system("", Some("NET-HW-002"), &dir())),
            Some("NET".into())
        );
        assert_eq!(
            code(resolve_system("", Some("net-hw-002"), &dir())),
            Some("NET".into())
        );
        assert_eq!(code(resolve_system("", None, &dir())), None, "沒有資產說明");
    }

    #[test]
    fn system_given_by_name_or_code() {
        assert_eq!(
            code(resolve_system("網路服務", None, &dir())),
            Some("NET".into())
        );
        assert_eq!(
            code(resolve_system("sys", Some("SYS-HW-003"), &dir())),
            Some("SYS".into())
        );
    }

    #[test]
    fn isms_id_from_another_system() {
        let e = resolve_system("NET", Some("SYS-HW-003"), &dir()).unwrap_err();
        assert_eq!(e.field, StandardField::IsmsId);
        assert_eq!(e.kind, FieldErrorKind::SystemMismatch);
        assert_eq!(
            e.message,
            "ISMS 編號「SYS-HW-003」的系統代碼 SYS 與資產說明 NET 不符"
        );
    }

    #[test]
    fn unknown_system() {
        let e = resolve_system("LAB", None, &dir()).unwrap_err();
        assert_eq!(e.field, StandardField::System);
        assert_eq!(e.message, "資產說明「LAB」尚未定義");
        let e = resolve_system("", Some("LAB-HW-001"), &dir()).unwrap_err();
        assert_eq!(e.field, StandardField::IsmsId);
        assert!(e.message.contains("LAB 尚未定義"), "{e:?}");
    }

    #[test]
    fn compliance_and_personal_data() {
        let mut errors = Vec::new();
        assert_eq!(parse_compliance("", &mut errors), None);
        assert_eq!(parse_compliance("4", &mut errors), Some(4));
        assert_eq!(parse_compliance("5", &mut errors), None);
        assert_eq!(errors[0].message, "法規遵循性「5」必須是 1–4");
        let classes = crate::assets::AssetClasses::builtin();
        let hw05 = classes.all().iter().find(|c| c.code.as_str() == "HW05");
        let mut errors = Vec::new();
        assert_eq!(parse_personal_data("", hw05, &mut errors), Some(false));
        assert_eq!(parse_personal_data("是", hw05, &mut errors), Some(true));
        assert_eq!(parse_personal_data("ｙ", hw05, &mut errors), Some(true));
        assert_eq!(parse_personal_data("N", hw05, &mut errors), Some(false));
        assert_eq!(parse_personal_data("maybe", hw05, &mut errors), None);
        assert_eq!(
            errors[0].message,
            "是否含個人資料「maybe」只能填 Y、N、是或否"
        );
    }
}
