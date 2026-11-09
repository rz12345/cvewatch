//! `gen_inventory_fixtures` 的生命週期清冊（22；由主檔移出以維持檔案長度）。

/// 22 的生命週期日期與 `eol` 工作表（虛構資產與型號）：`inventory_lifecycle.xlsx`。
/// SRV-01 的 `warranty_end` 寫成 Excel 日期儲存格（spec Spreadsheet date cell），其餘為文字。
pub fn write(dir: &std::path::Path) {
    use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};

    let path = dir.join("inventory_lifecycle.xlsx");
    let _ = std::fs::remove_file(&path);
    let mut book = Workbook::new();
    let assets = book.add_worksheet();
    assets.set_name("assets").expect("assets");
    let head = [
        "asset_tag",
        "name",
        "category",
        "product",
        "version",
        "brand",
        "model",
        "warranty_end",
        "support_end",
        "license_end",
    ];
    let data: [[&str; 10]; 3] = [
        [
            "FW-EDGE", "fw-edge", "HW03", "PAN-OS", "10.2.9", "", "", "", "", "",
        ],
        [
            "SRV-01",
            "esx01",
            "HW01",
            "ESXi",
            "8.0 U3c",
            "",
            "",
            "",
            "2027-06-30",
            "",
        ],
        [
            "SW-CORE",
            "sw-core",
            "HW02",
            "AOS-CX",
            "10.13.1000",
            "aruba",
            "xyz100",
            "2026/12/31",
            "",
            "",
        ],
    ];
    for (c, h) in head.iter().enumerate() {
        assets.write_string(0, c as u16, *h).expect("header");
    }
    for (r, row) in data.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            if !v.is_empty() {
                assets
                    .write_string(r as u32 + 1, c as u16, *v)
                    .expect("cell");
            }
        }
    }
    let date = ExcelDateTime::from_ymd(2027, 12, 31).expect("date");
    let shown = Format::new().set_num_format("yyyy/mm/dd");
    assets
        .write_datetime_with_format(2, 7, &date, &shown)
        .expect("date cell");
    let eol = book.add_worksheet();
    eol.set_name("EOL").expect("eol");
    let eol_rows = [
        ["asset_tag", "product", "eol", "source"],
        ["SW-CORE", "", "2027-06-30", "原廠延長支援函"],
        ["SRV-01", "ESXi", "2028-12-31", "延長支援合約"],
    ];
    for (r, row) in eol_rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            if !v.is_empty() {
                eol.write_string(r as u32, c as u16, *v).expect("eol cell");
            }
        }
    }
    let bytes = book.save_to_buffer().expect("save");
    std::fs::write(&path, bytes).expect("inventory_lifecycle.xlsx");
    println!("wrote {}", path.display());
}
