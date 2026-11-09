//! 讀取清冊：.xlsx（`assets` / `components` / `systems` / `ips` 工作表）或 .csv（assets 檔，
//! 另可指定元件檔與 IP 檔）。
//!
//! 只轉成字串表格；欄位意義與驗證在 [`super::validate`]。

use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};

use super::columns::SheetKind;
use crate::error::CoreError;

/// 一列資料；`number` 為試算表中的列號（表頭為第 1 列）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetRow {
    pub number: usize,
    pub cells: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    pub kind: SheetKind,
    pub headers: Vec<String>,
    pub rows: Vec<SheetRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workbook {
    pub assets: Sheet,
    pub components: Option<Sheet>,
    /// 資產說明（16 design D3；選填，CSV 清冊沒有）
    pub systems: Option<Sheet>,
    /// 資產的 IP（20 design D5；選填，CSV 以 `--ips` 指定）
    pub ips: Option<Sheet>,
}

const UTF8_BOM: &str = "\u{feff}";

/// 依副檔名讀取清冊；CSV 的元件另以 `components_csv`、IP 以 `ips_csv` 指定。
pub fn read_inventory(
    path: &Path,
    components_csv: Option<&Path>,
    ips_csv: Option<&Path>,
) -> Result<Workbook, CoreError> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "xlsx" | "xlsm" | "xls" | "ods" => {
            if components_csv.is_some() {
                return Err(CoreError::Invalid(
                    "--components 只用於 CSV；Excel 請使用 components 工作表".to_string(),
                ));
            }
            if ips_csv.is_some() {
                return Err(CoreError::Invalid(
                    "--ips 只用於 CSV；Excel 請使用 ips 工作表".to_string(),
                ));
            }
            read_workbook(path)
        }
        "csv" => Ok(Workbook {
            assets: read_csv(path, SheetKind::Assets)?,
            components: components_csv
                .map(|p| read_csv(p, SheetKind::Components))
                .transpose()?,
            systems: None,
            ips: ips_csv.map(|p| read_csv(p, SheetKind::Ips)).transpose()?,
        }),
        _ => Err(CoreError::Invalid(format!(
            "不支援的清冊格式：{}（支援 .xlsx、.csv）",
            path.display()
        ))),
    }
}

fn read_workbook(path: &Path) -> Result<Workbook, CoreError> {
    let mut book = open_workbook_auto(path)
        .map_err(|e| CoreError::Invalid(format!("無法開啟 {}：{e}", path.display())))?;
    let names = book.sheet_names();
    let find = |kind: SheetKind| {
        names
            .iter()
            .find(|n| n.trim().eq_ignore_ascii_case(kind.sheet_name()))
            .cloned()
    };
    let assets_name = find(SheetKind::Assets).ok_or_else(|| {
        CoreError::Invalid(format!(
            "找不到 assets 工作表（現有：{}）",
            names.join("、")
        ))
    })?;
    let mut load = |name: &str, kind: SheetKind| -> Result<Sheet, CoreError> {
        let range = book
            .worksheet_range(name)
            .map_err(|e| CoreError::Invalid(format!("無法讀取工作表 {name}：{e}")))?;
        let rows: Vec<Vec<String>> = range
            .rows()
            .map(|r| r.iter().map(cell_text).collect())
            .collect();
        Ok(to_sheet(kind, rows))
    };
    let assets = load(&assets_name, SheetKind::Assets)?;
    let components = match find(SheetKind::Components) {
        Some(name) => Some(load(&name, SheetKind::Components)?),
        None => None,
    };
    let systems = match find(SheetKind::Systems) {
        Some(name) => Some(load(&name, SheetKind::Systems)?),
        None => None,
    };
    let ips = match find(SheetKind::Ips) {
        Some(name) => Some(load(&name, SheetKind::Ips)?),
        None => None,
    };
    Ok(Workbook {
        assets,
        components,
        systems,
        ips,
    })
}

/// 儲存格轉字串。整數值的浮點數（例如 criticality `2`）不帶小數點。
pub fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        Data::Float(f) => f.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#{e:?}"),
    }
}

fn read_csv(path: &Path, kind: SheetKind) -> Result<Sheet, CoreError> {
    let bytes = std::fs::read(path).map_err(|e| CoreError::io(path, e))?;
    let text = String::from_utf8(bytes).map_err(|_| {
        CoreError::Invalid(format!(
            "{} 不是 UTF-8 編碼；請在 Excel 另存為「CSV UTF-8（逗號分隔）」",
            path.display()
        ))
    })?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.trim_start_matches(UTF8_BOM).as_bytes());
    let rows = reader
        .records()
        .map(|r| {
            r.map(|rec| rec.iter().map(|c| c.trim().to_string()).collect())
                .map_err(|e| CoreError::Invalid(format!("{} 格式錯誤：{e}", path.display())))
        })
        .collect::<Result<Vec<Vec<String>>, _>>()?;
    Ok(to_sheet(kind, rows))
}

/// 第一列為表頭；完全空白的列略過，但列號保持與試算表一致。
fn to_sheet(kind: SheetKind, mut rows: Vec<Vec<String>>) -> Sheet {
    let headers = if rows.is_empty() {
        Vec::new()
    } else {
        rows.remove(0)
    };
    let rows = rows
        .into_iter()
        .enumerate()
        .filter(|(_, cells)| cells.iter().any(|c| !c.is_empty()))
        .map(|(i, cells)| SheetRow {
            number: i + 2,
            cells,
        })
        .collect();
    Sheet {
        kind,
        headers,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_text_formats_numbers() {
        assert_eq!(cell_text(&Data::Float(2.0)), "2");
        assert_eq!(cell_text(&Data::Float(2.16)), "2.16");
        assert_eq!(cell_text(&Data::Int(3)), "3");
        assert_eq!(cell_text(&Data::String(" a ".into())), "a");
        assert_eq!(cell_text(&Data::Empty), "");
    }

    #[test]
    fn to_sheet_keeps_spreadsheet_row_numbers() {
        let rows = vec![
            vec!["h".to_string()],
            vec!["a".to_string()],
            vec!["".to_string()],
            vec!["b".to_string()],
        ];
        let sheet = to_sheet(SheetKind::Assets, rows);
        let numbers: Vec<usize> = sheet.rows.iter().map(|r| r.number).collect();
        assert_eq!(numbers, vec![2, 4]);
    }
}
