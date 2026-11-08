//! 列驗證：必填欄位、category / exposure / criticality 範圍、重複 asset_tag。
//! 所有錯誤彙整後一次回報，匯入整批中止。

use std::collections::{HashMap, HashSet};
use std::fmt;

use super::columns::{ColumnMap, SheetKind, StandardField};
use super::model::{
    AssetRecord, Category, Exposure, DEFAULT_CRITICALITY, MAX_CRITICALITY, MIN_CRITICALITY,
};
use super::sheet::{Sheet, SheetRow, Workbook};
use crate::error::CoreError;

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
    pub record: AssetRecord,
    /// 第一個為 assets 工作表的主要元件，其後為 components 工作表
    pub components: Vec<IncomingComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Inventory {
    pub assets: Vec<IncomingAsset>,
}

struct RowReader<'a> {
    row: &'a SheetRow,
    at: RowRef,
    columns: &'a HashMap<StandardField, usize>,
}

impl RowReader<'_> {
    fn get(&self, field: StandardField) -> &str {
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
pub fn parse_inventory(book: &Workbook, columns: &ColumnMap) -> Result<Inventory, CoreError> {
    let mut errors = Vec::new();
    let asset_cols = columns.locate(&book.assets.headers, SheetKind::Assets)?;
    let mut assets = parse_assets(&book.assets, &asset_cols, &mut errors);
    if let Some(sheet) = &book.components {
        let comp_cols = columns.locate(&sheet.headers, SheetKind::Components)?;
        let known = column_values(&book.assets, &asset_cols, StandardField::AssetTag);
        attach_components(sheet, &comp_cols, &known, &mut assets, &mut errors);
    }
    if !errors.is_empty() {
        return Err(CoreError::ImportRejected(errors));
    }
    Ok(Inventory { assets })
}

fn parse_assets(
    sheet: &Sheet,
    columns: &HashMap<StandardField, usize>,
    errors: &mut Vec<String>,
) -> Vec<IncomingAsset> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for row in &sheet.rows {
        let at = RowRef {
            sheet: SheetKind::Assets,
            number: row.number,
        };
        let reader = RowReader { row, at, columns };
        let before = errors.len();
        let parsed = parse_asset_row(&reader, errors);
        let tag = parsed.record.asset_tag.clone();
        if !tag.is_empty() {
            if let Some(first) = seen.insert(tag.clone(), row.number) {
                errors.push(format!("{at}：asset_tag「{tag}」與第 {first} 列重複"));
            }
        }
        if errors.len() == before {
            out.push(parsed);
        }
    }
    out
}

fn parse_asset_row(r: &RowReader<'_>, errors: &mut Vec<String>) -> IncomingAsset {
    let input = RecordInput {
        asset_tag: r.get(StandardField::AssetTag),
        name: r.get(StandardField::Name),
        category: r.get(StandardField::Category),
        exposure: r.get(StandardField::Exposure),
        criticality: r.get(StandardField::Criticality),
        location: r.get(StandardField::Location),
        owner: r.get(StandardField::Owner),
        notes: r.get(StandardField::Notes),
    };
    let record = validate_record(&input).unwrap_or_else(|field_errors| {
        errors.extend(
            field_errors
                .iter()
                .map(|e| format!("{}：{}", r.at, e.message)),
        );
        fallback_record(&input)
    });
    let product_raw = r.required(StandardField::Product, errors);
    let version_raw = r.required(StandardField::Version, errors);
    IncomingAsset {
        row: r.at,
        record,
        components: vec![IncomingComponent {
            row: r.at,
            product_raw,
            version_raw,
        }],
    }
}

/// 單筆資產欄位的原始輸入（清冊儲存格或畫面輸入框）；前後空白在驗證時去除。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordInput<'a> {
    pub asset_tag: &'a str,
    pub name: &'a str,
    pub category: &'a str,
    /// 空白時為預設 internal
    pub exposure: &'a str,
    /// 空白時為預設 2
    pub criticality: &'a str,
    pub location: &'a str,
    pub owner: &'a str,
    pub notes: &'a str,
}

