//! `conversion.rs` 的單元測試（17 任務 3.1、18 任務 4.2）。

use super::*;
use crate::assets::classes::{Cia, ClassCode};
use crate::assets::isms::describe::tests::catalog;
use crate::assets::isms::workbook::IsmsRow;
use crate::assets::model::Exposure;

fn isms_row(number: usize, id: (&str, &str), category: &str, name: &str, desc: &str) -> IsmsRow {
    IsmsRow {
        number,
        system_id: id.0.into(),
        serial: id.1.into(),
        category: category.into(),
        system_name: name.into(),
        description: desc.into(),
        ..IsmsRow::default()
    }
}

fn book(rows: Vec<IsmsRow>) -> IsmsBook {
    IsmsBook {
        sheet: "資訊資產清冊".into(),
        header_row: 4,
        rows,
    }
}

fn existing(
    tag: &str,
    isms: Option<&str>,
    status: AssetStatus,
    items: Vec<InventoryItem>,
) -> ExistingAsset {
    ExistingAsset {
        record: AssetRecord {
            asset_tag: tag.into(),
            name: tag.to_lowercase(),
            category: ClassCode::parse("HW03").unwrap(),
            location: Some("機房 A".into()),
            exposure: Exposure::Internet,
            cia: Cia::new(2, 3, 4),
            isms_id: isms.map(str::to_string),
            system_code: None,
            responsible_unit: None,
            custodian: None,
            user_unit: None,
            compliance: None,
            personal_data: false,
            brand: None,
            model: None,
            serial_number: None,
            property_number: None,
            hostname: None,
            management_url: None,
            notes: Some("既有備註".into()),
        },
        status,
        items,
    }
}

fn item(key: Option<&str>, product: &str, version: &str) -> InventoryItem {
    InventoryItem {
        key: key.map(str::to_string),
        product: product.into(),
        version: version.into(),
    }
}

fn system(code: &str, name: &str) -> AssetSystem {
    AssetSystem {
        code: SystemCode::parse(code).unwrap(),
        name: name.into(),
    }
}

fn run(rows: Vec<IsmsRow>, existing: Vec<ExistingAsset>) -> Conversion {
    let classes = AssetClasses::builtin();
    let catalog = catalog();
    let ctx = ConversionContext {
        existing,
        ..ConversionContext::empty(&classes, &catalog)
    };
    build_conversion(&book(rows), &ctx)
}

#[test]
fn mapping_a_storage_row() {
    let mut r = isms_row(
        7,
        ("NET-", "HW-003"),
        "HW05",
        "網路服務",
        "Synology XYZ-200 *1",
    );
    r.custodian = "王小明".into();
    r.confidentiality = "2".into();
    r.integrity = "3".into();
    r.availability = "4".into();
    r.personal_data = "N".into();
    let conv = run(vec![r], vec![]);
    let a = &conv.assets[0];
    assert_eq!(a.record.asset_tag, "NET-HW-003");
    assert_eq!(a.record.isms_id.as_deref(), Some("NET-HW-003"));
    assert_eq!(a.record.category.as_str(), "HW05");
    assert_eq!(a.record.system_code.as_deref(), Some("NET"));
    assert_eq!(a.record.name, "Synology XYZ-200");
    assert_eq!(a.record.custodian.as_deref(), Some("王小明"));
    assert_eq!(a.record.cia, Cia::new(2, 3, 4));
    assert!(!a.record.personal_data);
    assert_eq!(a.record.exposure, Exposure::Internal);
    assert!(!a.reused);
    assert_eq!(
        a.record.notes.as_deref(),
        Some("ISMS 描述：Synology XYZ-200 *1")
    );
    assert_eq!(a.items, vec![item(None, "Synology XYZ-200", "")]);
    assert_eq!(conv.systems, vec![system("NET", "網路服務")]);
    assert_eq!(conv.report.converted_new, 1);
    assert_eq!(conv.report.pending_components, 0, "未辨識產品不計入");
}

#[test]
fn two_names_for_one_system_code() {
    let conv = run(
        vec![
            isms_row(5, ("NET-", "HW-001"), "HW02", "網路服務", "PAN-OS"),
            isms_row(9, ("NET-", "HW-002"), "HW02", "網路服務組", "PAN-OS"),
            isms_row(10, ("WEB-", "HW-003"), "HW02", "", "PAN-OS"),
        ],
        vec![],
    );
    assert_eq!(
        conv.systems,
        vec![system("NET", "網路服務"), system("WEB", "WEB")],
        "都空白時以代碼為名稱"
    );
    assert_eq!(
        conv.report.system_name_conflicts,
        ["第 9 列的資產說明「網路服務組」與第 5 列不同，NET 使用「網路服務」"]
    );
}

