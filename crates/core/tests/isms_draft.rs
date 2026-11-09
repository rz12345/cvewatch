//! 17 任務 3.3～3.4：產生 ISMS 草稿（四個工作表、資料庫不變、目的檔已存在）與往返匯入
//! （未補版本中止、補上版本後成功、既有 ISMS 編號的資產為「更新 1」）。

mod common;

use std::path::{Path, PathBuf};

use calamine::{open_workbook_auto, Reader};
use chrono::Utc;
use common::matching::{world_with, World};
use cvewatch_core::assets::columns::ColumnMap;
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::isms::{generate_draft, DraftContext, DraftReport};
use cvewatch_core::assets::plan::ImportPlan;
use cvewatch_core::assets::sheet::{read_inventory, Sheet};
use cvewatch_core::assets::template::{write_sheets, SheetData};
use cvewatch_core::assets::validate::parse_inventory;
use cvewatch_core::assets::{AssetRepo, Cia};
use cvewatch_core::CoreError;

const FILLED_VERSION: &str = "1.0";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn isms_world() -> World {
    let aliases = std::fs::read_to_string(fixture("isms_catalog.d/isms-test.toml")).unwrap();
    world_with(Some(&aliases))
}

fn draft(w: &World, target: &Path) -> Result<DraftReport, CoreError> {
    let ctx = DraftContext::load(&w.conn, &w.classes, &w.catalog).unwrap();
    generate_draft(&fixture("isms_workbook.xlsx"), target, &ctx)
}

fn import(w: &mut World, path: &Path, dry_run: bool) -> Result<ImportPlan, CoreError> {
    let book = read_inventory(path, None)?;
    let inventory = parse_inventory(&book, &ColumnMap::default(), &w.classes)?;
    import_inventory(
        &mut w.conn,
        &w.catalog,
        &inventory,
        ImportOptions {
            dry_run,
            prune: false,
        },
        Utc::now(),
    )
}