impl RecordInput<'_> {
    /// 欄位的原始輸入；product、version 不屬於資產欄位，回傳空字串。
    pub fn value(&self, field: StandardField) -> &str {
        match field {
            StandardField::AssetTag => self.asset_tag,
            StandardField::Name => self.name,
            StandardField::Category => self.category,
            StandardField::Exposure => self.exposure,
            StandardField::Criticality => self.criticality,
            StandardField::Location => self.location,
            StandardField::Owner => self.owner,
            StandardField::Notes => self.notes,
            StandardField::Product | StandardField::Version => "",
        }
    }
}

/// 資產欄位中的必填欄位（依驗證與錯誤回報的順序）。
pub const REQUIRED_FIELDS: [StandardField; 3] = [
    StandardField::AssetTag,
    StandardField::Name,
    StandardField::Category,
];

/// 欄位錯誤的種類（讓畫面以自己的用語呈現，不必解析 `message`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldErrorKind {
    /// 必填欄位空白
    Missing,
    /// 不是允許的值（category、exposure）
    NotAllowed,
    /// 超出範圍或不是數字（criticality）
    OutOfRange,
}

/// 單一欄位的驗證錯誤；`message` 不含列位置（匯入時加上「第 N 列：」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: StandardField,
    pub kind: FieldErrorKind,
    pub message: String,
}

impl FieldError {
    fn new(field: StandardField, kind: FieldErrorKind, message: String) -> FieldError {
        FieldError {
            field,
            kind,
            message,
        }
    }
}

