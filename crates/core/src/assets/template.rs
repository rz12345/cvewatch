//! 空白範本（`cvewatch asset template`）與共用的 xlsx 寫出（清冊匯出也使用）。

use std::path::Path;

use rust_xlsxwriter::{Format, Workbook};

use super::classes::AssetClasses;
use super::columns::{SheetKind, StandardField};
use crate::error::CoreError;

/// 子類別說明工作表（匯入時不讀）。
pub const CLASSES_SHEET: &str = "classes";
const CLASSES_HEADERS: [&str; 7] = [
    "code",
    "name",
    "description",
    "confidentiality",
    "integrity",
    "availability",
    "personal_data",
];

const HEADER_COLUMN_WIDTH: f64 = 18.0;

/// 一個要寫出的工作表。
pub struct SheetData<'a> {
    pub name: &'a str,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// 範本範例列（欄位順序同 [`SheetKind::columns`]；C/I/A 與 personal_data 留空表示依子類別）。
fn example_assets() -> Vec<Vec<String>> {
    let rows: [[&str; 27]; 3] = [
        [
            "FW-01",
            "fw01",
            "HW03",
            "PAN-OS",
            "11.1.4-h7",
            "internet",
            "",
            "",
            "",
            "SYS-HW-001",
            "SYS",
            "網管組",
            "王小明",
            "網管組",
            "",
            "",
            "Palo Alto Networks",
            "XYZ-100",
            "SN0001",
            "P-0001",
            "fw01",
            "https://192.0.2.1/",
            "",
            "",
            "",
            "機房 A",
            "",
        ],
        [
            "SRV-01",
            "esx01",
            "HW01",
            "ESXi",
            "8.0 U3c",
            "mgmt",
            "3",
            "",
            "",
            "",
            "",
            "系統組",
            "",
            "",
            "3",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "2027-12-31",
            "2027-06-30",
            "",
            "機房 A",
            "",
        ],
        [
            "NAS-01",
            "nas01",
            "HW05",
            "Synology DSM",
            "7.2.2-72806 Update 3",
            "internal",
            "",
            "",
            "",
            "",
            "",
            "系統組",
            "",
            "",
            "",
            "N",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "機房 B",
            "",
        ],
    ];
    rows.iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect()
}

/// `ips` 工作表的範例列（asset_tag、ip、purpose；20 design D13）。
fn example_ips() -> Vec<Vec<String>> {
    vec![vec!["FW-01".into(), "192.0.2.1".into(), "管理介面".into()]]
}

/// `eol` 工作表的範例列（asset_tag、product、eol、source；22 design D7）：沿用 assets 的範例資產，
/// 讓範本本身可以 `--dry-run` 匯入。
fn example_eol() -> Vec<Vec<String>> {
    vec![vec![
        "SRV-01".into(),
        "ESXi".into(),
        "2030-12-31".into(),
        "範例：延長支援合約".into(),
    ]]
}

/// `systems` 工作表的範例列（代碼與名稱）。
fn example_systems() -> Vec<Vec<String>> {
    vec![vec!["SYS".into(), "核心系統".into()]]
}

/// `classes` 工作表：目前生效的子類別（無範本值時 C/I/A 留空）。
pub fn class_sheet(classes: &AssetClasses) -> SheetData<'static> {
    let rows = classes
        .all()
        .iter()
        .map(|c| {
            let ratings = c.template.map_or([const { String::new() }; 3], |t| {
                t.to_array().map(|n| n.to_string())
            });
            let mut row = vec![c.code.to_string(), c.name.clone(), c.description.clone()];
            row.extend(ratings);
            row.push(super::export::personal_data_text(c.personal_data).to_string());
            row
        })
        .collect();
    SheetData {
        name: CLASSES_SHEET,
        headers: CLASSES_HEADERS.iter().map(|h| h.to_string()).collect(),
        rows,
    }
}

fn example_components() -> Vec<Vec<String>> {
    vec![vec!["SRV-01".into(), "iDRAC9".into(), "7.10.30.00".into()]]
}

/// 以文字格式寫出的欄位。
fn is_text_column(field: StandardField) -> bool {
    matches!(
        field,
        StandardField::Version
            | StandardField::WarrantyEnd
            | StandardField::SupportEnd
            | StandardField::LicenseEnd
            | StandardField::Eol
    )
}

/// 標準欄名表頭。
pub(crate) fn headers(kind: SheetKind) -> Vec<String> {
    kind.columns()
        .iter()
        .map(|f| f.as_str().to_string())
        .collect()
}

/// 寫出含 `assets`、`components`、`systems`、`ips` 與 `eol` 工作表（標準欄名與範例列）及
/// `classes` 子類別說明的範本；目的檔已存在時拒絕。
pub fn write_template(path: &Path, classes: &AssetClasses) -> Result<(), CoreError> {
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
            SheetData {
                name: SheetKind::Systems.sheet_name(),
                headers: headers(SheetKind::Systems),
                rows: example_systems(),
            },
            SheetData {
                name: SheetKind::Ips.sheet_name(),
                headers: headers(SheetKind::Ips),
                rows: example_ips(),
            },
            SheetData {
                name: SheetKind::Eol.sheet_name(),
                headers: headers(SheetKind::Eol),
                rows: example_eol(),
            },
            class_sheet(classes),
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
/// `version` 欄設為文字格式，避免 Excel 把 `7.10` 轉成數字 `7.1`；日期欄也設為文字格式，
/// 避免地區設定改變顯示（22 design D8）。
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
            if StandardField::parse(header).is_some_and(is_text_column) {
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
