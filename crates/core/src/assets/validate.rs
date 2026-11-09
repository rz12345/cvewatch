//! 清冊驗證：逐列驗證資產欄位（[`super::record`]）、重複 asset_tag 與 ISMS 編號、元件工作表。
//! 所有錯誤彙整後一次回報，匯入整批中止；舊版 category 值與停用欄位只產生 notice。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;

use super::classes::AssetClasses;
use super::columns::{ColumnMap, SheetKind, StandardField, KEEPABLE_FIELDS};
use super::model::AssetRecord;
use super::sheet::{Sheet, SheetRow, Workbook};
use super::validate_eol::parse_eol;
use super::validate_ips::parse_ips;
use super::validate_systems::parse_systems;
use crate::error::CoreError;

pub use super::validate_eol::{EolInput, IncomingEol};
pub use super::validate_ips::IpsInput;
pub use super::validate_systems::IncomingSystem;

pub use super::record::{
    validate_import_record, validate_record, FieldError, FieldErrorKind, RecordContext,
    RecordInput, ValidRecord, RATING_FIELDS, REQUIRED_FIELDS,
};
pub use super::record_admin::{
    compliance_value, isms_system_code, protection_level, resolve_system,
};

/// 錯誤訊息中的列位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowRef {
    pub sheet: SheetKind,
    pub number: usize,
}

impl fmt::Display for RowRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.sheet {
            SheetKind::Assets => write!(f, "第 {} 列", self.number),
            SheetKind::Components => write!(f, "components 工作表第 {} 列", self.number),
            SheetKind::Systems => write!(f, "systems 工作表第 {} 列", self.number),
            SheetKind::Ips => write!(f, "ips 工作表第 {} 列", self.number),
            SheetKind::Eol => write!(f, "eol 工作表第 {} 列", self.number),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingComponent {
    pub row: RowRef,
    pub product_raw: String,
    pub version_raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingAsset {
    pub row: RowRef,
    /// `system_code` 一律為 None：資產說明在取得資料庫內容後由 `plan.rs` 解析（16 design D2）
    pub record: AssetRecord,
    /// `system` 欄的原文（代碼或名稱；沒有該欄或空白時為空字串）
    pub system_raw: String,
    /// 第一個為 assets 工作表的主要元件，其後為 components 工作表
    pub components: Vec<IncomingComponent>,
}

/// 不中止匯入的提醒（13 design D3）：舊版 category 值的轉換、停用欄位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportNotice {
    /// 整批的提醒（例如停用欄位）為 None
    pub row: Option<RowRef>,
    pub asset_tag: Option<String>,
    pub message: String,
}

impl fmt::Display for ImportNotice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.row, &self.asset_tag) {
            (Some(row), Some(tag)) => write!(f, "{row} {tag}：{}", self.message),
            (Some(row), None) => write!(f, "{row}：{}", self.message),
            (None, Some(tag)) => write!(f, "{tag}：{}", self.message),
            (None, None) => f.write_str(&self.message),
        }
    }
}

/// 停用的 `criticality` 欄（整批一次）。
pub const CRITICALITY_NOTICE: &str =
    "清冊的 criticality 欄已停用，改用 confidentiality、integrity、availability";

/// 清冊沒有 `isms_id` 欄（例如 13 之前的舊清冊）時保留既有資產的 ISMS 編號（整批一次）。
pub const ISMS_KEPT_NOTICE: &str = "清冊沒有 isms_id 欄，保留資料庫中既有資產的 ISMS 編號";

