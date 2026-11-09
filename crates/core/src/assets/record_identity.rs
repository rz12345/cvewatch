//! 資產識別欄位的驗證（20 design D2）：廠牌、型號、序號、財產編號、主機名稱與管理介面位址。
//! 清冊匯入與畫面編修共用；錯誤以 [`FieldError`] 回報，畫面依種類換成介面用語。
//! 財產編號的唯一性要查資料庫，不在此檢查（20 design D3）。

use super::columns::StandardField;
use super::record::{optional_text, FieldError, FieldErrorKind, RecordInput};

/// 主機名稱的最大長度（DNS 名稱上限）。
pub const MAX_HOSTNAME_LEN: usize = 253;

/// 驗證通過的識別欄位（空白為 None）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Identity {
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub property_number: Option<String>,
    pub hostname: Option<String>,
    pub management_url: Option<String>,
}

/// 解析識別欄位；主機名稱與管理介面位址不合法時記錄錯誤（該欄回傳 None）。
pub(super) fn parse_identity(input: &RecordInput<'_>, errors: &mut Vec<FieldError>) -> Identity {
    Identity {
        brand: optional_text(input.brand),
        model: optional_text(input.model),
        serial_number: optional_text(input.serial_number),
        property_number: optional_text(input.property_number),
        hostname: parse_hostname(input.hostname.trim(), errors),
        management_url: parse_management_url(input.management_url.trim(), errors),
    }
}

/// 主機名稱的字元是否允許（ASCII 英數字、`-`、`.`、`_`）。
fn hostname_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_')
}

/// 主機名稱：空白為 None；只接受英數字、`-`、`.`、`_`，最長 253 字元，保留大小寫。
fn parse_hostname(value: &str, errors: &mut Vec<FieldError>) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let (kind, message) = if !value.chars().all(hostname_char) {
        (
            FieldErrorKind::Format,
            format!("hostname「{value}」只能包含英數字、-、. 與 _"),
        )
    } else if value.len() > MAX_HOSTNAME_LEN {
        (
            FieldErrorKind::TooLong,
            format!("hostname 不可超過 {MAX_HOSTNAME_LEN} 個字元"),
        )
    } else {
        return Some(value.to_string());
    };
    errors.push(FieldError::new(StandardField::Hostname, kind, message));
    None
}

/// 管理介面位址的檢查結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlProblem {
    /// 不是 http／https、沒有主機，或含空白與控制字元
    Format,
    /// 含帳號或密碼
    Credentials,
}

/// 檢查管理介面位址（已去除前後空白、非空白）；畫面即時檢查與驗證共用。
pub fn check_management_url(value: &str) -> Result<(), UrlProblem> {
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(UrlProblem::Format);
    }
    let url = url::Url::parse(value).map_err(|_| UrlProblem::Format)?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return Err(UrlProblem::Format);
    }
    // 原文要以 http(s):// 開頭（`http:192.0.2.1` 之類的寫法 url crate 也會接受）
    let lower = value.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(UrlProblem::Format);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(UrlProblem::Credentials);
    }
    Ok(())
}

