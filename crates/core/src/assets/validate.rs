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

    fn optional(&self, field: StandardField) -> Option<String> {
        let v = self.get(field);
        (!v.is_empty()).then(|| v.to_string())
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
    let asset_tag = r.required(StandardField::AssetTag, errors);
    let name = r.required(StandardField::Name, errors);
    let category_raw = r.required(StandardField::Category, errors);
    let product_raw = r.required(StandardField::Product, errors);
    let version_raw = r.required(StandardField::Version, errors);
    let category = parse_enum(
        &category_raw,
        "category",
        r.at,
        Category::parse,
        Category::allowed,
        errors,
    )
    .unwrap_or(Category::Host);
    let exposure = match r.optional(StandardField::Exposure) {
        None => Exposure::default(),
        Some(v) => parse_enum(
            &v,
            "exposure",
            r.at,
            Exposure::parse,
            Exposure::allowed,
            errors,
        )
        .unwrap_or_default(),
    };
    let criticality = parse_criticality(r, errors);
    IncomingAsset {
        row: r.at,
        record: AssetRecord {
            asset_tag,
            name,
            category,
            location: r.optional(StandardField::Location),
            owner: r.optional(StandardField::Owner),
            exposure,
            criticality,
            notes: r.optional(StandardField::Notes),
        },
        components: vec![IncomingComponent {
            row: r.at,
            product_raw,
            version_raw,
        }],
    }
}

fn parse_enum<T>(
    value: &str,
    field: &str,
    at: RowRef,
    parse: fn(&str) -> Option<T>,
    allowed: fn() -> String,
    errors: &mut Vec<String>,
) -> Option<T> {
    if value.is_empty() {
        return None;
    }
    let parsed = parse(value);
    if parsed.is_none() {
        errors.push(format!(
            "{at}：{field}「{value}」不是允許的值（{}）",
            allowed()
        ));
    }
    parsed
}

fn parse_criticality(r: &RowReader<'_>, errors: &mut Vec<String>) -> u8 {
    let Some(raw) = r.optional(StandardField::Criticality) else {
        return DEFAULT_CRITICALITY;
    };
    match raw.parse::<u8>() {
        Ok(n) if (MIN_CRITICALITY..=MAX_CRITICALITY).contains(&n) => n,
        _ => {
            errors.push(format!(
                "{}：criticality「{raw}」必須是 {MIN_CRITICALITY}–{MAX_CRITICALITY}",
                r.at
            ));
            DEFAULT_CRITICALITY
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
