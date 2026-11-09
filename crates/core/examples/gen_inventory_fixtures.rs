//! 產生 `tests/fixtures/inventory_*.xlsx`（測試用清冊）與 `inventory_ips.csv`。
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

/// 20 的識別欄位（接在 [`ASSET_HEADERS`] 之後）。
const IDENTITY_HEADERS: [&str; 6] = [
    "brand",
    "model",
    "serial_number",
    "property_number",
    "hostname",
    "management_url",
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

    // 第 12 列缺少 product、第 5 列曝露度不合法（HOST-04）；version 空白為版本待補，不是錯誤（18）
    // 表頭為第 1 列，HOST-01..HOST-10 為第 2–11 列，缺 product 的列為第 12 列
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
    let mut last = rows(&[&["HOST-11", "host11", "HW01", "", "12.7"]]).remove(0);
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

    write_isms_fixtures();
    write_identity_fixtures();
}

/// 一列 assets：前 6 欄、空白的 C/I/A 與 ISMS 管理欄位，再接 6 個識別欄位。
fn identity_row(head: [&str; 6], identity: [&str; 6]) -> Vec<String> {
    let mut r: Vec<&str> = head.to_vec();
    r.extend(vec![""; ASSET_HEADERS.len() - head.len()]);
    r.extend(identity);
    r.iter().map(|s| s.to_string()).collect()
}

/// 20 的識別欄位與 IP（虛構的型號、序號、財產編號與文件保留位址）：
/// `inventory_identity.xlsx`（含 `ips` 工作表）與 CSV 的 IP 檔 `inventory_ips.csv`。
fn write_identity_fixtures() {
    let mut asset_headers = headers(&ASSET_HEADERS);
    asset_headers.extend(headers(&IDENTITY_HEADERS));
    let assets = vec![
        identity_row(
            [
                "FW-EDGE",
                "fw-edge",
                "HW03",
                "PAN-OS",
                "11.1.4-h7",
                "internet",
            ],
            [
                "Palo Alto Networks",
                "XYZ-100",
                "SN0001",
                "P-0001",
                "fw-edge",
                "https://192.0.2.1/",
            ],
        ),
        identity_row(
            ["SRV-01", "esx01", "HW01", "ESXi", "8.0 U3c", "mgmt"],
            ["", "", "", "P-0002", "srv01", ""],
        ),
        identity_row(
            ["NAS-01", "nas01", "HW05", "DSM", "7.2.1-69057", "internal"],
            ["", "", "", "", "", ""],
        ),
    ];
    write(
        "inventory_identity.xlsx",
        &[
            SheetData {
                name: "assets",
                headers: asset_headers,
                rows: assets,
            },
            SheetData {
                name: "ips",
                headers: headers(&["asset_tag", "ip", "purpose"]),
                rows: rows(&[
                    &["FW-EDGE", "192.0.2.1", "管理"],
                    &["SRV-01", "192.0.2.10", "管理"],
                    &["SRV-01", "2001:db8::10", ""],
                ]),
            },
        ],
    );
    // Excel 另存的「CSV UTF-8」帶 BOM
    let path = fixtures_dir().join("inventory_ips.csv");
    std::fs::write(
        &path,
        "\u{feff}asset_tag,ip,purpose\r\nSRV-01,192.0.2.10,\r\n",
    )
    .expect("inventory_ips.csv");
    println!("wrote {}", path.display());
}