fn optional_text(v: &str) -> Option<String> {
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// 驗證資產欄位（必填、category / exposure / criticality 允許值與預設值）；
/// 清冊匯入與畫面編修共用，錯誤依欄位順序回報。
pub fn validate_record(input: &RecordInput<'_>) -> Result<AssetRecord, Vec<FieldError>> {
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
    let category = parse_enum(
        input.category.trim(),
        StandardField::Category,
        Category::parse,
        Category::allowed,
        &mut errors,
    );
    let exposure = parse_enum(
        input.exposure.trim(),
        StandardField::Exposure,
        Exposure::parse,
        Exposure::allowed,
        &mut errors,
    );
    let criticality = parse_criticality(input.criticality.trim(), &mut errors);
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(AssetRecord {
        asset_tag: input.asset_tag.trim().to_string(),
        name: input.name.trim().to_string(),
        category: category.unwrap_or(Category::Host),
        location: optional_text(input.location),
        owner: optional_text(input.owner),
        exposure: exposure.unwrap_or_default(),
        criticality: criticality.unwrap_or(DEFAULT_CRITICALITY),
        notes: optional_text(input.notes),
    })
}

/// 驗證失敗的列仍需要 asset_tag 以偵測重複；其餘欄位不會被使用。
fn fallback_record(input: &RecordInput<'_>) -> AssetRecord {
    AssetRecord {
        asset_tag: input.asset_tag.trim().to_string(),
        name: input.name.trim().to_string(),
        category: Category::Host,
        location: None,
        owner: None,
        exposure: Exposure::default(),
        criticality: DEFAULT_CRITICALITY,
        notes: None,
    }
}

/// 空白回傳 None（必填與否由呼叫端判斷）；不是允許值時記錄錯誤。
fn parse_enum<T>(
    value: &str,
    field: StandardField,
    parse: fn(&str) -> Option<T>,
    allowed: fn() -> String,
    errors: &mut Vec<FieldError>,
) -> Option<T> {
    if value.is_empty() {
        return None;
    }
    let parsed = parse(value);
    if parsed.is_none() {
        errors.push(FieldError::new(
            field,
            FieldErrorKind::NotAllowed,
            format!("{}「{value}」不是允許的值（{}）", field.as_str(), allowed()),
        ));
    }
    parsed
}

fn parse_criticality(raw: &str, errors: &mut Vec<FieldError>) -> Option<u8> {
    if raw.is_empty() {
        return None;
    }
    match raw.parse::<u8>() {
        Ok(n) if (MIN_CRITICALITY..=MAX_CRITICALITY).contains(&n) => Some(n),
        _ => {
            errors.push(FieldError::new(
                StandardField::Criticality,
                FieldErrorKind::OutOfRange,
                format!("criticality「{raw}」必須是 {MIN_CRITICALITY}–{MAX_CRITICALITY}"),
            ));
            None
        }
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
        let version_raw = reader.required(StandardField::Version, errors);
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

    fn input<'a>() -> RecordInput<'a> {
        RecordInput {
            asset_tag: "FW-02",
            name: "fw02",
            category: "security",
            ..RecordInput::default()
        }
    }

    fn fields(r: Result<AssetRecord, Vec<FieldError>>) -> Vec<(StandardField, String)> {
        r.expect_err("應驗證失敗")
            .into_iter()
            .map(|e| (e.field, e.message))
            .collect()
    }

    #[test]
    fn empty_asset_tag_is_reported_on_its_field() {
        let errors = fields(validate_record(&RecordInput {
            asset_tag: "  ",
            ..input()
        }));
        assert_eq!(
            errors,
            vec![(StandardField::AssetTag, "缺少 asset_tag".to_string())]
        );
    }

    #[test]
    fn errors_carry_their_kind() {
        let errors = validate_record(&RecordInput {
            name: "",
            category: "router",
            criticality: "9",
            ..input()
        })
        .expect_err("應驗證失敗");
        let kinds: Vec<_> = errors.iter().map(|e| (e.field, e.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                (StandardField::Name, FieldErrorKind::Missing),
                (StandardField::Category, FieldErrorKind::NotAllowed),
                (StandardField::Criticality, FieldErrorKind::OutOfRange),
            ]
        );
        let empty = RecordInput::default();
        assert!(REQUIRED_FIELDS.iter().all(|f| empty.value(*f).is_empty()));
    }

    #[test]
    fn invalid_exposure_lists_allowed_values() {
        let errors = fields(validate_record(&RecordInput {
            exposure: "public",
            ..input()
        }));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, StandardField::Exposure);
        assert_eq!(
            errors[0].1,
            "exposure「public」不是允許的值（internet、dmz、internal、mgmt）"
        );
    }

    #[test]
    fn criticality_out_of_range_is_rejected() {
        let errors = fields(validate_record(&RecordInput {
            criticality: "5",
            ..input()
        }));
        assert_eq!(
            errors,
            vec![(
                StandardField::Criticality,
                "criticality「5」必須是 1–3".to_string()
            )]
        );
    }

    #[test]
    fn defaults_and_trimming() {
        let record = validate_record(&RecordInput {
            asset_tag: " FW-02 ",
            location: "  ",
            owner: " 網管組 ",
            ..input()
        })
        .expect("valid");
        assert_eq!(record.asset_tag, "FW-02");
        assert_eq!(record.exposure, Exposure::Internal);
        assert_eq!(record.criticality, DEFAULT_CRITICALITY);
        assert_eq!(record.category, Category::Security);
        assert_eq!(record.location, None);
        assert_eq!(record.owner.as_deref(), Some("網管組"));
    }

    #[test]
    fn several_errors_are_reported_in_field_order() {
        let errors = fields(validate_record(&RecordInput {
            name: "",
            category: "router",
            criticality: "x",
            ..input()
        }));
        let order: Vec<StandardField> = errors.iter().map(|e| e.0).collect();
        assert_eq!(
            order,
            vec![
                StandardField::Name,
                StandardField::Category,
                StandardField::Criticality
            ]
        );
    }
}