#[test]
fn blank_system_name_uses_the_database_name() {
    let classes = AssetClasses::builtin();
    let catalog = catalog();
    let ctx = ConversionContext {
        systems: SystemDirectory::new(vec![system("NET", "網路")]),
        ..ConversionContext::empty(&classes, &catalog)
    };
    let rows = vec![isms_row(5, ("NET-", "HW-001"), "HW02", "", "PAN-OS")];
    let conv = build_conversion(&book(rows), &ctx);
    assert_eq!(conv.systems, vec![system("NET", "網路")]);
    assert!(conv.report.system_name_conflicts.is_empty());
}

#[test]
fn isms_id_already_in_cvewatch() {
    let mut r = isms_row(
        6,
        ("NET-", "HW-002"),
        "HW03",
        "網路服務",
        "PAN-OS *1、Zyxel ABCD",
    );
    r.confidentiality = "3".into();
    r.integrity = "3".into();
    r.availability = "4".into();
    r.custodian = "王小明".into();
    let fw = existing(
        "FW-01",
        Some("net-hw-002"),
        AssetStatus::Active,
        vec![
            item(Some("paloalto-panos"), "PAN-OS", "11.1.6"),
            item(None, "zyxel abcd", "1.0"),
        ],
    );
    let conv = run(vec![r], vec![fw]);
    let a = &conv.assets[0];
    assert_eq!(a.record.asset_tag, "FW-01");
    assert_eq!(a.record.name, "fw-01");
    assert_eq!(a.record.exposure, Exposure::Internet);
    assert_eq!(a.record.location.as_deref(), Some("機房 A"));
    assert_eq!(a.record.notes.as_deref(), Some("既有備註"));
    assert_eq!(
        a.record.isms_id.as_deref(),
        Some("net-hw-002"),
        "保留既有寫法"
    );
    assert_eq!(a.record.cia, Cia::new(3, 3, 4), "ISMS 欄位取自 ISMS 清冊");
    assert_eq!(a.record.custodian.as_deref(), Some("王小明"));
    assert!(a.reused);
    assert_eq!(
        a.items,
        vec![
            item(Some("paloalto-panos"), "PAN-OS", "11.1.6"),
            item(None, "zyxel abcd", "1.0")
        ],
        "既有元件保留版本，未辨識產品不重複"
    );
    assert_eq!(conv.report.converted_existing, 1);
    assert_eq!(conv.report.pending_components, 0);
}

#[test]
fn reused_asset_gains_new_described_products() {
    let fw = existing("FW-01", Some("NET-HW-002"), AssetStatus::Active, vec![]);
    let rows = vec![isms_row(
        6,
        ("NET-", "HW-002"),
        "HW03",
        "網路服務",
        "PAN-OS",
    )];
    let conv = run(rows, vec![fw]);
    assert_eq!(
        conv.assets[0].items,
        vec![item(Some("paloalto-panos"), "PAN-OS", "")]
    );
    assert_eq!(conv.report.pending_components, 1);
}

#[test]
fn existing_asset_with_blank_description_keeps_its_items() {
    let fw = existing(
        "FW-01",
        Some("NET-HW-002"),
        AssetStatus::Active,
        vec![item(Some("paloalto-panos"), "PAN-OS", "11.1.6")],
    );
    let rows = vec![isms_row(6, ("NET-", "HW-002"), "HW03", "網路服務", "")];
    let conv = run(rows, vec![fw]);
    assert_eq!(conv.assets.len(), 1, "既有資產照常更新 ISMS 欄位");
    assert_eq!(
        conv.assets[0].items,
        vec![item(Some("paloalto-panos"), "PAN-OS", "11.1.6")]
    );
    assert!(conv.report.skipped_rows.is_empty());
}

#[test]
fn unresolved_item_now_in_the_catalog_is_not_added_twice() {
    // 既有的未辨識產品「PAN-OS」（例如當時目錄還沒有別名）在規劃時會成為 paloalto-panos
    let fw = existing(
        "FW-01",
        Some("NET-HW-002"),
        AssetStatus::Active,
        vec![item(None, "PAN-OS", "11.1.6")],
    );
    let rows = vec![isms_row(
        6,
        ("NET-", "HW-002"),
        "HW03",
        "網路服務",
        "PAN-OS",
    )];
    let conv = run(rows, vec![fw]);
    assert_eq!(conv.assets[0].items, vec![item(None, "PAN-OS", "11.1.6")]);
    assert_eq!(conv.report.pending_components, 0);
}

#[test]
fn existing_items_that_cannot_be_imported_skip_the_row() {
    let duplicate = existing(
        "FW-01",
        Some("NET-HW-002"),
        AssetStatus::Active,
        vec![
            item(Some("paloalto-panos"), "PAN-OS", "11.1.6"),
            item(None, "PAN-OS", "11.1.4"),
        ],
    );
    let gone = existing(
        "FW-02",
        Some("NET-HW-003"),
        AssetStatus::Active,
        vec![item(Some("no-such-product"), "no-such-product", "1.0")],
    );
    let rows = vec![
        isms_row(6, ("NET-", "HW-002"), "HW03", "網路服務", "PAN-OS"),
        isms_row(7, ("NET-", "HW-003"), "HW03", "網路服務", "PAN-OS"),
    ];
    let conv = run(rows, vec![duplicate, gone]);
    assert!(conv.assets.is_empty(), "{:?}", conv.assets);
    let reasons: Vec<String> = conv
        .report
        .skipped_rows
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert!(
        reasons[0].starts_with("第 6 列：FW-01 的未辨識產品「PAN-OS」已可辨識為 paloalto-panos"),
        "{reasons:?}"
    );
    assert!(
        reasons[1].starts_with("第 7 列：FW-02 的元件 no-such-product 已不在產品目錄中"),
        "{reasons:?}"
    );
}

