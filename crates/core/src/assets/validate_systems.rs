//! 清冊 `systems` 工作表的驗證（16 spec Inventory systems sheet）：代碼格式、名稱必填、
//! 工作表內代碼與名稱不重複。與資料庫中資產說明的名稱衝突在 `plan_systems.rs` 檢查。

use std::collections::HashMap;

use super::columns::{SheetKind, StandardField};
use super::sheet::Sheet;
use super::systems::{name_key, AssetSystem, SystemCode};
use super::validate::RowRef;

/// `systems` 工作表的一列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingSystem {
    pub row: RowRef,
    pub system: AssetSystem,
}

fn cell<'a>(
    cells: &'a [String],
    columns: &HashMap<StandardField, usize>,
    f: StandardField,
) -> &'a str {
    columns
        .get(&f)
        .and_then(|&i| cells.get(i))
        .map_or("", |s| s.trim())
}

/// 逐列驗證；錯誤加入 `errors`（「systems 工作表第 N 列：…」），只回傳通過的列。
/// 代碼與名稱都空白的列視為空列略過。
pub(super) fn parse_systems(
    sheet: &Sheet,
    columns: &HashMap<StandardField, usize>,
    errors: &mut Vec<String>,
) -> Vec<IncomingSystem> {
    let mut codes: HashMap<SystemCode, usize> = HashMap::new();
    let mut names: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for row in &sheet.rows {
        let at = RowRef {
            sheet: SheetKind::Systems,
            number: row.number,
        };
        let code_raw = cell(&row.cells, columns, StandardField::Code);
        let name = cell(&row.cells, columns, StandardField::Name);
        if code_raw.is_empty() && name.is_empty() {
            continue;
        }
        let before = errors.len();
        let code = SystemCode::parse(code_raw);
        if code.is_none() {
            errors.push(format!(
                "{at}：資產說明代碼「{code_raw}」必須是三碼英數字，例如 SYS"
            ));
        }
        if name.is_empty() {
            errors.push(format!("{at}：缺少 name"));
        }
        if let Some(code) = &code {
            if let Some(first) = codes.insert(code.clone(), row.number) {
                errors.push(format!("{at}：代碼 {code} 與第 {first} 列重複"));
            }
        }
        if !name.is_empty() {
            if let Some(first) = names.insert(name_key(name), row.number) {
                errors.push(format!("{at}：名稱「{name}」與第 {first} 列重複"));
            }
        }
        if let (Some(code), true) = (code, errors.len() == before) {
            out.push(IncomingSystem {
                row: at,
                system: AssetSystem {
                    code,
                    name: name.to_string(),
                },
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::sheet::SheetRow;

    fn sheet(rows: &[[&str; 2]]) -> (Sheet, HashMap<StandardField, usize>) {
        let sheet = Sheet {
            kind: SheetKind::Systems,
            headers: vec!["code".into(), "name".into()],
            rows: rows
                .iter()
                .enumerate()
                .map(|(i, r)| SheetRow {
                    number: i + 2,
                    cells: r.iter().map(|s| s.to_string()).collect(),
                })
                .collect(),
        };
        let cols = HashMap::from([(StandardField::Code, 0), (StandardField::Name, 1)]);
        (sheet, cols)
    }

    #[test]
    fn valid_rows_are_parsed() {
        let (s, c) = sheet(&[["net", "網路服務"], ["", ""], ["SYS", "核心系統"]]);
        let mut errors = Vec::new();
        let out = parse_systems(&s, &c, &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].system.code.as_str(), "NET");
        assert_eq!(out[1].row.number, 4);
    }

    #[test]
    fn duplicate_name_in_the_systems_sheet() {
        let (s, c) = sheet(&[
            ["NET", "網路服務"],
            ["SYS", "核心系統"],
            ["LAB", "網路服務"],
        ]);
        let mut errors = Vec::new();
        let out = parse_systems(&s, &c, &mut errors);
        assert_eq!(
            errors,
            vec!["systems 工作表第 4 列：名稱「網路服務」與第 2 列重複".to_string()]
        );
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn full_width_names_are_duplicates() {
        let (s, c) = sheet(&[["NET", "NET服務"], ["LAB", "ＮＥＴ服務"]]);
        let mut errors = Vec::new();
        parse_systems(&s, &c, &mut errors);
        assert_eq!(errors.len(), 1, "{errors:?}");
    }

    #[test]
    fn bad_code_and_missing_name() {
        let (s, c) = sheet(&[["NET1", ""], ["NET", "a"], ["net", "b"]]);
        let mut errors = Vec::new();
        parse_systems(&s, &c, &mut errors);
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors[0].contains("三碼英數字"));
        assert!(errors[1].contains("缺少 name"));
        assert!(errors[2].contains("代碼 NET 與第 3 列重複"));
    }
}