fn count(w: &World, table: &str) -> i64 {
    w.conn
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// 把工作表轉回寫出格式；空白的 version 補上 [`FILLED_VERSION`]。
fn filled(sheet: &Sheet, name: &'static str) -> SheetData<'static> {
    let version = sheet.headers.iter().position(|h| h == "version");
    let rows = sheet
        .rows
        .iter()
        .map(|r| {
            let mut cells = r.cells.clone();
            if let Some(i) = version {
                cells.resize(cells.len().max(i + 1), String::new());
                if cells[i].is_empty() {
                    cells[i] = FILLED_VERSION.into();
                }
            }
            cells
        })
        .collect();
    SheetData {
        name,
        headers: sheet.headers.clone(),
        rows,
    }
}

/// 使用者在草稿補上所有版本後另存。
fn fill_versions(draft: &Path, out: &Path) {
    let book = read_inventory(draft, None).unwrap();
    let mut sheets = vec![filled(&book.assets, "assets")];
    sheets.extend(book.components.as_ref().map(|s| filled(s, "components")));
    sheets.extend(book.systems.as_ref().map(|s| filled(s, "systems")));
    write_sheets(out, &sheets).unwrap();
}

#[test]
fn isms_generating_a_draft() {
    let w = isms_world();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("draft.xlsx");
    let report = draft(&w, &target).unwrap();
    let mut book = open_workbook_auto(&target).unwrap();
    assert_eq!(
        book.sheet_names(),
        ["assets", "components", "systems", "isms_report"]
    );
    let report_sheet = book.worksheet_range("isms_report").unwrap();
    assert!(report_sheet.height() > 1, "isms_report 有明細");
    assert_eq!(count(&w, "asset"), 0, "資料庫不變");
    assert_eq!(count(&w, "asset_system"), 0, "資料庫不變");
    assert_eq!(
        report.summary(),
        "轉換 6 個資產（沿用既有 0）、略過 5 列、待補版本 7 個元件"
    );
    assert_eq!(
        report.major_line().as_deref(),
        Some("略過 PE 1、IF 2、SE 1")
    );
    assert_eq!(
        report.skipped_rows[0].to_string(),
        "第 15 列：ISMS 編號「WEB-SE-003」的大類 SE 與資產類別 SW01 不符"
    );
    assert_eq!(report.merged.len(), 1, "NET-HW-004 的兩行 Aruba XYZ-100");
    let texts: Vec<&str> = report
        .unresolved_lines
        .iter()
        .map(|u| u.text.as_str())
        .collect();
    assert_eq!(texts, ["Zyxel ABCD", "Synology XYZ-200", "官網系統"]);
    assert_eq!(
        report.system_name_conflicts.len(),
        1,
        "第 9 列的資產說明不同"
    );

    let inv = read_inventory(&target, None).unwrap();
    let tags: Vec<&str> = inv
        .assets
        .rows
        .iter()
        .map(|r| r.cells[0].as_str())
        .collect();
    assert_eq!(
        tags,
        [
            "NET-HW-001",
            "NET-HW-002",
            "NET-HW-003",
            "NET-HW-004",
            "NET-SW-001",
            "WEB-SW-002"
        ]
    );
    let exposure = inv
        .assets
        .headers
        .iter()
        .position(|h| h == "exposure")
        .unwrap();
    assert!(inv
        .assets
        .rows
        .iter()
        .all(|r| r.cells.get(exposure).is_none_or(|c| c.is_empty())));
    let components = inv.components.unwrap();
    assert_eq!(components.rows[0].cells[..2], ["NET-HW-001", "Zyxel ABCD"]);
    let systems = inv.systems.unwrap();
    let names: Vec<(&str, &str)> = systems
        .rows
        .iter()
        .map(|r| (r.cells[0].as_str(), r.cells[1].as_str()))
        .collect();
    assert_eq!(names, [("NET", "網路服務"), ("WEB", "官網服務")]);
}

#[test]
fn isms_draft_target_exists() {
    let w = isms_world();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("draft.xlsx");
    std::fs::write(&target, b"keep me").unwrap();
    let e = draft(&w, &target).unwrap_err().to_string();
    assert!(e.contains("目的檔已存在"), "{e}");
    assert_eq!(std::fs::read(&target).unwrap(), b"keep me");
    let ctx = DraftContext::load(&w.conn, &w.classes, &w.catalog).unwrap();
    let e = generate_draft(
        &fixture("isms_not_workbook.xlsx"),
        &dir.path().join("x.xlsx"),
        &ctx,
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("找不到 ISMS 資訊資產清冊的表頭"), "{e}");
    assert!(!dir.path().join("x.xlsx").exists(), "不產生草稿");
}

#[test]
fn isms_importing_a_draft_before_filling_versions() {
    let mut w = isms_world();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("draft.xlsx");
    draft(&w, &target).unwrap();
    let CoreError::ImportRejected(errors) = import(&mut w, &target, false).unwrap_err() else {
        panic!("應中止匯入");
    };
    assert!(
        errors.iter().any(|e| e == "第 2 列：缺少 version"),
        "{errors:?}"
    );
    assert!(
        errors.iter().all(|e| e.contains("缺少 version")),
        "{errors:?}"
    );
    assert_eq!(count(&w, "asset"), 0, "資料庫不變");
}

#[test]
fn isms_importing_a_completed_draft() {
    let mut w = isms_world();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("draft.xlsx");
    draft(&w, &target).unwrap();
    let done = dir.path().join("done.xlsx");
    fill_versions(&target, &done);
    let plan = import(&mut w, &done, false).unwrap();
    assert_eq!(plan.added.len(), 6);
    let a = AssetRepo::new(&w.conn)
        .find_by_tag("NET-HW-003")
        .unwrap()
        .unwrap();
    assert_eq!(a.record.isms_id.as_deref(), Some("NET-HW-003"));
    assert_eq!(a.record.system_code.as_deref(), Some("NET"));
    assert_eq!(a.record.custodian.as_deref(), Some("王小明"));
    assert_eq!(a.record.cia, Cia::new(2, 3, 4));
    assert_eq!(a.record.name, "Synology XYZ-200");
    let web = AssetRepo::new(&w.conn)
        .find_by_tag("WEB-SW-002")
        .unwrap()
        .unwrap();
    assert!(web.record.personal_data);
    assert_eq!(web.record.compliance, Some(3));

    // ISMS 清冊改版後重新產生草稿：全部沿用既有資產，重匯沒有新增
    let again = dir.path().join("again.xlsx");
    let report = draft(&w, &again).unwrap();
    assert_eq!(report.converted_existing, 6);
    assert_eq!(report.missing_versions, 0, "既有元件保留版本");
    let plan = import(&mut w, &again, true).unwrap();
    assert!(plan.added.is_empty() && plan.updated.is_empty(), "{plan:?}");
}

#[test]
fn isms_existing_isms_id_is_an_update() {
    let mut w = isms_world();
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("existing.xlsx");
    let headers = [
        "asset_tag",
        "name",
        "category",
        "product",
        "version",
        "exposure",
        "isms_id",
    ];
    write_sheets(
        &existing,
        &[
            SheetData {
                name: "assets",
                headers: headers.iter().map(|h| h.to_string()).collect(),
                rows: vec![[
                    "FW-01",
                    "fw01",
                    "HW03",
                    "PAN-OS",
                    "11.1.6",
                    "internet",
                    "NET-HW-002",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect()],
            },
            SheetData {
                name: "systems",
                headers: vec!["code".into(), "name".into()],
                rows: vec![vec!["NET".into(), "網路服務".into()]],
            },
        ],
    )
    .unwrap();
    import(&mut w, &existing, false).unwrap();
    let target = dir.path().join("draft.xlsx");
    let report = draft(&w, &target).unwrap();
    assert_eq!(report.converted_existing, 1);
    let done = dir.path().join("done.xlsx");
    fill_versions(&target, &done);
    let plan = import(&mut w, &done, true).unwrap();
    assert_eq!(plan.added.len(), 5);
    assert_eq!(plan.updated.len(), 1, "{plan:?}");
    assert_eq!(plan.updated[0].asset_tag, "FW-01");
    assert_eq!(plan.version_change_count(), 0, "PAN-OS 版本沿用 11.1.6");
}
