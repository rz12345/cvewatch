//! 單筆資產欄位的驗證（清冊匯入與畫面編修共用，13 design D4、D5）：必填欄位、子類別、
//! 曝露度、C/I/A（空白帶入子類別範本值）與 ISMS 編號；ISMS 管理欄位見 [`super::record_admin`]。

use super::classes::{AssetClass, AssetClasses, Cia, ClassCode, MAX_RATING, MIN_RATING};
use super::columns::StandardField;
use super::model::{AssetRecord, Exposure};
use super::record_admin::{parse_compliance, parse_personal_data, resolve_system};
use super::record_identity::parse_identity;
use super::systems::SystemDirectory;

/// 單筆資產欄位的原始輸入（清冊儲存格或畫面輸入框）；前後空白在驗證時去除。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordInput<'a> {
    pub asset_tag: &'a str,
    pub name: &'a str,
    /// 子類別代碼或名稱（清冊匯入另接受舊版值）
    pub category: &'a str,
    /// 空白時為預設 internal
    pub exposure: &'a str,
    /// C/I/A 空白時帶入子類別範本值
    pub confidentiality: &'a str,
    pub integrity: &'a str,
    pub availability: &'a str,
    /// 空白表示沒有 ISMS 編號
    pub isms_id: &'a str,
    /// 資產說明代碼或名稱；空白時由 ISMS 編號的系統代碼帶出
    pub system: &'a str,
    pub responsible_unit: &'a str,
    pub custodian: &'a str,
    pub user_unit: &'a str,
    /// 空白表示未評定
    pub compliance: &'a str,
    /// Y／N／是／否；空白時帶入子類別預設值
    pub personal_data: &'a str,
    /// 識別欄位（20 design D2）
    pub brand: &'a str,
    pub model: &'a str,
    pub serial_number: &'a str,
    pub property_number: &'a str,
    pub hostname: &'a str,
    pub management_url: &'a str,
    pub location: &'a str,
    pub notes: &'a str,
}

impl RecordInput<'_> {
    /// 欄位的原始輸入；product、version 與停用欄位不屬於資產欄位，回傳空字串。
    pub fn value(&self, field: StandardField) -> &str {
        match field {
            StandardField::AssetTag => self.asset_tag,
            StandardField::Name => self.name,
            StandardField::Category => self.category,
            StandardField::Exposure => self.exposure,
            StandardField::Confidentiality => self.confidentiality,
            StandardField::Integrity => self.integrity,
            StandardField::Availability => self.availability,
            StandardField::IsmsId => self.isms_id,
            StandardField::System => self.system,
            StandardField::ResponsibleUnit => self.responsible_unit,
            StandardField::Custodian | StandardField::Owner => self.custodian,
            StandardField::UserUnit => self.user_unit,
            StandardField::Compliance => self.compliance,
            StandardField::PersonalData => self.personal_data,
            StandardField::Brand => self.brand,
            StandardField::Model => self.model,
            StandardField::SerialNumber => self.serial_number,
            StandardField::PropertyNumber => self.property_number,
            StandardField::Hostname => self.hostname,
            StandardField::ManagementUrl => self.management_url,
            StandardField::Location => self.location,
            StandardField::Notes => self.notes,
            StandardField::Product
            | StandardField::Version
            | StandardField::Criticality
            | StandardField::Code
            | StandardField::Ip
            | StandardField::Purpose => "",
        }
    }
}

/// 資產欄位中的必填欄位（依驗證與錯誤回報的順序）。
pub const REQUIRED_FIELDS: [StandardField; 3] = [
    StandardField::AssetTag,
    StandardField::Name,
    StandardField::Category,
];

/// C/I/A 欄位與介面名稱（依回報順序）。
pub const RATING_FIELDS: [(StandardField, &str); 3] = [
    (StandardField::Confidentiality, "機密性"),
    (StandardField::Integrity, "完整性"),
    (StandardField::Availability, "可用性"),
];