/// 舊欄名 `owner`（16 spec Legacy owner column；整批一次）。
pub const OWNER_NOTICE: &str = "owner 欄已改名為 custodian（保管人），已將其內容匯入為保管人";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Inventory {
    pub assets: Vec<IncomingAsset>,
    /// `systems` 工作表的資產說明（沒有該工作表時為空）
    pub systems: Vec<IncomingSystem>,
    pub notices: Vec<ImportNotice>,
    /// assets 工作表缺少的可保留欄位（[`KEEPABLE_FIELDS`]）：既有資產沿用資料庫中的值
    /// （不視為清空；16 design D4）
    pub kept_fields: BTreeSet<StandardField>,
    /// `ips` 工作表的 IP（20 design D5；預設為不處理）
    pub ips: IpsInput,
    /// `eol` 工作表的 EoL 覆寫（22 design D7；預設為不處理）
    pub eol: EolInput,
}

impl Inventory {
    /// 某欄是否因清冊沒有而保留既有值。
    pub fn keeps(&self, field: StandardField) -> bool {
        self.kept_fields.contains(&field)
    }

    /// 不處理生命週期欄位（ISMS 匯入：保留既有日期與覆寫，不提醒）。
    pub fn lifecycle_untouched(&self) -> bool {
        matches!(self.eol, EolInput::Untouched)
    }
}

/// 讀取一列的儲存格（依欄位位置；沒有該欄或空白為空字串，已去除前後空白）。
pub(super) struct RowReader<'a> {
    pub(super) row: &'a SheetRow,
    pub(super) at: RowRef,
    pub(super) columns: &'a HashMap<StandardField, usize>,
}

impl<'a> RowReader<'a> {
    pub(super) fn get(&self, field: StandardField) -> &'a str {
        self.columns
            .get(&field)
            .and_then(|&i| self.row.cells.get(i))
            .map_or("", |s| s.trim())
    }

    fn required(&self, field: StandardField, errors: &mut Vec<String>) -> String {
        let v = self.get(field);
        if v.is_empty() {
            errors.push(format!("{}：缺少 {}", self.at, field.as_str()));
        }
        v.to_string()
    }
}

/// 驗證整份清冊並轉成匯入資料；有任何錯誤時回傳 [`CoreError::ImportRejected`]。
pub fn parse_inventory(
    book: &Workbook,
    columns: &ColumnMap,
    classes: &AssetClasses,
) -> Result<Inventory, CoreError> {
    let mut errors = Vec::new();
    let asset_cols = columns.locate(&book.assets.headers, SheetKind::Assets)?;
    let mut notices: Vec<ImportNotice> = columns
        .deprecated_in(&book.assets.headers)
        .into_iter()
        .map(|_| ImportNotice {
            row: None,
            asset_tag: None,
            message: CRITICALITY_NOTICE.to_string(),
        })
        .collect();
    if columns.uses_legacy_owner(&book.assets.headers) {
        notices.push(ImportNotice {
            row: None,
            asset_tag: None,
            message: OWNER_NOTICE.to_string(),
        });
    }
    let systems = match &book.systems {
        Some(sheet) => {
            let cols = columns.locate(&sheet.headers, SheetKind::Systems)?;
            parse_systems(sheet, &cols, &mut errors)
        }
        None => Vec::new(),
    };
    let mut assets = parse_assets(
        &book.assets,
        &asset_cols,
        classes,
        &mut notices,
        &mut errors,
    );
    let known = column_values(&book.assets, &asset_cols, StandardField::AssetTag);
    if let Some(sheet) = &book.components {
        let comp_cols = columns.locate(&sheet.headers, SheetKind::Components)?;
        attach_components(sheet, &comp_cols, &known, &mut assets, &mut errors);
    }
    let ips = match &book.ips {
        Some(sheet) => {
            let cols = columns.locate(&sheet.headers, SheetKind::Ips)?;
            let incoming: HashSet<String> =
                assets.iter().map(|a| a.record.asset_tag.clone()).collect();
            IpsInput::Sheet(parse_ips(sheet, &cols, &incoming, &known, &mut errors))
        }
        None => IpsInput::Missing,
    };
    let eol = match &book.eol {
        Some(sheet) => {
            let cols = columns.locate(&sheet.headers, SheetKind::Eol)?;
            let incoming: HashSet<String> =
                assets.iter().map(|a| a.record.asset_tag.clone()).collect();
            EolInput::Sheet(parse_eol(sheet, &cols, &incoming, &known, &mut errors))
        }
        None => EolInput::Missing,
    };
    if !errors.is_empty() {
        return Err(CoreError::ImportRejected(errors));
    }
    let kept_fields = KEEPABLE_FIELDS
        .into_iter()
        .filter(|f| !asset_cols.contains_key(f))
        .collect();
    Ok(Inventory {
        assets,
        systems,
        notices,
        kept_fields,
        ips,
        eol,
    })
}

