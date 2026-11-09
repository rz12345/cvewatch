//! 讀取 ISMS 資訊資產清冊（17 design D2）：依表頭文字找出表頭列與欄位，不依固定列號。
//!
//! - 欄名正規化：NFKC、去除空白與換行、去除括號與括號內的文字（「機密性\n(A)」→「機密性」）
//! - 工作表：優先「資訊資產清冊」，沒有（或它沒有表頭）時依序找第一個在前 10 列有表頭的工作表
//! - 兩欄「資產編號」依欄位順序為系統代碼與流水號
//! - `#N/A` 等錯誤值視為空白；只保留第二欄資產編號非空的列

use unicode_normalization::UnicodeNormalization;

use crate::error::CoreError;

/// 優先使用的工作表名稱。
pub const ISMS_SHEET_NAME: &str = "資訊資產清冊";
/// 在前幾列中找表頭。
pub const HEADER_SEARCH_ROWS: usize = 10;

const ID: &str = "資產編號";
const CATEGORY: &str = "資產類別";
const SYSTEM_NAME: &str = "資產說明";
const DESCRIPTION: &str = "資產描述";
const CONFIDENTIALITY: &str = "機密性";
const INTEGRITY: &str = "完整性";
const AVAILABILITY: &str = "可用性";
const RESPONSIBLE_UNIT: &str = "權責單位";
const CUSTODIAN: &str = "保管人";
const USER_UNIT: &str = "使用單位";
const COMPLIANCE: &str = "法規遵循性";
const PERSONAL_DATA: &str = "是否含個人資料";

/// ISMS 資訊資產清冊 v2.1 的 16 欄表頭（讀取端測試與 ISMS 匯出共用；23 design D2）。
pub const ISMS_V21_HEADERS: [&str; 16] = [
    "資產編號",
    "資產編號",
    "資產類別",
    "資產說明",
    "資產名稱",
    "資產描述",
    "權責單位",
    "保管人",
    "使用單位",
    "機密性\n(A)",
    "完整性\n(B)",
    "可用性\n(C)",
    "資產\n價值\n(MAX ABC)",
    "法規遵循性(D)",
    "資通系統\n防護等級\n(MAX ABCD)",
    "是否含\n個人資料",
];

/// 判定為表頭列的欄名（同時出現）。
pub(crate) const HEADER_MARKERS: [&str; 3] = [ID, CATEGORY, DESCRIPTION];

/// 欄名正規化（見模組說明）。
pub fn normalize_header(raw: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in raw.nfkc() {
        match c {
            '(' | '[' | '【' | '〔' => depth += 1,
            ')' | ']' | '】' | '〕' => depth = depth.saturating_sub(1),
            c if depth > 0 || c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    out
}

/// 各欄位在表頭中的位置（0 起算）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsmsColumns {
    pub system_id: usize,
    pub serial: usize,
    pub category: usize,
    pub system_name: usize,
    pub description: usize,
    pub confidentiality: usize,
    pub integrity: usize,
    pub availability: usize,
    pub responsible_unit: Option<usize>,
    pub custodian: Option<usize>,
    pub user_unit: Option<usize>,
    pub compliance: Option<usize>,
    pub personal_data: Option<usize>,
}

/// ISMS 清冊的一個資產列（各欄原文，已去除前後空白，錯誤值為空白）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IsmsRow {
    /// 試算表中的列號（1 起算）
    pub number: usize,
    pub system_id: String,
    pub serial: String,
    pub category: String,
    pub system_name: String,
    pub description: String,
    pub confidentiality: String,
    pub integrity: String,
    pub availability: String,
    pub responsible_unit: String,
    pub custodian: String,
    pub user_unit: String,
    pub compliance: String,
    pub personal_data: String,
}

/// 讀取結果：資料工作表名稱、表頭列號與資產列。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IsmsBook {
    pub sheet: String,
    pub header_row: usize,
    pub rows: Vec<IsmsRow>,
}

/// 一個工作表的文字內容：（列號, 各欄文字），欄位以 A 欄為 0。
pub type Grid = Vec<(usize, Vec<String>)>;

fn is_header_row(cells: &[String]) -> bool {
    let names: Vec<String> = cells.iter().map(|c| normalize_header(c)).collect();
    HEADER_MARKERS.iter().all(|m| names.iter().any(|n| n == m))
}

/// 前 10 列中的表頭列（在 `grid` 中的位置）。
pub(super) fn find_header(grid: &Grid) -> Option<usize> {
    grid.iter()
        .take_while(|(number, _)| *number <= HEADER_SEARCH_ROWS)
        .position(|(_, cells)| is_header_row(cells))
}

