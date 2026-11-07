//! 產生 `tests/fixtures/inventory_*.xlsx`（測試用清冊）。
//!
//! 執行：`cargo run -p cvewatch-core --example gen_inventory_fixtures`
//! 輸出固定內容，重新產生後 git diff 應只有 xlsx 內部時間戳記不同。

use std::path::{Path, PathBuf};

use cvewatch_core::assets::template::{write_sheets, SheetData};

const ASSET_HEADERS: [&str; 7] = [
    "asset_tag",
    "name",
    "category",
    "product",
    "version",
    "exposure",
    "criticality",
];

fn rows(data: &[&[&str]]) -> Vec<Vec<String>> {
    data.iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect()
}

fn headers(h: &[&str]) -> Vec<String> {
    h.iter().map(|s| s.to_string()).collect()
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn write(name: &str, sheets: &[SheetData<'_>]) {
    let path = fixtures_dir().join(name);
    let _ = std::fs::remove_file(&path);
    write_sheets(&path, sheets).unwrap_or_else(|e| panic!("{name}: {e}"));
    println!("wrote {}", path.display());
}

fn base_assets() -> Vec<Vec<String>> {
    rows(&[
        &[
            "FW-01",
            "fw01",
            "security",
            "PAN-OS",
            "11.1.4-h7",
            "internet",
            "3",
        ],
        &[
            "R6515-01",
            "esx01",
            "virtualization",
            "ESXi",
            "8.0 U3c",
            "mgmt",
            "3",
        ],
        &[
            "NAS-01",
            "nas01",
            "storage",
            "Synology DSM",
            "7.2.1-69057",
            "internal",
            "2",
        ],
        &["NAS-02", "nas02", "storage", "DSM", "7.2.1-69057", "", ""],
        &[
            "NAS-03",
            "nas03",
            "storage",
            "Synolgy DSM",
            "7.2.2-72806",
            "internal",
            "1",
        ],
        &[
            "FW-02",
            "fw02",
            "security",
            "Palo Alto PA-460",
            "11.1 latest",
            "dmz",
            "3",
        ],
    ])
}

fn base_components() -> Vec<Vec<String>> {
    rows(&[
        &["R6515-01", "iDRAC9", "7.10.30.00"],
        &["R6515-01", "PowerEdge R6515 BIOS", "2.16.0"],
    ])
}

fn main() {
    std::fs::create_dir_all(fixtures_dir()).expect("fixtures dir");
    let components_headers = headers(&["asset_tag", "product", "version"]);

    write(
        "inventory_basic.xlsx",
        &[
            SheetData {
                name: "assets",
                headers: headers(&ASSET_HEADERS),
                rows: base_assets(),
            },
            SheetData {
                name: "components",
                headers: components_headers.clone(),
                rows: base_components(),
            },
        ],
    );

    // 第二版：FW-01 與 NAS-01 升級、NAS-02 從清冊消失
    let v2: Vec<Vec<String>> = base_assets()
        .into_iter()
        .filter(|r| r[0] != "NAS-02")
        .map(|mut r| {
            match r[0].as_str() {
                "FW-01" => r[4] = "11.1.6".into(),
                "NAS-01" => r[4] = "7.2.2-72806".into(),
                _ => {}
            }
            r
        })
        .collect();
    write(
        "inventory_v2.xlsx",
        &[
            SheetData {
                name: "assets",
                headers: headers(&ASSET_HEADERS),
                rows: v2,
            },
            SheetData {
                name: "components",
                headers: components_headers.clone(),
                rows: base_components(),
            },
        ],
    );

    // 中文欄名（搭配 [import.columns]）
    write(
        "inventory_chinese.xlsx",
        &[SheetData {
            name: "Assets",
            headers: headers(&[
                "財產編號",
                "設備名稱",
                "類別",
                "產品",
                "版本",
                "曝露度",
                "重要性",
                "放置地點",
            ]),
            rows: rows(&[&[
                "FW-01",
                "fw01",
                "security",
                "PAN-OS",
                "11.1.4-h7",
                "internet",
                "3",
                "機房 A",
            ]]),
        }],
    );

    // 缺少 version 欄
    write(
        "inventory_missing_column.xlsx",
        &[SheetData {
            name: "assets",
            headers: headers(&["asset_tag", "name", "category", "product"]),
            rows: rows(&[&["FW-01", "fw01", "security", "PAN-OS"]]),
        }],
    );

    // 第 12 列缺少 version、第 5 列曝露度不合法（HOST-04）
    // 表頭為第 1 列，HOST-01..HOST-10 為第 2–11 列，缺 version 的列為第 12 列
    let mut bad: Vec<Vec<String>> = (1..=10)
        .map(|i| {
            let tag = format!("HOST-{i:02}");
            vec![
                tag.clone(),
                tag,
                "host".into(),
                "Debian".into(),
                "12.7".into(),
                "internal".into(),
                "2".into(),
            ]
        })
        .collect();
    bad[3][5] = "public".into();
    bad.push(rows(&[&["HOST-11", "host11", "host", "Debian", "", "", ""]]).remove(0));
    write(
        "inventory_bad_rows.xlsx",
        &[SheetData {
            name: "assets",
            headers: headers(&ASSET_HEADERS),
            rows: bad,
        }],
    );
}