/// 同一份清冊內重複的值（asset_tag 區分大小寫，ISMS 編號與財產編號不分大小寫）；
/// 含驗證失敗的列。
#[derive(Default)]
struct SeenValues {
    tags: HashMap<String, usize>,
    isms: HashMap<String, usize>,
    property_numbers: HashMap<String, usize>,
}

impl SeenValues {
    fn check(&mut self, r: &RowReader<'_>, errors: &mut Vec<String>) {
        let at = r.at;
        let tag = r.get(StandardField::AssetTag);
        if !tag.is_empty() {
            if let Some(first) = self.tags.insert(tag.to_string(), r.row.number) {
                errors.push(format!("{at}：asset_tag「{tag}」與第 {first} 列重複"));
            }
        }
        let isms = r.get(StandardField::IsmsId);
        if !isms.is_empty() {
            if let Some(first) = self.isms.insert(isms.to_ascii_lowercase(), r.row.number) {
                errors.push(format!("{at}：ISMS 編號「{isms}」與第 {first} 列重複"));
            }
        }
        let property = r.get(StandardField::PropertyNumber);
        if !property.is_empty() {
            let key = property.to_ascii_lowercase();
            if let Some(first) = self.property_numbers.insert(key, r.row.number) {
                errors.push(format!("{at}：財產編號「{property}」與第 {first} 列重複"));
            }
        }
    }
}

fn parse_assets(
    sheet: &Sheet,
    columns: &HashMap<StandardField, usize>,
    classes: &AssetClasses,
    notices: &mut Vec<ImportNotice>,
    errors: &mut Vec<String>,
) -> Vec<IncomingAsset> {
    let mut seen = SeenValues::default();
    let mut out = Vec::new();
    for row in &sheet.rows {
        let at = RowRef {
            sheet: SheetKind::Assets,
            number: row.number,
        };
        let reader = RowReader { row, at, columns };
        let before = errors.len();
        let parsed = parse_asset_row(&reader, classes, notices, errors);
        seen.check(&reader, errors);
        if let (Some(parsed), true) = (parsed, errors.len() == before) {
            out.push(parsed);
        }
    }
    out
}

/// 一列的原始輸入（停用欄位不讀）。
fn row_input<'a>(r: &'a RowReader<'_>) -> RecordInput<'a> {
    RecordInput {
        asset_tag: r.get(StandardField::AssetTag),
        name: r.get(StandardField::Name),
        category: r.get(StandardField::Category),
        exposure: r.get(StandardField::Exposure),
        confidentiality: r.get(StandardField::Confidentiality),
        integrity: r.get(StandardField::Integrity),
        availability: r.get(StandardField::Availability),
        isms_id: r.get(StandardField::IsmsId),
        system: r.get(StandardField::System),
        responsible_unit: r.get(StandardField::ResponsibleUnit),
        custodian: r.get(StandardField::Custodian),
        user_unit: r.get(StandardField::UserUnit),
        compliance: r.get(StandardField::Compliance),
        personal_data: r.get(StandardField::PersonalData),
        brand: r.get(StandardField::Brand),
        model: r.get(StandardField::Model),
        serial_number: r.get(StandardField::SerialNumber),
        property_number: r.get(StandardField::PropertyNumber),
        hostname: r.get(StandardField::Hostname),
        management_url: r.get(StandardField::ManagementUrl),
        warranty_end: r.get(StandardField::WarrantyEnd),
        support_end: r.get(StandardField::SupportEnd),
        license_end: r.get(StandardField::LicenseEnd),
        location: r.get(StandardField::Location),
        notes: r.get(StandardField::Notes),
    }
}