/// 欄位錯誤的種類（讓畫面以自己的用語呈現，不必解析 `message`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldErrorKind {
    /// 必填欄位空白（含沒有範本值的子類別未填 C/I/A）
    Missing,
    /// 不是允許的值（category、exposure）或格式不符（ISMS 編號）
    NotAllowed,
    /// 超出範圍或不是數字（C/I/A）
    OutOfRange,
    /// ISMS 編號的大類與資產類別不符
    ClassMismatch,
    /// ISMS 編號的系統代碼與資產說明不符
    SystemMismatch,
    /// 資產說明（或 ISMS 編號的系統代碼）尚未定義
    UnknownSystem,
    /// 格式不符（主機名稱、管理介面位址；20 design D2）
    Format,
    /// 管理介面位址含帳號或密碼
    Credentials,
    /// 超過長度上限（主機名稱）
    TooLong,
}

/// 單一欄位的驗證錯誤；`message` 不含列位置（匯入時加上「第 N 列：」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: StandardField,
    pub kind: FieldErrorKind,
    pub message: String,
}

impl FieldError {
    pub(super) fn new(field: StandardField, kind: FieldErrorKind, message: String) -> FieldError {
        FieldError {
            field,
            kind,
            message,
        }
    }
}

/// 驗證通過的資產欄位；`legacy` 為由舊版 category 值轉換時的原值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidRecord {
    pub record: AssetRecord,
    pub legacy: Option<String>,
}

pub(super) fn optional_text(v: &str) -> Option<String> {
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// 驗證時參考的設定：子類別表與資產說明目錄（16 design D2）。
#[derive(Debug, Clone, Copy)]
pub struct RecordContext<'a> {
    pub classes: &'a AssetClasses,
    pub systems: &'a SystemDirectory,
}

/// 驗證資產欄位（畫面編修）：類別只接受已定義子類別的代碼或名稱，資產說明須已定義；
/// 錯誤依欄位順序回報。
pub fn validate_record(
    input: &RecordInput<'_>,
    ctx: &RecordContext<'_>,
) -> Result<AssetRecord, Vec<FieldError>> {
    let valid = validate_with(input, ctx.classes, false);
    // 其他欄位有錯時仍檢查資產說明（一次列出全部錯誤）；ISMS 編號本身有錯時不再比對系統代碼
    let isms_ok = match &valid {
        Ok(_) => true,
        Err(errors) => errors.iter().all(|e| e.field != StandardField::IsmsId),
    };
    let isms = Some(input.isms_id.trim()).filter(|v| isms_ok && !v.is_empty());
    let system = resolve_system(input.system, isms, ctx.systems);
    match (valid, system) {
        (Ok(v), Ok(code)) => Ok(AssetRecord {
            system_code: code.map(|c| c.as_str().to_string()),
            ..v.record
        }),
        (Ok(_), Err(e)) => Err(vec![e]),
        (Err(errors), Ok(_)) => Err(errors),
        (Err(mut errors), Err(e)) => {
            // 資產說明錯誤依欄位順序放在 ISMS 編號之後
            let at = errors
                .iter()
                .position(|x| field_order(x.field) > field_order(e.field))
                .unwrap_or(errors.len());
            errors.insert(at, e);
            Err(errors)
        }
    }
}

fn field_order(field: StandardField) -> usize {
    StandardField::ALL
        .iter()
        .position(|f| *f == field)
        .unwrap_or(usize::MAX)
}

/// 驗證清冊的一列：另接受舊版 category 值（network、security…），轉換結果見
/// [`ValidRecord::legacy`]。資產說明在取得資料庫內容後才解析（`plan.rs`），此處
/// `system_code` 一律為 None。
pub fn validate_import_record(
    input: &RecordInput<'_>,
    classes: &AssetClasses,
) -> Result<ValidRecord, Vec<FieldError>> {
    validate_with(input, classes, true)
}