/// 管理介面位址：空白為 None；以使用者寫法儲存（不存 `Url::to_string()`，避免補上結尾 `/`）。
/// 錯誤訊息不含可能帶有密碼的原文。
fn parse_management_url(value: &str, errors: &mut Vec<FieldError>) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let (kind, message) = match check_management_url(value) {
        Ok(()) => return Some(value.to_string()),
        Err(UrlProblem::Credentials) => (
            FieldErrorKind::Credentials,
            "management_url 含帳號或密碼；帳號與密碼不得寫入管理介面位址".to_string(),
        ),
        Err(UrlProblem::Format) if value.contains('@') => (
            FieldErrorKind::Format,
            "management_url 需以 http:// 或 https:// 開頭並包含主機".to_string(),
        ),
        Err(UrlProblem::Format) => (
            FieldErrorKind::Format,
            format!("management_url「{value}」需以 http:// 或 https:// 開頭並包含主機"),
        ),
    };
    errors.push(FieldError::new(StandardField::ManagementUrl, kind, message));
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::classes::AssetClasses;
    use crate::assets::record::validate_import_record;

    fn base<'a>() -> RecordInput<'a> {
        RecordInput {
            asset_tag: "FW-EDGE",
            name: "fw-edge",
            category: "HW03",
            ..RecordInput::default()
        }
    }

    fn errors(input: RecordInput<'_>) -> Vec<(StandardField, FieldErrorKind, String)> {
        validate_import_record(&input, &AssetClasses::builtin())
            .expect_err("應驗證失敗")
            .into_iter()
            .map(|e| (e.field, e.kind, e.message))
            .collect()
    }

    #[test]
    fn identity_fields_are_trimmed() {
        let v = validate_import_record(
            &RecordInput {
                brand: " Aruba ",
                model: "XYZ-100",
                serial_number: "SN0001",
                property_number: " P-0001 ",
                hostname: " fw-edge ",
                management_url: "  ",
                ..base()
            },
            &AssetClasses::builtin(),
        )
        .unwrap();
        let r = v.record;
        assert_eq!(r.brand.as_deref(), Some("Aruba"));
        assert_eq!(r.model.as_deref(), Some("XYZ-100"));
        assert_eq!(r.serial_number.as_deref(), Some("SN0001"));
        assert_eq!(r.property_number.as_deref(), Some("P-0001"));
        assert_eq!(r.hostname.as_deref(), Some("fw-edge"));
        assert_eq!(r.management_url, None);
    }

    #[test]
    fn valid_hostname_keeps_letter_case() {
        let mut e = Vec::new();
        assert_eq!(
            parse_hostname("SRV-01.lab.example", &mut e).as_deref(),
            Some("SRV-01.lab.example")
        );
        assert_eq!(
            parse_hostname("WIN_FILE01", &mut e).as_deref(),
            Some("WIN_FILE01")
        );
        assert!(e.is_empty());
        let longest = "a".repeat(MAX_HOSTNAME_LEN);
        assert!(parse_hostname(&longest, &mut e).is_some());
        assert!(e.is_empty());
    }

    #[test]
    fn url_in_the_hostname_field() {
        assert_eq!(
            errors(RecordInput {
                hostname: "https://fw-edge",
                ..base()
            }),
            vec![(
                StandardField::Hostname,
                FieldErrorKind::Format,
                "hostname「https://fw-edge」只能包含英數字、-、. 與 _".to_string()
            )]
        );
        for bad in ["fw edge", "fw01,fw02", "fw01:443", "admin@fw01", "防火牆"] {
            let e = errors(RecordInput {
                hostname: bad,
                ..base()
            });
            assert_eq!(e[0].1, FieldErrorKind::Format, "{bad}");
        }
    }

    #[test]
    fn hostname_longer_than_253_is_rejected() {
        let long = "a".repeat(MAX_HOSTNAME_LEN + 1);
        let e = errors(RecordInput {
            hostname: &long,
            ..base()
        });
        assert_eq!(
            e,
            vec![(
                StandardField::Hostname,
                FieldErrorKind::TooLong,
                "hostname 不可超過 253 個字元".to_string()
            )]
        );
    }

    #[test]
    fn https_management_address_keeps_the_written_form() {
        let mut e = Vec::new();
        assert_eq!(
            parse_management_url("https://192.0.2.1:8443/", &mut e).as_deref(),
            Some("https://192.0.2.1:8443/")
        );
        assert_eq!(
            parse_management_url("http://192.0.2.1", &mut e).as_deref(),
            Some("http://192.0.2.1"),
            "不補結尾的 /"
        );
        assert_eq!(
            parse_management_url("HTTPS://fw-edge.lab.example/login", &mut e).as_deref(),
            Some("HTTPS://fw-edge.lab.example/login")
        );
        assert!(e.is_empty(), "{e:?}");
    }

    #[test]
    fn address_without_a_scheme() {
        for bad in [
            "192.0.2.1",
            "ftp://192.0.2.1",
            "http://",
            "https://fw edge/",
            "http:192.0.2.1",
        ] {
            let e = errors(RecordInput {
                management_url: bad,
                ..base()
            });
            assert_eq!(e.len(), 1, "{bad}");
            assert_eq!(e[0].0, StandardField::ManagementUrl, "{bad}");
            assert_eq!(e[0].1, FieldErrorKind::Format, "{bad}");
            assert!(e[0].2.contains("需以 http:// 或 https:// 開頭"), "{e:?}");
        }
    }

    #[test]
    fn credentials_in_the_address() {
        for bad in [
            "https://admin:secret@192.0.2.1/",
            "https://admin@192.0.2.1/",
            "http://:secret@192.0.2.1/",
        ] {
            let e = errors(RecordInput {
                management_url: bad,
                ..base()
            });
            assert_eq!(e.len(), 1, "{bad}");
            assert_eq!(e[0].1, FieldErrorKind::Credentials, "{bad}");
            assert!(e[0].2.contains("帳號與密碼不得寫入管理介面位址"));
            assert!(!e[0].2.contains("secret"), "訊息不可帶出密碼：{}", e[0].2);
        }
        let e = errors(RecordInput {
            management_url: "admin:secret@192.0.2.1",
            ..base()
        });
        assert_eq!(e[0].1, FieldErrorKind::Format);
        assert!(!e[0].2.contains("secret"), "{}", e[0].2);
    }
}