fn parse_asset_row(
    r: &RowReader<'_>,
    classes: &AssetClasses,
    notices: &mut Vec<ImportNotice>,
    errors: &mut Vec<String>,
) -> Option<IncomingAsset> {
    let valid = match validate_import_record(&row_input(r), classes) {
        Ok(v) => Some(v),
        Err(field_errors) => {
            // 沒有範本值的子類別：每個空白的 C/I/A 各一筆錯誤但訊息相同，清冊只列一次
            let mut lines: Vec<String> = Vec::new();
            for e in field_errors {
                let line = format!("{}：{}", r.at, e.message);
                if !lines.contains(&line) {
                    lines.push(line);
                }
            }
            errors.extend(lines);
            None
        }
    };
    let product_raw = r.required(StandardField::Product, errors);
    // version 空白為版本待補（18 design D3）
    let version_raw = r.get(StandardField::Version).to_string();
    let valid = valid?;
    if let Some(old) = &valid.legacy {
        notices.push(legacy_notice(r.at, &valid.record, old, classes));
    }
    Some(IncomingAsset {
        row: r.at,
        record: valid.record,
        system_raw: r.get(StandardField::System).to_string(),
        components: vec![IncomingComponent {
            row: r.at,
            product_raw,
            version_raw,
        }],
    })
}

/// 「security 已轉為 HW03 資安設備」。
fn legacy_notice(
    at: RowRef,
    record: &AssetRecord,
    old: &str,
    classes: &AssetClasses,
) -> ImportNotice {
    let label = classes.label(&record.category);
    ImportNotice {
        row: Some(at),
        asset_tag: Some(record.asset_tag.clone()),
        message: format!("{old} 已轉為 {label}"),
    }
}

/// 工作表中某欄出現過的所有值（含驗證失敗的列）。
fn column_values(
    sheet: &Sheet,
    columns: &HashMap<StandardField, usize>,
    field: StandardField,
) -> HashSet<String> {
    let Some(&i) = columns.get(&field) else {
        return HashSet::new();
    };
    sheet
        .rows
        .iter()
        .filter_map(|r| r.cells.get(i))
        .map(|v| v.trim().to_string())
        .collect()
}

