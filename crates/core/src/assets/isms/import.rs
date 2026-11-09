//! 從 ISMS 清冊匯入（18 design D4）：轉換結果 → [`Inventory`] → 一般清冊匯入的交易
//! （[`import_inventory`]，`prune` 一律關閉）。版本歷史、比對待補與寫入規則都與一般匯入相同。

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use super::conversion::{build_conversion, Conversion, ConversionContext};
use super::report::IsmsReport;
use super::workbook_read::read_isms_workbook;
use crate::assets::classes::AssetClasses;
use crate::assets::columns::{SheetKind, StandardField};
use crate::assets::import::{import_inventory, ImportOptions};
use crate::assets::model::AssetRecord;
use crate::assets::plan::ImportPlan;
use crate::assets::validate::{
    IncomingAsset, IncomingComponent, IncomingSystem, Inventory, RowRef,
};
use crate::catalog::Catalog;
use crate::error::CoreError;

/// ISMS 匯入的結果：ISMS 報表與一般匯入的差異。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsmsImport {
    pub report: IsmsReport,
    pub plan: ImportPlan,
}

/// ISMS 清冊的列位置（錯誤訊息為「第 N 列」）。
fn row_ref(number: usize) -> RowRef {
    RowRef {
        sheet: SheetKind::Assets,
        number,
    }
}

/// 轉換結果組成與讀檔相同的清冊：資產說明在規劃階段由 `system_raw`（系統代碼）解析，
/// 所以 `record.system_code` 清空（同 [`IncomingAsset`] 的約定）。ISMS 清冊提供所有可保留欄位，
/// 不保留資料庫中的值。
pub fn to_inventory(conversion: &Conversion) -> Inventory {
    let assets = conversion
        .assets
        .iter()
        .map(|a| {
            let row = row_ref(a.row);
            IncomingAsset {
                row,
                system_raw: a
                    .record
                    .system_code
                    .as_ref()
                    .map(|c| c.to_string())
                    .unwrap_or_default(),
                record: AssetRecord {
                    system_code: None,
                    ..a.record.clone()
                },
                components: a
                    .items
                    .iter()
                    .map(|i| IncomingComponent {
                        row,
                        product_raw: i.product.clone(),
                        version_raw: i.version.clone(),
                    })
                    .collect(),
            }
        })
        .collect();
    let systems = conversion
        .systems
        .iter()
        .map(|s| IncomingSystem {
            // 名稱衝突的錯誤指向第一個使用該代碼的列
            row: row_ref(
                conversion
                    .assets
                    .iter()
                    .find(|a| a.record.system_code.as_deref() == Some(s.code.as_str()))
                    .map_or(0, |a| a.row),
            ),
            system: s.clone(),
        })
        .collect();
    // ISMS 清冊沒有生命週期欄位：保留資料庫中的日期與 EoL 覆寫（eol 預設為 Untouched）
    Inventory {
        assets,
        systems,
        kept_fields: [
            StandardField::WarrantyEnd,
            StandardField::SupportEnd,
            StandardField::LicenseEnd,
        ]
        .into(),
        ..Inventory::default()
    }
}

/// 讀取 ISMS 清冊、轉換並匯入；`dry_run` 時資料庫不變。不在 ISMS 清冊中的資產不退役。
pub fn import_isms(
    conn: &mut Connection,
    classes: &AssetClasses,
    catalog: &Catalog,
    path: &Path,
    dry_run: bool,
    now: DateTime<Utc>,
) -> Result<IsmsImport, CoreError> {
    let book = read_isms_workbook(path)?;
    // 轉換在匯入交易之外讀資料庫：CLI 是單一程序，GUI 同時只會有一個匯入（design D4）
    let ctx = ConversionContext::load(conn, classes, catalog)?;
    let conversion = build_conversion(&book, &ctx);
    let inventory = to_inventory(&conversion);
    let plan = import_inventory(
        conn,
        catalog,
        &inventory,
        ImportOptions {
            dry_run,
            prune: false,
        },
        now,
    )?;
    tracing::info!(
        dry_run,
        converted = conversion.report.converted(),
        skipped = conversion.report.skipped(),
        pending = conversion.report.pending_components,
        "ISMS 清冊匯入"
    );
    Ok(IsmsImport {
        report: conversion.report,
        plan,
    })
}
