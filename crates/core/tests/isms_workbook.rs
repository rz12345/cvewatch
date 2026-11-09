//! 17 任務 1.3：以 fixture 讀取 ISMS 資訊資產清冊（表頭位置、說明列、非 ISMS 清冊、缺欄）。

use std::path::PathBuf;

use cvewatch_core::assets::isms::workbook_read::read_isms_workbook;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

#[test]
fn isms_header_below_document_information_rows() {
    let book = read_isms_workbook(&fixture("isms_workbook.xlsx")).unwrap();
    assert_eq!(book.sheet, "資訊資產清冊");
    assert_eq!(book.header_row, 4);
    let first = &book.rows[0];
    assert_eq!(first.number, 5, "從第 5 列開始讀取資產");
    assert_eq!(
        (first.system_id.as_str(), first.serial.as_str()),
        ("NET-", "HW-001")
    );
    assert_eq!(first.description, "Core: Aruba XYZ-100 *2、\nZyxel ABCD *1");
    assert_eq!(first.compliance, "－");
}

#[test]
fn isms_footnote_and_blank_formula_rows_are_not_assets() {
    let book = read_isms_workbook(&fixture("isms_workbook.xlsx")).unwrap();
    let numbers: Vec<usize> = book.rows.iter().map(|r| r.number).collect();
    assert_eq!(
        numbers,
        (5..=15).collect::<Vec<_>>(),
        "第 16–18 列不是資產列"
    );
    assert!(book.rows.iter().all(|r| !r.serial.is_empty()));
}

#[test]
fn isms_not_an_isms_workbook() {
    let e = read_isms_workbook(&fixture("isms_not_workbook.xlsx"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("找不到 ISMS 資訊資產清冊的表頭"), "{e}");
    // cvewatch 清冊也不是 ISMS 清冊
    let e = read_isms_workbook(&fixture("inventory_basic.xlsx"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("找不到 ISMS 資訊資產清冊的表頭"), "{e}");
}

#[test]
fn isms_missing_required_column() {
    let e = read_isms_workbook(&fixture("isms_missing_column.xlsx"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("缺少欄位：可用性"), "{e}");
}

#[test]
fn isms_vertically_merged_system_code() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("merged.xlsx");
    let mut book = rust_xlsxwriter::Workbook::new();
    let ws = book.add_worksheet();
    ws.set_name("資訊資產清冊").unwrap();
    let headers = [
        "資產編號",
        "資產編號",
        "資產類別",
        "資產說明",
        "資產描述",
        "機密性",
        "完整性",
        "可用性",
    ];
    for (c, h) in headers.iter().enumerate() {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    let plain = rust_xlsxwriter::Format::new();
    ws.merge_range(1, 0, 2, 0, "NET-", &plain).unwrap();
    ws.merge_range(1, 3, 2, 3, "網路服務", &plain).unwrap();
    for (r, serial) in [(1, "HW-001"), (2, "HW-002")] {
        ws.write_string(r, 1, serial).unwrap();
        ws.write_string(r, 2, "HW02").unwrap();
        ws.write_string(r, 4, "PAN-OS").unwrap();
    }
    book.save(&path).unwrap();
    let rows = read_isms_workbook(&path).unwrap().rows;
    let ids: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|r| {
            (
                r.system_id.as_str(),
                r.serial.as_str(),
                r.system_name.as_str(),
            )
        })
        .collect();
    assert_eq!(
        ids,
        [
            ("NET-", "HW-001", "網路服務"),
            ("NET-", "HW-002", "網路服務")
        ]
    );
}