fn attach_components(
    sheet: &Sheet,
    columns: &HashMap<StandardField, usize>,
    known_tags: &HashSet<String>,
    assets: &mut [IncomingAsset],
    errors: &mut Vec<String>,
) {
    let index: HashMap<String, usize> = assets
        .iter()
        .enumerate()
        .map(|(i, a)| (a.record.asset_tag.clone(), i))
        .collect();
    for row in &sheet.rows {
        let at = RowRef {
            sheet: SheetKind::Components,
            number: row.number,
        };
        let reader = RowReader { row, at, columns };
        let before = errors.len();
        let tag = reader.required(StandardField::AssetTag, errors);
        let product_raw = reader.required(StandardField::Product, errors);
        let version_raw = reader.get(StandardField::Version).to_string();
        if errors.len() != before {
            continue;
        }
        match index.get(&tag) {
            Some(&i) => assets[i].components.push(IncomingComponent {
                row: at,
                product_raw,
                version_raw,
            }),
            // 該資產列本身有錯，錯誤已回報，不重複提示。
            None if known_tags.contains(&tag) => {}
            None => errors.push(format!("{at}：asset_tag「{tag}」不在 assets 工作表中")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(headers: &[&str], rows: &[&[&str]]) -> Sheet {
        Sheet {
            kind: SheetKind::Assets,
            headers: headers.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .enumerate()
                .map(|(i, r)| SheetRow {
                    number: i + 2,
                    cells: r.iter().map(|s| s.to_string()).collect(),
                })
                .collect(),
        }
    }

    fn parse(rows: &[&[&str]]) -> Result<Inventory, CoreError> {
        let book = Workbook {
            eol: None,
            assets: sheet(&HEADERS, rows),
            components: None,
            systems: None,
            ips: None,
        };
        parse_inventory(&book, &ColumnMap::default(), &AssetClasses::builtin())
    }

    const HEADERS: [&str; 7] = [
        "asset_tag",
        "name",
        "category",
        "product",
        "version",
        "criticality",
        "isms_id",
    ];

    fn rejected(r: Result<Inventory, CoreError>) -> Vec<String> {
        match r {
            Err(CoreError::ImportRejected(errors)) => errors,
            other => panic!("應中止：{other:?}"),
        }
    }

    #[test]
    fn legacy_category_and_criticality_give_notices() {
        let inv = parse(&[
            &["FW-01", "fw01", "security", "PAN-OS", "11.1.4", "3", ""],
            &["NAS-01", "nas01", "HW05", "DSM", "7.2", "2", ""],
        ])
        .unwrap();
        let notices: Vec<String> = inv.notices.iter().map(|n| n.to_string()).collect();
        assert_eq!(
            notices,
            vec![
                CRITICALITY_NOTICE.to_string(),
                "第 2 列 FW-01：security 已轉為 HW03 資安設備".to_string(),
            ]
        );
        assert_eq!(inv.assets[0].record.category.as_str(), "HW03");
        assert_eq!(inv.assets[0].record.cia.value(), 4);
    }

    #[test]
    fn missing_product_rejects_but_blank_version_is_pending() {
        let errors = rejected(parse(&[
            &["NAS-03", "nas03", "HW05", "DSM", "", "", ""],
            &["FW-01", "fw01", "HW03", "", "11.1.6", "", ""],
        ]));
        assert_eq!(errors, vec!["第 3 列：缺少 product".to_string()]);

        let inv = parse(&[&["NAS-03", "nas03", "HW05", "DSM", "", "", ""]]).unwrap();
        assert_eq!(inv.assets[0].components[0].version_raw, "");
    }

    #[test]
    fn inventory_without_an_ips_sheet_is_missing() {
        let inv = parse(&[&["NAS-01", "n1", "HW05", "DSM", "7.2", "", ""]]).unwrap();
        assert_eq!(inv.ips, IpsInput::Missing);
        assert_eq!(Inventory::default().ips, IpsInput::Untouched);
    }

    #[test]
    fn duplicate_isms_id_lists_both_rows() {
        let errors = rejected(parse(&[
            &["NAS-01", "n1", "HW05", "DSM", "7.2", "", "SYS-HW-005"],
            &["NAS-02", "n2", "HW05", "DSM", "7.2", "", "sys-hw-005"],
        ]));
        assert_eq!(
            errors,
            vec!["第 3 列：ISMS 編號「sys-hw-005」與第 2 列重複".to_string()]
        );
    }

    #[test]
    fn application_system_message_appears_once_per_row() {
        let errors = rejected(parse(&[&["APP-01", "app", "SW01", "PAN-OS", "1", "", ""]]));
        assert_eq!(
            errors,
            vec!["第 2 列：SW01 應用系統必須填寫機密性、完整性與可用性".to_string()]
        );
    }

    #[test]
    fn invalid_rows_still_report_tag_duplicates() {
        let errors = rejected(parse(&[
            &["FW-01", "fw01", "printer", "PAN-OS", "1", "", ""],
            &["FW-01", "fw01", "HW03", "PAN-OS", "1", "", ""],
        ]));
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].contains("printer"), "{errors:?}");
        assert_eq!(errors[1], "第 3 列：asset_tag「FW-01」與第 2 列重複");
    }
}