fn validate_with(
    input: &RecordInput<'_>,
    classes: &AssetClasses,
    allow_legacy: bool,
) -> Result<ValidRecord, Vec<FieldError>> {
    let mut errors = Vec::new();
    for field in REQUIRED_FIELDS {
        if input.value(field).trim().is_empty() {
            errors.push(FieldError::new(
                field,
                FieldErrorKind::Missing,
                format!("缺少 {}", field.as_str()),
            ));
        }
    }
    let resolved = parse_class(input.category.trim(), classes, allow_legacy, &mut errors);
    let exposure = parse_exposure(input.exposure.trim(), &mut errors);
    let class = resolved.as_ref().and_then(|r| classes.get(&r.code));
    let cia = parse_ratings(input, class, &mut errors);
    let isms_id = parse_isms_id(input.isms_id.trim(), class.map(|c| &c.code), &mut errors);
    let compliance = parse_compliance(input.compliance.trim(), &mut errors);
    let personal_data = parse_personal_data(input.personal_data.trim(), class, &mut errors);
    let identity = parse_identity(input, &mut errors);
    match (errors.is_empty(), resolved, cia, personal_data) {
        (true, Some(resolved), Some(cia), Some(personal_data)) => Ok(ValidRecord {
            record: AssetRecord {
                asset_tag: input.asset_tag.trim().to_string(),
                name: input.name.trim().to_string(),
                category: resolved.code,
                location: optional_text(input.location),
                exposure: exposure.unwrap_or_default(),
                cia,
                isms_id,
                system_code: None,
                responsible_unit: optional_text(input.responsible_unit),
                custodian: optional_text(input.custodian),
                user_unit: optional_text(input.user_unit),
                compliance,
                personal_data,
                brand: identity.brand,
                model: identity.model,
                serial_number: identity.serial_number,
                property_number: identity.property_number,
                hostname: identity.hostname,
                management_url: identity.management_url,
                notes: optional_text(input.notes),
            },
            legacy: resolved.legacy,
        }),
        _ => Err(errors),
    }
}

/// 空白回傳 None（必填錯誤已記錄）；不屬於已定義子類別時記錄錯誤。
fn parse_class(
    value: &str,
    classes: &AssetClasses,
    allow_legacy: bool,
    errors: &mut Vec<FieldError>,
) -> Option<super::classes::Resolved> {
    if value.is_empty() {
        return None;
    }
    let resolved = classes.resolve(value, allow_legacy);
    if resolved.is_none() {
        errors.push(FieldError::new(
            StandardField::Category,
            FieldErrorKind::NotAllowed,
            format!(
                "category「{value}」不是允許的值（子類別代碼 {} 或子類別名稱）",
                classes.allowed()
            ),
        ));
    }
    resolved
}

fn parse_exposure(value: &str, errors: &mut Vec<FieldError>) -> Option<Exposure> {
    if value.is_empty() {
        return None;
    }
    let parsed = Exposure::parse(value);
    if parsed.is_none() {
        errors.push(FieldError::new(
            StandardField::Exposure,
            FieldErrorKind::NotAllowed,
            format!("exposure「{value}」不是允許的值（{}）", Exposure::allowed()),
        ));
    }
    parsed
}

/// 「機密性、完整性與可用性」「完整性與可用性」「可用性」。
fn join_labels(labels: &[&str]) -> String {
    match labels {
        [] => String::new(),
        [one] => one.to_string(),
        [init @ .., last] => format!("{}與{last}", init.join("、")),
    }
}