#[test]
fn isms_id_of_a_retired_asset() {
    let nas = existing("NAS-02", Some("NET-HW-005"), AssetStatus::Retired, vec![]);
    let rows = vec![isms_row(8, ("NET-", "HW-005"), "HW05", "網路服務", "NAS")];
    let conv = run(rows, vec![nas]);
    assert!(conv.assets.is_empty());
    assert!(conv.systems.is_empty());
    assert_eq!(
        conv.report.skipped_rows[0].to_string(),
        "第 8 列：ISMS 編號 NET-HW-005 屬於已退役資產 NAS-02，須先恢復該資產"
    );
    assert!(conv.report.not_in_workbook.is_empty(), "退役資產不列入");
}

#[test]
fn asset_tag_equal_to_the_isms_id_is_reused() {
    let a = existing("net-hw-001", None, AssetStatus::Active, vec![]);
    let rows = vec![isms_row(
        5,
        ("NET-", "HW-001"),
        "HW02",
        "網路服務",
        "PAN-OS",
    )];
    let conv = run(rows, vec![a]);
    assert_eq!(conv.assets[0].record.asset_tag, "net-hw-001");
    assert_eq!(conv.assets[0].record.isms_id.as_deref(), Some("NET-HW-001"));
}

#[test]
fn assets_missing_and_rows_skipped() {
    let pve = existing("PVE-01", None, AssetStatus::Active, vec![]);
    let conv = run(
        vec![
            isms_row(5, ("NET-", "PE-001"), "PE01", "網路服務", "管理員"),
            isms_row(
                6,
                ("NET-", "HW-001"),
                "HW02",
                "網路服務",
                "Core: Aruba XYZ-100 *2、Edge: Aruba XYZ-100 *9",
            ),
            isms_row(7, ("NET-", "HW-002"), "HW02", "網路服務", ""),
        ],
        vec![pve],
    );
    let r = &conv.report;
    assert_eq!(r.skipped_by_major.get("PE"), Some(&1));
    assert_eq!(r.not_in_workbook, ["PVE-01"]);
    assert_eq!(r.merged[0].lines.len(), 2);
    assert_eq!(conv.assets[0].items.len(), 1, "合併成一個元件");
    assert_eq!(conv.assets.len(), 1, "描述空白的新資產略過");
    assert_eq!(
        r.skipped_rows[0].to_string(),
        "第 7 列：ISMS 編號 NET-HW-002 沒有資產描述，無法建立元件；請在 ISMS 清冊補上資產描述"
    );
}

#[test]
fn asset_tag_used_by_another_asset() {
    let x = existing(
        "NET-HW-001",
        Some("ABC-HW-009"),
        AssetStatus::Active,
        vec![],
    );
    let conv = run(
        vec![
            isms_row(5, ("NET-", "HW-001"), "HW02", "網路服務", "PAN-OS"),
            isms_row(6, ("ABC-", "HW-009"), "HW02", "網路服務", "PAN-OS"),
        ],
        vec![x],
    );
    assert_eq!(
        conv.report.skipped_rows[0].to_string(),
        "第 5 列：ISMS 編號 NET-HW-001 與既有資產 NET-HW-001（ISMS 編號 ABC-HW-009）的 asset_tag 相同；請先以 cvewatch asset rename 更改該資產的編號"
    );
    let tags: Vec<&str> = conv
        .assets
        .iter()
        .map(|a| a.record.asset_tag.as_str())
        .collect();
    assert_eq!(tags, ["NET-HW-001"], "第 6 列沿用既有資產");
    assert!(conv.assets[0].reused);
}

#[test]
fn two_rows_for_one_asset() {
    // 第 5 列以 ISMS 編號找到既有資產；第 6 列的 ISMS 編號等於同一資產的 asset_tag
    let a = existing(
        "NET-HW-002",
        Some("NET-HW-001"),
        AssetStatus::Active,
        vec![],
    );
    let conv = run(
        vec![
            isms_row(5, ("NET-", "HW-001"), "HW02", "網路服務", "PAN-OS"),
            isms_row(6, ("NET-", "HW-002"), "HW02", "網路服務", "PAN-OS"),
        ],
        vec![a],
    );
    assert_eq!(conv.assets.len(), 1);
    assert!(
        conv.report.skipped_rows[0]
            .to_string()
            .starts_with("第 6 列："),
        "{:?}",
        conv.report.skipped_rows
    );
}
