//! 清冊的 `eol` 工作表（22 design D7）：逐列讀取硬體（product 空白）與元件的 EoL 覆寫。
//! 產品寫法在規劃階段以目錄解析（[`super::plan_eol`]），此處只檢查格式與同一份清冊內的重複。

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use super::columns::{SheetKind, StandardField};
use super::dates::invalid_date_message;
use super::model::EolOverride;
use super::record_lifecycle::{parse_override, OverrideError, OVERRIDE_SOURCE_REQUIRED};
use super::sheet::Sheet;
use super::validate::{RowReader, RowRef};
use crate::catalog::normalize;

/// 清冊中的 EoL 覆寫。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum EolInput {
    /// 不處理覆寫，也不提醒（ISMS 匯入使用）
    #[default]
    Untouched,
    /// 清冊沒有 `eol` 工作表（CSV 清冊一律如此）：保留既有覆寫，需要時提醒
    Missing,
    /// `eol` 工作表的列（依列序）；清冊中各資產的覆寫以此為準
    Sheet(Vec<IncomingEol>),
}

/// `eol` 工作表的一列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingEol {
    pub row: RowRef,
    pub asset_tag: String,
    /// 產品寫法；None 為硬體覆寫
    pub product_raw: Option<String>,
    pub value: EolOverride,
}

fn override_message(e: &OverrideError) -> String {
    match e {
        OverrideError::InvalidDate(raw) => invalid_date_message("eol", raw),
        OverrideError::MissingDate => "缺少 eol".to_string(),
        OverrideError::MissingSource => OVERRIDE_SOURCE_REQUIRED.to_string(),
    }
}

/// 讀取 `eol` 工作表。`incoming` 為 assets 工作表中驗證通過的資產編號，`known` 為 assets
/// 工作表中出現過的所有編號（該資產列本身有錯時不重複提示）。
pub(super) fn parse_eol(
    sheet: &Sheet,
    columns: &HashMap<StandardField, usize>,
    incoming: &HashSet<String>,
    known: &HashSet<String>,
    errors: &mut Vec<String>,
) -> Vec<IncomingEol> {
    // (資產, 正規化產品寫法；硬體為空字串) → 第一次出現的列號
    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    let mut out = Vec::new();
    for row in &sheet.rows {
        let at = RowRef {
            sheet: SheetKind::Eol,
            number: row.number,
        };
        let reader = RowReader { row, at, columns };
        let tag = reader.get(StandardField::AssetTag);
        let product = reader.get(StandardField::Product);
        let before = errors.len();
        if tag.is_empty() {
            errors.push(format!("{at}：缺少 asset_tag"));
        }
        let value = match parse_override(
            reader.get(StandardField::Eol),
            reader.get(StandardField::Source),
        ) {
            Ok(Some(v)) => Some(v),
            Ok(None) => {
                errors.push(format!("{at}：缺少 eol 與 source"));
                None
            }
            Err(e) => {
                errors.push(format!("{at}：{}", override_message(&e)));
                None
            }
        };
        if errors.len() != before {
            continue;
        }
        if !incoming.contains(tag) {
            if !known.contains(tag) {
                errors.push(format!("{at}：asset_tag「{tag}」不在 assets 工作表中"));
            }
            continue;
        }
        let key = (tag.to_string(), normalize(product));
        let first = match seen.entry(key) {
            Entry::Occupied(e) => Some(*e.get()),
            Entry::Vacant(e) => {
                e.insert(row.number);
                None
            }
        };
        if let Some(first) = first {
            let what = if product.is_empty() {
                "硬體 EoL 覆寫".to_string()
            } else {
                format!("產品「{product}」的 EoL 覆寫")
            };
            errors.push(format!("{at}：{tag} 的{what}與第 {first} 列重複"));
            continue;
        }
        out.push(IncomingEol {
            row: at,
            asset_tag: tag.to_string(),
            product_raw: (!product.is_empty()).then(|| product.to_string()),
            value: value.expect("沒有錯誤時已解析"),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::columns::ColumnMap;
    use crate::assets::sheet::SheetRow;

    fn parse(rows: &[&[&str]]) -> (Vec<IncomingEol>, Vec<String>) {
        let sheet = Sheet {
            kind: SheetKind::Eol,
            headers: ["asset_tag", "product", "eol", "source"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows: rows
                .iter()
                .enumerate()
                .map(|(i, r)| SheetRow {
                    number: i + 2,
                    cells: r.iter().map(|s| s.to_string()).collect(),
                })
                .collect(),
        };
        let cols = ColumnMap::default()
            .locate(&sheet.headers, SheetKind::Eol)
            .unwrap();
        let incoming: HashSet<String> = ["SW-CORE", "SRV-01"].map(String::from).into();
        let known: HashSet<String> = ["SW-CORE", "SRV-01", "BAD-01"].map(String::from).into();
        let mut errors = Vec::new();
        let out = parse_eol(&sheet, &cols, &incoming, &known, &mut errors);
        (out, errors)
    }

    #[test]
    fn hardware_and_component_rows() {
        let (out, errors) = parse(&[
            &["SW-CORE", "", "2027-06-30", "原廠延長支援函"],
            &["SRV-01", "ESXi", "2028/12/31", "延長支援合約"],
        ]);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(out[0].product_raw, None);
        assert_eq!(out[1].product_raw.as_deref(), Some("ESXi"));
        assert_eq!(out[1].value.source, "延長支援合約");
        assert_eq!(out[1].value.date.to_string(), "2028-12-31");
    }

    #[test]
    fn override_without_a_source_and_invalid_dates() {
        let (_, errors) = parse(&[
            &["SW-CORE", "", "2027-06-30", ""],
            &["SRV-01", "ESXi", "2027-02-30", "x"],
        ]);
        assert_eq!(
            errors,
            [
                "eol 工作表第 2 列：EoL 覆寫必須填寫來源",
                "eol 工作表第 3 列：eol 的「2027-02-30」不是有效日期（請用 YYYY-MM-DD）",
            ]
        );
    }

    #[test]
    fn duplicate_rows_for_the_same_target() {
        let (_, errors) = parse(&[
            &["SRV-01", "ESXi", "2028-12-31", "a"],
            &["SRV-01", "esxi", "2029-12-31", "b"],
            &["SRV-01", "ESXI", "2030-12-31", "b"],
            &["SW-CORE", "", "2027-06-30", "c"],
            &["SW-CORE", " ", "2027-07-30", "d"],
        ]);
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors[0].contains("第 3 列") && errors[0].contains("與第 2 列重複"));
        assert!(
            errors[1].contains("第 4 列") && errors[1].contains("與第 2 列重複"),
            "指向第一次出現的列：{errors:?}"
        );
        assert!(errors[2].contains("硬體 EoL 覆寫與第 5 列重複"));
    }

    #[test]
    fn unknown_assets_are_reported_once() {
        let (out, errors) = parse(&[
            &["XX-99", "", "2027-06-30", "a"],
            &["BAD-01", "", "2027-06-30", "a"],
        ]);
        assert!(out.is_empty());
        assert_eq!(
            errors,
            ["eol 工作表第 2 列：asset_tag「XX-99」不在 assets 工作表中"]
        );
    }
}