/// C/I/A：有填時必須是 1–4；空白帶入子類別範本值。子類別沒有範本值時空白的項目為必填
/// （每個空白欄位各一筆錯誤，訊息相同）。類別無效時只檢查範圍，回傳 None。
fn parse_ratings(
    input: &RecordInput<'_>,
    class: Option<&AssetClass>,
    errors: &mut Vec<FieldError>,
) -> Option<Cia> {
    let mut values = [None; 3];
    for (i, (field, label)) in RATING_FIELDS.iter().enumerate() {
        let raw = input.value(*field).trim();
        if raw.is_empty() {
            continue;
        }
        match raw.parse::<u8>() {
            Ok(n) if (MIN_RATING..=MAX_RATING).contains(&n) => values[i] = Some(n),
            _ => errors.push(FieldError::new(
                *field,
                FieldErrorKind::OutOfRange,
                format!("{label}「{raw}」必須是 {MIN_RATING}–{MAX_RATING}"),
            )),
        }
    }
    let class = class?;
    let template = class.template.map(|t| t.to_array());
    let blank: Vec<usize> = (0..3)
        .filter(|&i| input.value(RATING_FIELDS[i].0).trim().is_empty())
        .collect();
    let Some(template) = template else {
        if !blank.is_empty() {
            let labels: Vec<&str> = blank.iter().map(|&i| RATING_FIELDS[i].1).collect();
            let message = format!("{}必須填寫{}", class.label(), join_labels(&labels));
            for &i in &blank {
                errors.push(FieldError::new(
                    RATING_FIELDS[i].0,
                    FieldErrorKind::Missing,
                    message.clone(),
                ));
            }
            return None;
        }
        return Some(Cia::new(values[0]?, values[1]?, values[2]?));
    };
    let pick = |i: usize| -> Option<u8> {
        if blank.contains(&i) {
            Some(template[i])
        } else {
            values[i]
        }
    };
    Some(Cia::new(pick(0)?, pick(1)?, pick(2)?))
}

/// ISMS 編號格式 `系統代碼(3 英數字)-大類(HW|SW)-流水號(3 數字)`；回傳大類部分（大寫）。
fn isms_major(id: &str) -> Option<String> {
    let mut parts = id.split('-');
    let (system, major, serial) = (parts.next()?, parts.next()?, parts.next()?);
    let ok = parts.next().is_none()
        && system.len() == 3
        && system.bytes().all(|b| b.is_ascii_alphanumeric())
        && super::classes::MajorClass::parse(major).is_some()
        && major.len() == 2
        && serial.len() == 3
        && serial.bytes().all(|b| b.is_ascii_digit());
    ok.then(|| major.to_ascii_uppercase())
}

