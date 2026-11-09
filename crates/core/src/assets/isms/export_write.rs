//! ISMS 匯出的 xlsx 版面（23 design D3）：單一工作表「資訊資產清冊」，第 1–3 列文件資訊、
//! 第 4 列 v2.1 的 16 欄表頭、第 5 列起一列一個資產；不合併儲存格、不寫公式。
//!
//! 文件資訊列不含「資產編號」「資產類別」「資產描述」，讀取端不會把它們誤認為表頭；
//! 不寫文件編號。

use chrono::NaiveDate;
use rust_xlsxwriter::{Format, FormatAlign, Workbook, Worksheet, XlsxError};

use super::export::IsmsExportRow;
use super::workbook::{ISMS_SHEET_NAME, ISMS_V21_HEADERS};

/// 文件資訊第 1 列的標題。
pub const EXPORT_TITLE: &str = "資訊資產清冊（cvewatch 匯出）";
/// 文件資訊第 3 列的說明。
pub const EXPORT_NOTE: &str =
    "僅含有 ISMS 編號的使用中硬體與軟體資產；描述欄中括號內為版本或廠牌型號";
/// 表頭所在的列（0 起算；試算表第 4 列）。
pub const HEADER_ROW: u32 = 3;
/// 未評定的寫法（讀取端視為未評定）。
pub const NOT_RATED: &str = "-";

/// 資產描述欄（0 起算）。
const DESCRIPTION_COLUMN: u16 = 5;
const COLUMN_WIDTH: f64 = 12.0;
const DESCRIPTION_WIDTH: f64 = 40.0;

/// 一列的儲存格值。
enum Cell<'a> {
    Text(&'a str),
    Number(u8),
}

fn rated(v: Option<u8>) -> Cell<'static> {
    v.map_or(Cell::Text(NOT_RATED), Cell::Number)
}

fn cells(r: &IsmsExportRow) -> [Cell<'_>; 16] {
    [
        Cell::Text(&r.system_id),
        Cell::Text(&r.serial),
        Cell::Text(&r.category),
        Cell::Text(&r.system_name),
        Cell::Text(&r.class_name),
        Cell::Text(&r.description),
        Cell::Text(&r.responsible_unit),
        Cell::Text(&r.custodian),
        Cell::Text(&r.user_unit),
        Cell::Number(r.confidentiality),
        Cell::Number(r.integrity),
        Cell::Number(r.availability),
        Cell::Number(r.value),
        rated(r.compliance),
        rated(r.protection_level),
        Cell::Text(&r.personal_data),
    ]
}

fn document_rows(ws: &mut Worksheet, exported_on: NaiveDate) -> Result<(), XlsxError> {
    ws.write_string_with_format(0, 0, EXPORT_TITLE, &Format::new().set_bold())?;
    ws.write_string(1, 0, "匯出日期")?;
    ws.write_string(1, 1, exported_on.format("%Y-%m-%d").to_string())?;
    ws.write_string(2, 0, "說明")?;
    ws.write_string(2, 1, EXPORT_NOTE)?;
    Ok(())
}

fn header_row(ws: &mut Worksheet) -> Result<(), XlsxError> {
    let header = Format::new()
        .set_bold()
        .set_text_wrap()
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);
    for (col, name) in (0u16..).zip(ISMS_V21_HEADERS) {
        let width = if col == DESCRIPTION_COLUMN {
            DESCRIPTION_WIDTH
        } else {
            COLUMN_WIDTH
        };
        ws.set_column_width(col, width)?;
        ws.write_string_with_format(HEADER_ROW, col, name, &header)?;
    }
    Ok(())
}

