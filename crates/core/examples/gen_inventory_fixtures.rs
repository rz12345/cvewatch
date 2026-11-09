//! 產生 `tests/fixtures/inventory_*.xlsx`（測試用清冊）。
//!
//! 執行：`cargo run -p cvewatch-core --example gen_inventory_fixtures`
//! 輸出固定內容，重新產生後 git diff 應只有 xlsx 內部時間戳記不同。

use std::path::{Path, PathBuf};

use cvewatch_core::assets::template::{write_sheets, SheetData};

const ASSET_HEADERS: [&str; 16] = [
    "asset_tag",
    "name",
    "category",
    "product",
    "version",
    "exposure",
    "confidentiality",
    "integrity",
    "availability",
    "isms_id",
    "system",
    "responsible_unit",
    "custodian",
    "user_unit",
    "compliance",
    "personal_data",
];

/// 16 之前的清冊：有 isms_id 與 owner 欄，沒有資產說明與 ISMS 管理欄位（相容測試用）。
const OWNER_HEADERS: [&str; 8] = [
    "asset_tag",
    "name",
    "category",
    "product",
    "version",
    "exposure",
    "isms_id",
    "owner",
];

/// 13 之前的清冊：英文 category 與重要性欄（相容測試用）。
const LEGACY_HEADERS: [&str; 7] = [
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

/// 新格式：子類別代碼（或名稱）、C/I/A 多數留空（依子類別範本值）；FW-01 的資產說明由
/// ISMS 編號帶出（`SYS`），ISMS 管理欄位只有 FW-01 與 SRV-01 有填（虛構的單位與人名）。
fn base_assets() -> Vec<Vec<String>> {
    let mut assets = rows(&[
        &[
            "FW-01",
            "fw01",
            "HW03",
            "PAN-OS",
            "11.1.4-h7",
            "internet",
            "",
            "",
            "",
            "SYS-HW-003",
        ],
        &[
            "SRV-01", "esx01", "HW01", "ESXi", "8.0 U3c", "mgmt", "3", "", "", "",
        ],
        &[
            "NAS-01",
            "nas01",
            "儲存設備",
            "Synology DSM",
            "7.2.1-69057",
            "internal",
            "",
            "",
            "",
            "",
        ],
        &[
            "NAS-02",
            "nas02",
            "HW05",
            "DSM",
            "7.2.1-69057",
            "",
            "",
            "",
            "",
            "",
        ],
        &[
            "NAS-03",
            "nas03",
            "hw05",
            "Synolgy DSM",
            "7.2.2-72806",
            "internal",
            "",
            "",
            "2",
            "",
        ],
        &[
            "FW-02",
            "fw02",
            "HW03",
            "Palo Alto PAN-OS",
            "11.1 latest",
            "dmz",
            "",
            "",
            "",
            "",
        ],
    ]);
    let admin: [(&str, [&str; 6]); 2] = [
        ("FW-01", ["", "網管組", "王小明", "網管組", "", "N"]),
        ("SRV-01", ["SYS", "系統組", "", "", "3", ""]),
    ];
    for row in &mut assets {
        let extra = admin
            .iter()
            .find(|(tag, _)| *tag == row[0])
            .map_or([""; 6], |(_, v)| *v);
        row.extend(extra.iter().map(|s| s.to_string()));
    }
    assets
}

fn base_systems() -> Vec<Vec<String>> {
    rows(&[&["SYS", "核心系統"]])
}

/// 13 之前的清冊內容（`inventory_legacy.xlsx`）。
fn legacy_assets() -> Vec<Vec<String>> {
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
            "SRV-01",
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
            "Palo Alto PAN-OS",
            "11.1 latest",
            "dmz",
            "3",
        ],
    ])
}

fn base_components() -> Vec<Vec<String>> {
    rows(&[
        &["SRV-01", "iDRAC9", "7.10.30.00"],
        &["SRV-01", "PowerEdge BIOS", "2.16.0"],
    ])
}

fn main() {
    std::fs::create_dir_all(fixtures_dir()).expect("fixtures dir");
    let components_headers = headers(&["asset_tag", "product", "version"]);
    let systems_headers = headers(&["code", "name"]);

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
            SheetData {
                name: "systems",
                headers: systems_headers.clone(),
                rows: base_systems(),
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
        "inventory_legacy.xlsx",
        &[
            SheetData {
                name: "assets",
                headers: headers(&LEGACY_HEADERS),
                rows: legacy_assets(),
            },
            SheetData {
                name: "components",
                headers: components_headers.clone(),
                rows: base_components(),
            },
        ],
    );

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
            SheetData {
                name: "systems",
                headers: systems_headers.clone(),
                rows: base_systems(),
            },
        ],
    );

    // 16 之前的清冊：owner 欄、沒有 system 與 ISMS 管理欄位
    write(
        "inventory_owner.xlsx",
        &[SheetData {
            name: "assets",
            headers: headers(&OWNER_HEADERS),
            rows: rows(&[
                &[
                    "FW-01",
                    "fw01",
                    "HW03",
                    "PAN-OS",
                    "11.1.4-h7",
                    "internet",
                    "SYS-HW-003",
                    "王小明",
                ],
                &[
                    "NAS-01",
                    "nas01",
                    "HW05",
                    "Synology DSM",
                    "7.2.1-69057",
                    "internal",
                    "",
                    "李小華",
                ],
            ]),
        }],
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
                "機密性",
                "放置地點",
            ]),
            rows: rows(&[&[
                "FW-01",
                "fw01",
                "資安設備",
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
            rows: rows(&[&["FW-01", "fw01", "HW03", "PAN-OS"]]),
        }],
    );

    // 第 12 列缺少 version、第 5 列曝露度不合法（HOST-04）
    // 表頭為第 1 列，HOST-01..HOST-10 為第 2–11 列，缺 version 的列為第 12 列
    let mut bad: Vec<Vec<String>> = (1..=10)
        .map(|i| {
            let tag = format!("HOST-{i:02}");
            let mut row = vec![
                tag.clone(),
                tag,
                "HW01".into(),
                "Debian".into(),
                "12.7".into(),
                "internal".into(),
            ];
            row.resize(ASSET_HEADERS.len(), String::new());
            row
        })
        .collect();
    bad[3][5] = "public".into();
    let mut last = rows(&[&["HOST-11", "host11", "HW01", "Debian", ""]]).remove(0);
    last.resize(ASSET_HEADERS.len(), String::new());
    bad.push(last);
    write(
        "inventory_bad_rows.xlsx",
        &[SheetData {
            name: "assets",
            headers: headers(&ASSET_HEADERS),
            rows: bad,
        }],
    );
}