/// 空白為 None；格式不符或大類與子類別不符時記錄錯誤。保留使用者寫法（已去除前後空白）。
fn parse_isms_id(
    value: &str,
    class: Option<&ClassCode>,
    errors: &mut Vec<FieldError>,
) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let Some(major) = isms_major(value) else {
        errors.push(FieldError::new(
            StandardField::IsmsId,
            FieldErrorKind::NotAllowed,
            format!("ISMS 編號「{value}」格式應為「系統代碼-大類-流水號」，例如 SYS-HW-001"),
        ));
        return None;
    };
    if let Some(code) = class {
        if code.major().as_str() != major {
            errors.push(FieldError::new(
                StandardField::IsmsId,
                FieldErrorKind::ClassMismatch,
                format!("ISMS 編號「{value}」的大類 {major} 與資產類別 {code} 不符"),
            ));
            return None;
        }
    }
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes() -> AssetClasses {
        AssetClasses::builtin()
    }

    fn input<'a>() -> RecordInput<'a> {
        RecordInput {
            asset_tag: "FW-02",
            name: "fw02",
            category: "HW03",
            ..RecordInput::default()
        }
    }

    fn systems() -> SystemDirectory {
        let sys = |code: &str, name: &str| crate::assets::AssetSystem {
            code: crate::assets::SystemCode::parse(code).unwrap(),
            name: name.into(),
        };
        SystemDirectory::new(vec![sys("NET", "網路服務"), sys("SYS", "核心系統")])
    }

    fn check(i: RecordInput<'_>) -> Result<AssetRecord, Vec<FieldError>> {
        let classes = classes();
        let systems = systems();
        validate_record(
            &i,
            &RecordContext {
                classes: &classes,
                systems: &systems,
            },
        )
    }

    #[test]
    fn administrative_fields_are_recorded() {
        let r = check(RecordInput {
            responsible_unit: " 網路組 ",
            custodian: "王小明",
            user_unit: "",
            compliance: "4",
            personal_data: "是",
            system: "網路服務",
            isms_id: "NET-HW-002",
            ..input()
        })
        .unwrap();
        assert_eq!(r.responsible_unit.as_deref(), Some("網路組"));
        assert_eq!(r.custodian.as_deref(), Some("王小明"));
        assert_eq!(r.user_unit, None);
        assert_eq!(r.compliance, Some(4));
        assert!(r.personal_data);
        assert_eq!(r.system_code.as_deref(), Some("NET"));
        assert_eq!(r.protection_level(), Some(4));
        let plain = check(input()).unwrap();
        assert_eq!(plain.system_code, None);
        assert!(!plain.personal_data, "HW03 預設不含個人資料");
        assert_eq!(plain.protection_level(), None, "未評定");
    }

    #[test]
    fn system_mismatch_is_reported_with_other_errors() {
        let errors = check(RecordInput {
            name: "",
            system: "NET",
            isms_id: "SYS-HW-001",
            ..input()
        })
        .expect_err("應驗證失敗");
        let fields: Vec<_> = errors.iter().map(|e| (e.field, e.kind)).collect();
        assert_eq!(
            fields,
            vec![
                (StandardField::Name, FieldErrorKind::Missing),
                (StandardField::IsmsId, FieldErrorKind::SystemMismatch),
            ]
        );
        let malformed = check(RecordInput {
            system: "NET",
            isms_id: "SYSHW001",
            ..input()
        })
        .expect_err("應驗證失敗");
        assert_eq!(
            malformed.len(),
            1,
            "格式錯誤時不再比對系統代碼：{malformed:?}"
        );
    }

    #[test]
    fn system_error_follows_isms_field_order() {
        let errors = check(RecordInput {
            name: "",
            system: "LAB",
            compliance: "9",
            ..input()
        })
        .expect_err("應驗證失敗");
        let order: Vec<_> = errors.iter().map(|e| e.field).collect();
        assert_eq!(
            order,
            vec![
                StandardField::Name,
                StandardField::System,
                StandardField::Compliance
            ]
        );
    }

    fn fields(r: Result<AssetRecord, Vec<FieldError>>) -> Vec<(StandardField, String)> {
        r.expect_err("應驗證失敗")
            .into_iter()
            .map(|e| (e.field, e.message))
            .collect()
    }

    #[test]
    fn empty_asset_tag_is_reported_on_its_field() {
        let errors = fields(check(RecordInput {
            asset_tag: "  ",
            ..input()
        }));
        assert_eq!(
            errors,
            vec![(StandardField::AssetTag, "缺少 asset_tag".to_string())]
        );
    }

    #[test]
    fn errors_carry_their_kind_in_field_order() {
        let errors = check(RecordInput {
            name: "",
            category: "router",
            availability: "9",
            isms_id: "SYSHW001",
            ..input()
        })
        .expect_err("應驗證失敗");
        let kinds: Vec<_> = errors.iter().map(|e| (e.field, e.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                (StandardField::Name, FieldErrorKind::Missing),
                (StandardField::Category, FieldErrorKind::NotAllowed),
                (StandardField::Availability, FieldErrorKind::OutOfRange),
                (StandardField::IsmsId, FieldErrorKind::NotAllowed),
            ]
        );
        let empty = RecordInput::default();
        assert!(REQUIRED_FIELDS.iter().all(|f| empty.value(*f).is_empty()));
    }

    #[test]
    fn invalid_exposure_lists_allowed_values() {
        let errors = fields(check(RecordInput {
            exposure: "public",
            ..input()
        }));
        assert_eq!(
            errors,
            vec![(
                StandardField::Exposure,
                "exposure「public」不是允許的值（internet、dmz、internal、mgmt）".to_string()
            )]
        );
    }

    #[test]
    fn class_by_name_or_lower_case_code() {
        let r = check(RecordInput {
            category: "資安設備",
            ..input()
        })
        .unwrap();
        assert_eq!(r.category.as_str(), "HW03");
        let r = check(RecordInput {
            category: "hw05",
            ..input()
        })
        .unwrap();
        assert_eq!(r.category.as_str(), "HW05");
        for bad in ["IF01", "printer", "security"] {
            let e = fields(check(RecordInput {
                category: bad,
                ..input()
            }));
            assert_eq!(e[0].0, StandardField::Category, "{bad}");
            assert!(e[0].1.contains(bad), "{e:?}");
        }
    }

    #[test]
    fn legacy_category_only_on_import() {
        let v = validate_import_record(
            &RecordInput {
                category: "security",
                ..input()
            },
            &classes(),
        )
        .unwrap();
        assert_eq!(v.record.category.as_str(), "HW03");
        assert_eq!(v.legacy.as_deref(), Some("security"));
    }

    #[test]
    fn template_values_for_a_server() {
        let r = check(RecordInput {
            category: "HW01",
            ..input()
        })
        .unwrap();
        assert_eq!(r.cia, Cia::new(2, 3, 4));
        assert_eq!(r.cia.value(), 4);
        assert_eq!(r.exposure, Exposure::Internal);
        assert_eq!(r.isms_id, None);
    }

    #[test]
    fn explicit_values_override_the_template() {
        let r = check(RecordInput {
            category: "HW05",
            confidentiality: "4",
            ..input()
        })
        .unwrap();
        assert_eq!(r.cia, Cia::new(4, 3, 4));
        assert_eq!(r.cia.value(), 4);
    }

    #[test]
    fn value_out_of_range() {
        let errors = fields(check(RecordInput {
            availability: "5",
            ..input()
        }));
        assert_eq!(
            errors,
            vec![(
                StandardField::Availability,
                "可用性「5」必須是 1–4".to_string()
            )]
        );
        let errors = fields(check(RecordInput {
            confidentiality: "x",
            ..input()
        }));
        assert_eq!(errors[0].0, StandardField::Confidentiality);
    }

    #[test]
    fn application_system_without_ratings() {
        let errors = check(RecordInput {
            category: "SW01",
            ..input()
        })
        .expect_err("SW01 必填");
        let fields: Vec<_> = errors.iter().map(|e| (e.field, e.kind)).collect();
        assert_eq!(
            fields,
            RATING_FIELDS
                .iter()
                .map(|(f, _)| (*f, FieldErrorKind::Missing))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            errors[0].message,
            "SW01 應用系統必須填寫機密性、完整性與可用性"
        );
        let errors = check(RecordInput {
            category: "SW01",
            confidentiality: "3",
            ..input()
        })
        .unwrap_err();
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].message, "SW01 應用系統必須填寫完整性與可用性");
        let r = check(RecordInput {
            category: "SW01",
            confidentiality: "3",
            integrity: "4",
            availability: "2",
            ..input()
        })
        .unwrap();
        assert_eq!(r.cia, Cia::new(3, 4, 2));
    }

    #[test]
    fn valid_isms_id_keeps_the_written_form() {
        let r = check(RecordInput {
            isms_id: " sys-hw-003 ",
            ..input()
        })
        .unwrap();
        assert_eq!(r.isms_id.as_deref(), Some("sys-hw-003"));
        assert_eq!(r.asset_tag, "FW-02");
    }

    #[test]
    fn malformed_isms_id() {
        for bad in [
            "SYSHW001",
            "AL-HW-001",
            "SYS-IF-001",
            "SYS-HW-01",
            "SYS-HW-001-1",
        ] {
            let errors = fields(check(RecordInput {
                isms_id: bad,
                ..input()
            }));
            assert_eq!(
                errors,
                vec![(
                    StandardField::IsmsId,
                    format!("ISMS 編號「{bad}」格式應為「系統代碼-大類-流水號」，例如 SYS-HW-001")
                )],
                "{bad}"
            );
        }
    }

    #[test]
    fn major_class_mismatch() {
        let errors = check(RecordInput {
            isms_id: "SYS-SW-003",
            ..input()
        })
        .unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].kind, FieldErrorKind::ClassMismatch);
        assert_eq!(
            errors[0].message,
            "ISMS 編號「SYS-SW-003」的大類 SW 與資產類別 HW03 不符"
        );
    }

    #[test]
    fn defaults_and_trimming() {
        let record = check(RecordInput {
            asset_tag: " FW-02 ",
            location: "  ",
            custodian: " 網管組 ",
            ..input()
        })
        .expect("valid");
        assert_eq!(record.asset_tag, "FW-02");
        assert_eq!(record.location, None);
        assert_eq!(record.custodian.as_deref(), Some("網管組"));
    }
}
