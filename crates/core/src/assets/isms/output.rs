//! 寫出草稿（17 design D5、D7）：重用清冊匯出的列組裝與工作表寫出，另加 `isms_report`
//! 工作表（匯入時不讀）。不寫資料庫；目的檔已存在時在讀取 ISMS 清冊之前就拒絕。

use std::path::Path;

use super::draft::{build_draft, Draft, DraftContext};
use super::report::{DraftReport, REPORT_HEADERS, REPORT_SHEET};
use super::workbook_read::read_isms_workbook;
use crate::assets::columns::{SheetKind, StandardField};
use crate::assets::export::{asset_row, component_rows, inventory_sheets, InventoryRows};
use crate::assets::template::{ensure_absent, write_sheets, SheetData};
use crate::error::CoreError;

/// `assets` 工作表中曝露度欄的位置。
fn exposure_column() -> usize {
    SheetKind::Assets
        .columns()
        .iter()
        .position(|f| *f == StandardField::Exposure)
        .expect("assets 工作表有 exposure 欄")
}

/// 草稿的三個清冊工作表資料列。
fn draft_rows(draft: &Draft) -> InventoryRows {
    let exposure = exposure_column();
    let mut rows = InventoryRows::default();
    for a in &draft.assets {
        let mut row = asset_row(&a.record, a.items.first());
        if a.blank_exposure {
            row[exposure] = String::new();
        }
        rows.assets.push(row);
        rows.components
            .extend(component_rows(&a.record.asset_tag, a.items.iter().skip(1)));
    }
    rows.systems = draft
        .systems
        .iter()
        .map(|s| vec![s.code.as_str().to_string(), s.name.clone()])
        .collect();
    rows
}

/// 寫出草稿 xlsx（`assets`、`components`、`systems`、`isms_report`）；目的檔已存在時不覆蓋。
pub fn write_draft(path: &Path, draft: &Draft) -> Result<(), CoreError> {
    let mut sheets = inventory_sheets(draft_rows(draft));
    sheets.push(SheetData {
        name: REPORT_SHEET,
        headers: REPORT_HEADERS.iter().map(|h| h.to_string()).collect(),
        rows: draft.report.sheet_rows(),
    });
    write_sheets(path, &sheets)
}

/// 讀取 ISMS 清冊 `source`、轉換並寫出草稿到 `target`（CLI 與畫面共用）。
pub fn generate_draft(
    source: &Path,
    target: &Path,
    ctx: &DraftContext<'_>,
) -> Result<DraftReport, CoreError> {
    ensure_absent(target)?;
    let book = read_isms_workbook(source)?;
    let draft = build_draft(&book, ctx);
    write_draft(target, &draft)?;
    tracing::info!(
        source = %source.display(),
        target = %target.display(),
        converted = draft.report.converted(),
        reused = draft.report.converted_existing,
        skipped = draft.report.skipped(),
        "已產生 ISMS 草稿"
    );
    Ok(draft.report)
}