/// 依表頭決定欄位位置；缺少必要欄位時列出全部缺少的欄名。
pub fn locate_columns(headers: &[String]) -> Result<IsmsColumns, Vec<String>> {
    let names: Vec<String> = headers.iter().map(|h| normalize_header(h)).collect();
    let find = |name: &str| names.iter().position(|n| n == name);
    let ids: Vec<usize> = (0..names.len()).filter(|&i| names[i] == ID).collect();
    let mut missing = Vec::new();
    if ids.len() < 2 {
        missing.push(format!("{ID}（需要兩欄：系統代碼與流水號）"));
    }
    let mut need = |name: &str| {
        let at = find(name);
        if at.is_none() {
            missing.push(name.to_string());
        }
        at.unwrap_or_default()
    };
    let columns = IsmsColumns {
        system_id: ids.first().copied().unwrap_or_default(),
        serial: ids.get(1).copied().unwrap_or_default(),
        category: need(CATEGORY),
        system_name: need(SYSTEM_NAME),
        description: need(DESCRIPTION),
        confidentiality: need(CONFIDENTIALITY),
        integrity: need(INTEGRITY),
        availability: need(AVAILABILITY),
        responsible_unit: find(RESPONSIBLE_UNIT),
        custodian: find(CUSTODIAN),
        user_unit: find(USER_UNIT),
        compliance: find(COMPLIANCE),
        personal_data: find(PERSONAL_DATA),
    };
    if missing.is_empty() {
        Ok(columns)
    } else {
        Err(missing)
    }
}

pub(super) fn not_isms(source: &str) -> CoreError {
    CoreError::Invalid(format!(
        "{source} 找不到 ISMS 資訊資產清冊的表頭（前 {HEADER_SEARCH_ROWS} 列中沒有同時含「{ID}」「{CATEGORY}」「{DESCRIPTION}」的列）"
    ))
}

/// 工作表的檢查順序：[`ISMS_SHEET_NAME`] 優先，其餘依原順序。
pub(super) fn candidate_order(names: &[String]) -> Vec<usize> {
    let preferred = names
        .iter()
        .position(|name| normalize_header(name) == ISMS_SHEET_NAME);
    preferred
        .into_iter()
        .chain((0..names.len()).filter(|i| Some(*i) != preferred))
        .collect()
}

fn cell(cells: &[String], i: usize) -> String {
    cells.get(i).cloned().unwrap_or_default()
}

fn optional(cells: &[String], i: Option<usize>) -> String {
    i.map(|i| cell(cells, i)).unwrap_or_default()
}

fn to_row(number: usize, cells: &[String], c: &IsmsColumns) -> IsmsRow {
    IsmsRow {
        number,
        system_id: cell(cells, c.system_id),
        serial: cell(cells, c.serial),
        category: cell(cells, c.category),
        system_name: cell(cells, c.system_name),
        description: cell(cells, c.description),
        confidentiality: cell(cells, c.confidentiality),
        integrity: cell(cells, c.integrity),
        availability: cell(cells, c.availability),
        responsible_unit: optional(cells, c.responsible_unit),
        custodian: optional(cells, c.custodian),
        user_unit: optional(cells, c.user_unit),
        compliance: optional(cells, c.compliance),
        personal_data: optional(cells, c.personal_data),
    }
}

/// 以 `header_at`（在 `grid` 中的位置）為表頭讀出資產列；缺少必要欄位時列出缺少的欄名。
pub(super) fn parse_sheet(
    name: &str,
    grid: &Grid,
    header_at: usize,
) -> Result<IsmsBook, CoreError> {
    let (header_row, headers) = &grid[header_at];
    let columns = locate_columns(headers).map_err(|missing| {
        CoreError::Invalid(format!(
            "ISMS 資訊資產清冊（工作表「{name}」第 {header_row} 列）缺少欄位：{}",
            missing.join("、")
        ))
    })?;
    let rows = grid[header_at + 1..]
        .iter()
        .map(|(number, cells)| to_row(*number, cells, &columns))
        .filter(|r| !r.serial.is_empty())
        .collect();
    Ok(IsmsBook {
        sheet: name.to_string(),
        header_row: *header_row,
        rows,
    })
}