/// ISMS 資訊資產清冊的表頭（17；與機構 v2.0／v2.1 相同的欄名寫法，含換行與括號註記）。
const ISMS_HEADERS: [&str; 16] = [
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

/// 文件資訊列（第 1–3 列；全為虛構內容）。第 1 列寫在 headers。
fn isms_document_rows() -> (Vec<String>, Vec<Vec<String>>) {
    (
        headers(&[
            "文件編號：XX-0000-01",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "機密等級：一般",
        ]),
        rows(&[&["填表日期", "2026/01/01"], &["填表單位", "測試單位"]]),
    )
}

/// 資產列（第 5 列起）：多行描述、同產品兩行、`－`、PE/IF/SE 列、大類不符列、
/// 含 `#N/A` 的空白公式列與編碼說明列。系統代碼、人名與型號都是虛構的。
fn isms_asset_rows() -> Vec<Vec<String>> {
    let unit = ["網路組", "王小明", "網路組"];
    let web = ["資訊組", "王小明", "資訊組"];
    // 欄位順序同 ISMS_HEADERS：編號兩欄、[類別, 資產說明, 資產名稱]、描述、
    // [權責單位, 保管人, 使用單位]、[C, I, A, 資產價值, 法規遵循性, 防護等級, 個資]
    let row = |id: [&str; 2], class: [&str; 3], desc: &str, who: [&str; 3], r: [&str; 7]| {
        id.iter()
            .chain(&class)
            .chain(&[desc])
            .chain(&who)
            .chain(&r)
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
    };
    let mut out = vec![
        row(
            ["NET-", "HW-001"],
            ["HW02", "網路服務", "通訊設備"],
            "Core: Aruba XYZ-100 *2、\nZyxel ABCD *1",
            unit,
            ["2", "3", "4", "4", "－", "", "N"],
        ),
        row(
            ["NET-", "HW-002"],
            ["HW03", "網路服務", "資安設備"],
            "PAN-OS *1",
            unit,
            ["3", "3", "4", "4", "", "", "N"],
        ),
        row(
            ["NET-", "HW-003"],
            ["HW05", "網路服務", "儲存設備"],
            "Synology XYZ-200 *1",
            unit,
            ["2", "3", "4", "4", "", "", "N"],
        ),
        row(
            ["NET-", "HW-004"],
            ["HW02", "網路服務", "通訊設備"],
            "Core: Aruba XYZ-100 *2、Edge: Aruba XYZ-100 *9",
            unit,
            ["2", "3", "4", "4", "2", "4", "N"],
        ),
        row(
            ["NET-", "SW-001"],
            ["SW07", "網路服務組", "一般套裝軟體"],
            "備份軟體 Veeam Backup & Replication",
            unit,
            ["2", "2", "2", "2", "", "", "N"],
        ),
        row(
            ["WEB-", "SW-002"],
            ["SW01", "官網服務", "應用系統"],
            "官網系統",
            web,
            ["3", "3", "3", "3", "3", "3", "Y"],
        ),
        row(
            ["WEB-", "PE-001"],
            ["PE01", "官網服務", "人員"],
            "系統管理員",
            web,
            ["2", "2", "2", "2", "", "", "N"],
        ),
        row(
            ["WEB-", "IF-001"],
            ["IF01", "官網服務", "資料"],
            "網站原始碼",
            web,
            ["2", "2", "2", "2", "", "", "N"],
        ),
        row(
            ["WEB-", "IF-002"],
            ["IF02", "官網服務", "文件"],
            "設計文件",
            web,
            ["2", "2", "2", "2", "", "", "N"],
        ),
        row(
            ["WEB-", "SE-001"],
            ["SE01", "官網服務", "服務"],
            "網站代管服務",
            web,
            ["2", "2", "2", "2", "", "", "N"],
        ),
        // 大類不符：編號為 SE、類別為 SW01
        row(
            ["WEB-", "SE-003"],
            ["SW01", "官網服務", "應用系統"],
            "網站管理後台",
            web,
            ["3", "3", "3", "3", "", "", "N"],
        ),
    ];
    // v2.1 未填列：公式欄沒有值（#N/A），第二欄資產編號空白
    let mut formula = vec![String::new(); ISMS_HEADERS.len()];
    for i in [2, 4, 9, 10, 11, 12] {
        formula[i] = "#N/A".into();
    }
    out.push(formula);
    out.push(vec!["註：資產編號由系統代碼、大類與流水號組成".into()]);
    out.push(vec!["註：C/I/A 依子類別範本值評定".into()]);
    out
}

/// 「選項(勿刪)」工作表（下拉選單來源；有「資產類別」「資產描述」但沒有「資產編號」）。
fn isms_options_sheet() -> SheetData<'static> {
    SheetData {
        name: "選項(勿刪)",
        headers: headers(&[
            "資產名稱",
            "資產類別",
            "資產描述",
            "機密性",
            "完整性",
            "可用性",
            "資產價值",
            "是否含\n個人資料",
            "",
            "",
            "權責單位",
            "保管人",
            "使用單位",
            "使用說明",
            "",
            "資產編碼",
            "資產編碼說明",
        ]),
        rows: rows(&[
            &[
                "伺服器",
                "HW01",
                "大型主機",
                "2",
                "3",
                "4",
                "4",
                "N",
                "",
                "",
                "網路組",
                "",
                "",
                "",
                "",
                "NET",
                "網路服務",
            ],
            &[
                "應用系統",
                "SW01",
                "自行開發之系統",
                "",
                "",
                "",
                "",
                "N",
                "",
                "",
                "資訊組",
                "",
                "",
                "",
                "",
                "WEB",
                "官網服務",
            ],
        ]),
    }
}

fn write_isms_fixtures() {
    let (first, info) = isms_document_rows();
    let mut sheet = info.clone();
    sheet.push(headers(&ISMS_HEADERS));
    sheet.extend(isms_asset_rows());
    write(
        "isms_workbook.xlsx",
        &[
            SheetData {
                name: "資訊資產清冊",
                headers: first.clone(),
                rows: sheet,
            },
            isms_options_sheet(),
        ],
    );

    // 缺少「可用性」欄
    let without: Vec<&str> = ISMS_HEADERS
        .iter()
        .copied()
        .filter(|h| !h.starts_with("可用性"))
        .collect();
    let mut sheet = info;
    sheet.push(headers(&without));
    write(
        "isms_missing_column.xlsx",
        &[SheetData {
            name: "資訊資產清冊",
            headers: first,
            rows: sheet,
        }],
    );

    // 不是 ISMS 清冊
    write(
        "isms_not_workbook.xlsx",
        &[SheetData {
            name: "工作表1",
            headers: headers(&["項目", "數量", "說明"]),
            rows: rows(&[&["交換器", "3", "機房"], &["印表機", "1", "辦公室"]]),
        }],
    );
}
