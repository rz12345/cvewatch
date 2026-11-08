//! 空白範本（`cvewatch asset template`）與共用的 xlsx 寫出（清冊匯出也使用）。

use std::path::Path;

use rust_xlsxwriter::{Format, Workbook};

use super::columns::{SheetKind, StandardField};
use crate::error::CoreError;

const HEADER_COLUMN_WIDTH: f64 = 18.0;

/// 一個要寫出的工作表。
pub struct SheetData<'a> {
    pub name: &'a str,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// 範本範例列。
fn example_assets() -> Vec<Vec<String>> {
    let rows: [[&str; 10]; 3] = [
        [
            "FW-01",
            "fw01",
            "security",
            "PAN-OS",
            "11.1.4-h7",
            "internet",
            "3",
            "機房 A",
            "網管組",
            "",
        ],
        [
            "R6515-01",
            "esx01",
            "virtualization",
            "ESXi",
            "8.0 U3c",
            "mgmt",
            "3",
            "機房 A",
            "系統組",
            "",
        ],
        [
            "NAS-01",
            "nas01",
            "storage",
            "Synology DSM",
            "7.2.2-72806 Update 3",
            "internal",
            "2",
            "機房 B",
            "系統組",
            "",
        ],
    ];
    rows.iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect()
}

fn example_components() -> Vec<Vec<String>> {
    vec![vec![
        "R6515-01".into(),
        "iDRAC9".into(),
        "7.10.30.00".into(),
    ]]
}

/// 標準欄名表頭。
pub(crate) fn headers(kind: SheetKind) -> Vec<String> {
    kind.columns()
        .iter()
        .map(|f| f.as_str().to_string())
        .collect()
}

/// 寫出含 `assets` 與 `components` 工作表、標準欄名與範例列的範本；目的檔已存在時拒絕。
pub fn write_template(path: &Path) -> Result<(), CoreError> {
    write_sheets(
        path,
        &[
            SheetData {
                name: SheetKind::Assets.sheet_name(),
                headers: headers(SheetKind::Assets),
                rows: example_assets(),
            },
            SheetData {
                name: SheetKind::Components.sheet_name(),
                headers: headers(SheetKind::Components),
                rows: example_components(),
            },
        ],
    )
}

/// 目的檔已存在（範本與清冊匯出共用的訊息）。
fn already_exists(path: &Path) -> CoreError {
    CoreError::Invalid(format!("目的檔已存在，不覆蓋：{}", path.display()))
}

/// 目的檔已存在時回報錯誤（例如開啟資料庫之前先檢查）；實際寫入仍以 `create_new` 保證不覆蓋。
pub fn ensure_absent(path: &Path) -> Result<(), CoreError> {
    crate::files::ensure_absent(path, already_exists)
}

/// 寫出 xlsx（design D6）：先在記憶體中產生，再以 `create_new` 寫檔，目的檔已存在時不覆蓋。
/// `version` 欄設為文字格式，避免 Excel 把 `7.10` 轉成數字 `7.1`。
pub fn write_sheets(path: &Path, sheets: &[SheetData<'_>]) -> Result<(), CoreError> {
    let xlsx_error = |e: rust_xlsxwriter::XlsxError| {
        CoreError::Invalid(format!("無法寫出 {}：{e}", path.display()))
    };
    let mut book = Workbook::new();
    let bold = Format::new().set_bold();
    let text = Format::new().set_num_format("@");
    for sheet in sheets {
        let ws = book.add_worksheet();
        ws.set_name(sheet.name).map_err(xlsx_error)?;
        for (col, header) in sheet.headers.iter().enumerate() {
            let col = u16::try_from(col).map_err(|_| CoreError::Invalid("欄位過多".into()))?;
            ws.set_column_width(col, HEADER_COLUMN_WIDTH)
                .map_err(xlsx_error)?;
            if StandardField::parse(header) == Some(StandardField::Version) {
                ws.set_column_format(col, &text).map_err(xlsx_error)?;
            }
            ws.write_string_with_format(0, col, header, &bold)
                .map_err(xlsx_error)?;
        }
        for (r, row) in sheet.rows.iter().enumerate() {
            let r = u32::try_from(r + 1).map_err(|_| CoreError::Invalid("列數過多".into()))?;
            for (c, value) in row.iter().enumerate() {
                let c = u16::try_from(c).map_err(|_| CoreError::Invalid("欄位過多".into()))?;
                if !value.is_empty() {
                    ws.write_string(r, c, value).map_err(xlsx_error)?;
                }
            }
        }
    }
    let bytes = book.save_to_buffer().map_err(xlsx_error)?;
    crate::files::write_new(path, &bytes, already_exists)
}