/// 由各工作表的文字內容讀出 ISMS 清冊（不需要檔案，供單元測試使用）：依
/// [`candidate_order`] 找第一個在前 10 列有表頭的工作表。`source` 為錯誤訊息中的檔名。
pub fn parse_isms_sheets(source: &str, sheets: &[(String, Grid)]) -> Result<IsmsBook, CoreError> {
    let names: Vec<String> = sheets.iter().map(|(n, _)| n.clone()).collect();
    for i in candidate_order(&names) {
        let (name, grid) = &sheets[i];
        if let Some(header_at) = find_header(grid) {
            return parse_sheet(name, grid, header_at);
        }
    }
    Err(not_isms(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|s| s.to_string()).collect()
    }

    fn grid(rows: &[&[&str]]) -> Grid {
        rows.iter()
            .enumerate()
            .map(|(i, r)| (i + 1, strings(r)))
            .collect()
    }

    fn isms_grid() -> Grid {
        let data: [&str; 16] = [
            "NET-",
            "HW-001",
            "HW02",
            "網路服務",
            "通訊設備",
            "Switch",
            "網路組",
            "王小明",
            "",
            "2",
            "3",
            "4",
            "4",
            "",
            "",
            "N",
        ];
        grid(&[
            &["文件編號：XX-0000-01"],
            &["填表日期", "2026/01/01"],
            &["填表單位", "測試單位"],
            &ISMS_V21_HEADERS,
            &data,
            &[
                "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "",
            ],
            &["註：資產編號 = 系統代碼 + 大類 + 流水號"],
        ])
    }

    #[test]
    fn header_names_are_normalized() {
        assert_eq!(normalize_header("機密性\n(A)"), "機密性");
        assert_eq!(normalize_header("資產\n價值\n(MAX ABC)"), "資產價值");
        assert_eq!(normalize_header("法規遵循性（D）"), "法規遵循性");
        assert_eq!(normalize_header(" 是否含\n個人資料 "), "是否含個人資料");
    }

    #[test]
    fn header_below_document_information_rows() {
        let book = parse_isms_sheets("x.xlsx", &[(ISMS_SHEET_NAME.into(), isms_grid())]).unwrap();
        assert_eq!(book.header_row, 4);
        assert_eq!(book.rows.len(), 1, "空白列與說明列不是資產列");
        let r = &book.rows[0];
        assert_eq!(r.number, 5);
        assert_eq!(
            (r.system_id.as_str(), r.serial.as_str()),
            ("NET-", "HW-001")
        );
        assert_eq!(r.availability, "4");
        assert_eq!(r.personal_data, "N");
        assert_eq!(r.custodian, "王小明");
    }

    #[test]
    fn missing_availability_is_reported() {
        let headers: Vec<&str> = ISMS_V21_HEADERS
            .iter()
            .copied()
            .filter(|h| !h.starts_with("可用性"))
            .collect();
        let sheets = [("資訊資產清冊".to_string(), grid(&[&headers]))];
        let e = parse_isms_sheets("x.xlsx", &sheets)
            .unwrap_err()
            .to_string();
        assert!(e.contains("缺少欄位：可用性"), "{e}");
        assert!(e.contains("第 1 列"), "{e}");
    }

    #[test]
    fn a_single_id_column_is_missing_a_column() {
        let missing = locate_columns(&strings(&ISMS_V21_HEADERS[1..])).unwrap_err();
        assert_eq!(missing, vec!["資產編號（需要兩欄：系統代碼與流水號）"]);
    }

    #[test]
    fn options_sheet_is_not_selected() {
        let options = grid(&[&[
            "資產名稱",
            "資產類別",
            "資產描述",
            "機密性",
            "完整性",
            "可用性",
            "資產編碼",
        ]]);
        let sheets = [
            ("選項(勿刪)".to_string(), options.clone()),
            ("工作表1".to_string(), isms_grid()),
        ];
        let book = parse_isms_sheets("x.xlsx", &sheets).unwrap();
        assert_eq!(book.sheet, "工作表1", "依序找第一個有表頭的工作表");
        let e = parse_isms_sheets("x.xlsx", &[("選項(勿刪)".to_string(), options)])
            .unwrap_err()
            .to_string();
        assert!(e.contains("找不到 ISMS 資訊資產清冊的表頭"), "{e}");
    }

    #[test]
    fn preferred_sheet_name_wins() {
        let sheets = [
            ("其他".to_string(), isms_grid()),
            (ISMS_SHEET_NAME.to_string(), isms_grid()),
        ];
        assert_eq!(
            parse_isms_sheets("x.xlsx", &sheets).unwrap().sheet,
            ISMS_SHEET_NAME
        );
    }

    #[test]
    fn header_after_row_ten_is_not_found() {
        let mut g: Grid = (1..=10).map(|n| (n, strings(&["說明"]))).collect();
        g.push((11, strings(&ISMS_V21_HEADERS)));
        assert!(parse_isms_sheets("x.xlsx", &[("a".into(), g)]).is_err());
    }
}
