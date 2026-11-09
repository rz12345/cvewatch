//! 讀取 ISMS 清冊檔（17 design D2；code review）：依 [`candidate_order`] 逐一載入工作表，找到
//! 表頭就停止（用不到的工作表讀取失敗時略過）；只有一欄寬的垂直合併儲存格，把左上角的值
//! 填入整個範圍（手工維護的清冊常把同一系統代碼、資產說明合併成一格）。

use std::path::Path;

use calamine::{open_workbook_auto, Data, Dimensions, Range, Reader, Sheets};

use super::workbook::{candidate_order, find_header, not_isms, parse_sheet, Grid, IsmsBook};
use crate::assets::sheet::cell_text;
use crate::error::CoreError;

/// Excel 錯誤值的文字寫法（v2.1 未填列的公式結果）。
const ERROR_LITERALS: [&str; 8] = [
    "#N/A",
    "#REF!",
    "#VALUE!",
    "#DIV/0!",
    "#NAME?",
    "#NULL!",
    "#NUM!",
    "#GETTING_DATA",
];

/// 儲存格文字；錯誤值（含文字形式的 `#N/A`）為空白。
fn isms_cell_text(cell: &Data) -> String {
    match cell {
        Data::Error(_) => String::new(),
        other => {
            let text = cell_text(other);
            if ERROR_LITERALS.iter().any(|e| text.eq_ignore_ascii_case(e)) {
                String::new()
            } else {
                text
            }
        }
    }
}

/// 使用範圍不一定從 A1 開始：列號與欄位置換算成試算表的絕對位置。
fn to_grid(range: &Range<Data>) -> Grid {
    let (row0, col0) = range.start().unwrap_or((0, 0));
    range
        .rows()
        .enumerate()
        .map(|(i, r)| {
            let mut cells = vec![String::new(); col0 as usize];
            cells.extend(r.iter().map(isms_cell_text));
            (row0 as usize + i + 1, cells)
        })
        .collect()
}

/// 只有一欄寬、跨兩列以上的合併儲存格：空白的格子填入左上角的值。橫跨多欄的合併
/// （文件資訊列、編碼說明列）不處理，避免說明文字被填進資產編號欄。
pub(super) fn fill_vertical_merges(grid: Grid, merges: &[Dimensions]) -> Grid {
    let Some(first) = grid.first().map(|(n, _)| *n) else {
        return grid;
    };
    let mut out = grid;
    for m in merges
        .iter()
        .filter(|m| m.start.1 == m.end.1 && m.end.0 > m.start.0)
    {
        let col = m.start.1 as usize;
        let index = |row: u32| (row as usize + 1).checked_sub(first);
        let Some(value) = index(m.start.0)
            .and_then(|i| out.get(i))
            .and_then(|(_, cells)| cells.get(col))
            .filter(|v| !v.is_empty())
            .cloned()
        else {
            continue;
        };
        for row in m.start.0 + 1..=m.end.0 {
            let Some((_, cells)) = index(row).and_then(|i| out.get_mut(i)) else {
                continue;
            };
            if cells.len() <= col {
                cells.resize(col + 1, String::new());
            }
            if cells[col].is_empty() {
                cells[col] = value.clone();
            }
        }
    }
    out
}

/// 合併儲存格（只支援 .xlsx；其他格式或讀取失敗時視為沒有合併）。
fn merges<RS: std::io::Read + std::io::Seek>(book: &mut Sheets<RS>, name: &str) -> Vec<Dimensions> {
    match book {
        Sheets::Xlsx(x) => x.merge_cells_by_sheet_name(name).unwrap_or_else(|e| {
            tracing::warn!(sheet = name, error = %e, "無法讀取合併儲存格");
            Vec::new()
        }),
        _ => Vec::new(),
    }
}

/// 讀取 ISMS 資訊資產清冊（.xlsx）。
pub fn read_isms_workbook(path: &Path) -> Result<IsmsBook, CoreError> {
    let mut book = open_workbook_auto(path)
        .map_err(|e| CoreError::Invalid(format!("無法開啟 {}：{e}", path.display())))?;
    let names = book.sheet_names();
    let order = candidate_order(&names);
    for (rank, i) in order.into_iter().enumerate() {
        let name = &names[i];
        let range = match book.worksheet_range(name) {
            Ok(r) => r,
            // 第一個檢查的工作表（「資訊資產清冊」或唯一的工作表）讀不到就是錯誤
            Err(e) if rank == 0 => {
                return Err(CoreError::Invalid(format!("無法讀取工作表 {name}：{e}")))
            }
            Err(e) => {
                tracing::warn!(sheet = %name, error = %e, "略過無法讀取的工作表");
                continue;
            }
        };
        let grid = to_grid(&range);
        if let Some(header_at) = find_header(&grid) {
            let grid = fill_vertical_merges(grid, &merges(&mut book, name));
            return parse_sheet(name, &grid, header_at);
        }
    }
    Err(not_isms(&path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(n: usize, cells: &[&str]) -> (usize, Vec<String>) {
        (n, cells.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn error_values_are_blank() {
        assert_eq!(isms_cell_text(&Data::String("#N/A".into())), "");
        assert_eq!(
            isms_cell_text(&Data::Error(calamine::CellErrorType::NA)),
            ""
        );
        assert_eq!(isms_cell_text(&Data::Float(3.0)), "3");
    }

    #[test]
    fn vertical_merges_are_filled() {
        let grid = vec![
            row(4, &["資產編號", "資產編號"]),
            row(5, &["NET-", "HW-001"]),
            row(6, &["", "HW-002"]),
            row(7, &["", "HW-003"]),
            row(8, &["註：說明"]),
            row(9, &[""]),
        ];
        let merges = [
            // A5:A7（0 起算的列 4–6）
            Dimensions {
                start: (4, 0),
                end: (6, 0),
            },
            // A8:P9：橫跨多欄的說明列不處理
            Dimensions {
                start: (7, 0),
                end: (8, 15),
            },
        ];
        let out = fill_vertical_merges(grid, &merges);
        let col_a: Vec<&str> = out.iter().map(|(_, c)| c[0].as_str()).collect();
        assert_eq!(col_a, ["資產編號", "NET-", "NET-", "NET-", "註：說明", ""]);
        assert_eq!(out[5].1.len(), 1, "第 9 列的 B 欄仍是空白");
    }
}