/// 一個資料列；`wrap` 為資產描述欄的自動換行格式（所有列共用）。
fn data_row(
    ws: &mut Worksheet,
    row: u32,
    r: &IsmsExportRow,
    wrap: &Format,
) -> Result<(), XlsxError> {
    for (col, cell) in (0u16..).zip(cells(r)) {
        match cell {
            Cell::Text("") => {}
            Cell::Text(v) if col == DESCRIPTION_COLUMN => {
                ws.write_string_with_format(row, col, v, wrap)?;
            }
            Cell::Text(v) => {
                ws.write_string(row, col, v)?;
            }
            Cell::Number(n) => {
                ws.write_number(row, col, f64::from(n))?;
            }
        }
    }
    Ok(())
}

/// 產生匯出檔的位元組（不碰檔案）。
pub fn write_workbook(
    rows: &[IsmsExportRow],
    exported_on: NaiveDate,
) -> Result<Vec<u8>, XlsxError> {
    let mut book = Workbook::new();
    let ws = book.add_worksheet();
    ws.set_name(ISMS_SHEET_NAME)?;
    document_rows(ws, exported_on)?;
    header_row(ws)?;
    let wrap = Format::new().set_text_wrap().set_align(FormatAlign::Top);
    for (row, r) in (HEADER_ROW + 1..).zip(rows) {
        data_row(ws, row, r, &wrap)?;
    }
    ws.set_freeze_panes(HEADER_ROW + 1, 0)?;
    book.save_to_buffer()
}

#[cfg(test)]
mod tests {
    use calamine::{open_workbook, Data, Reader, Xlsx};

    use super::*;
    use crate::assets::isms::workbook::{normalize_header, HEADER_MARKERS};

    fn row(serial: &str) -> IsmsExportRow {
        IsmsExportRow {
            system_id: "NET-".into(),
            serial: serial.into(),
            category: "HW02".into(),
            system_name: "網路服務".into(),
            class_name: "通訊設備".into(),
            description: "廠牌型號：【Aruba XYZ-100】\nAOS-CX（1.2.3）".into(),
            responsible_unit: String::new(),
            custodian: "王小明".into(),
            user_unit: String::new(),
            confidentiality: 2,
            integrity: 3,
            availability: 4,
            value: 4,
            compliance: None,
            protection_level: None,
            personal_data: "N".into(),
        }
    }

    fn text(cell: &Data) -> String {
        crate::assets::sheet::cell_text(cell)
    }

    #[test]
    fn header_row_of_the_exported_file() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        let bytes = write_workbook(&[row("HW-001"), row("HW-002")], date).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("isms.xlsx");
        std::fs::write(&path, bytes).unwrap();
        let mut book: Xlsx<_> = open_workbook(&path).unwrap();
        assert_eq!(book.sheet_names(), vec![ISMS_SHEET_NAME.to_string()]);
        let range = book.worksheet_range(ISMS_SHEET_NAME).unwrap();
        assert_eq!(range.start(), Some((0, 0)));
        let rows: Vec<Vec<String>> = range.rows().map(|r| r.iter().map(text).collect()).collect();
        assert_eq!(rows[3].len(), 16);
        assert_eq!(rows[3], ISMS_V21_HEADERS.map(String::from));
        assert_eq!(&rows[3][..2], ["資產編號", "資產編號"]);
        for info in &rows[..3] {
            for cell in info {
                for marker in HEADER_MARKERS {
                    assert_ne!(normalize_header(cell), marker, "文件資訊列：{info:?}");
                    assert!(!cell.contains(marker), "文件資訊列：{info:?}");
                }
            }
        }
        assert_eq!(rows[1][1], "2026-11-09");
        assert_eq!(rows[4][..3], ["NET-", "HW-001", "HW02"]);
        assert_eq!(rows[4][9..], ["2", "3", "4", "4", "-", "-", "N"]);
        assert_eq!(rows[5][1], "HW-002");
        assert!(
            matches!(range.get((4, 9)), Some(Data::Float(f)) if *f == 2.0),
            "C/I/A 為數字"
        );
        assert!(
            book.merge_cells_by_sheet_name(ISMS_SHEET_NAME)
                .unwrap()
                .is_empty(),
            "不使用合併儲存格"
        );
    }
}
