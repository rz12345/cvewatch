//! 清冊匯出（design D5，`cvewatch asset export`）：使用中資產寫成與匯入相同格式的 xlsx
//! （`assets`、`components` 工作表、標準欄名），匯出後直接重匯為「新增 0、更新 0」。
//!
//! - 產品欄寫目前產品目錄的 `display_name`（重匯時以同一目錄解析回同一產品；不能時改寫 key），
//!   未辨識產品寫原文
//! - 版本寫 `version_raw`，重匯不產生版本變更
//! - 不含 retired 資產與主機套件元件（套件以 `asset import-packages` 管理）
//! - category 寫子類別代碼，C/I/A 寫實際值（13 design D4）

use std::path::Path;

use rusqlite::{params, Connection};

use super::columns::{SheetKind, StandardField};
use super::model::{Asset, AssetRecord};
use super::repo::{AssetFilter, AssetRepo};
use super::template::{headers, write_sheets, SheetData};
use crate::catalog::Catalog;
use crate::error::CoreError;

/// 匯出的結果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportSummary {
    pub assets: usize,
    /// 重匯前需要處理的問題（不中止匯出）
    pub warnings: Vec<String>,
}

/// 兩個工作表的資料列（不含表頭）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryRows {
    pub assets: Vec<Vec<String>>,
    pub components: Vec<Vec<String>>,
    pub warnings: Vec<String>,
}

/// 產品欄的寫法：目錄的顯示名稱（重匯時解析回同一產品）；否則寫 key（key 也是別名）。
fn product_text(catalog: &Catalog, key: &str) -> String {
    match catalog.get(key) {
        Some(p)
            if catalog
                .resolve(&p.display_name)
                .is_some_and(|r| r.key == key) =>
        {
            p.display_name.clone()
        }
        _ => key.to_string(),
    }
}

/// 一個資產的產品寫法與版本：清冊元件（依建立順序）在前，未辨識產品在後。
fn items(
    conn: &Connection,
    catalog: &Catalog,
    asset_id: i64,
) -> Result<Vec<(String, String)>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT p.key, c.version_raw FROM component c
         JOIN catalog_product p ON p.id = c.product_id
         WHERE c.asset_id = ?1 AND c.kind = 'product' AND c.source = 'excel'
           AND c.removed_at IS NULL
         ORDER BY c.id",
    )?;
    let mut out: Vec<(String, String)> = stmt
        .query_map(params![asset_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get(1)?))
        })?
        .map(|row| row.map(|(key, version)| (product_text(catalog, &key), version)))
        .collect::<Result<_, _>>()?;
    let mut stmt = conn.prepare(
        "SELECT product_raw, version_raw FROM asset_unresolved WHERE asset_id = ?1 ORDER BY id",
    )?;
    let unresolved = stmt.query_map(params![asset_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    for row in unresolved {
        out.push(row?);
    }
    Ok(out)
}

fn text(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

/// `assets` 工作表一列（欄位順序同 [`SheetKind::columns`]）。
fn asset_row(r: &AssetRecord, first: Option<&(String, String)>) -> Vec<String> {
    let (product, version) = first.cloned().unwrap_or_default();
    SheetKind::Assets
        .columns()
        .iter()
        .map(|field| match field {
            StandardField::AssetTag => r.asset_tag.clone(),
            StandardField::Name => r.name.clone(),
            StandardField::Category => r.category.as_str().to_string(),
            StandardField::Product => product.clone(),
            StandardField::Version => version.clone(),
            StandardField::Exposure => r.exposure.as_str().to_string(),
            // 寫出實際值（不留空）：重匯不因範本值改變而變動
            StandardField::Confidentiality => r.cia.confidentiality.to_string(),
            StandardField::Integrity => r.cia.integrity.to_string(),
            StandardField::Availability => r.cia.availability.to_string(),
            StandardField::IsmsId => text(&r.isms_id),
            // 停用欄位不在 columns() 中
            StandardField::Criticality => String::new(),
            StandardField::Location => text(&r.location),
            StandardField::Owner => text(&r.owner),
            StandardField::Notes => text(&r.notes),
        })
        .collect()
}

/// 讀取使用中資產並轉成工作表資料列。
pub fn inventory_rows(conn: &Connection, catalog: &Catalog) -> Result<InventoryRows, CoreError> {
    let assets: Vec<Asset> = AssetRepo::new(conn).list(&AssetFilter::default())?;
    let mut rows = InventoryRows::default();
    for asset in &assets {
        let r = &asset.record;
        let items = items(conn, catalog, asset.id)?;
        if items.is_empty() {
            rows.warnings.push(format!(
                "{} 沒有任何元件，產品與版本留空；重匯前需補上產品與版本",
                r.asset_tag
            ));
        }
        rows.assets.push(asset_row(r, items.first()));
        for (product, version) in items.iter().skip(1) {
            rows.components
                .push(vec![r.asset_tag.clone(), product.clone(), version.clone()]);
        }
    }
    Ok(rows)
}

/// 匯出使用中資產到 `path`（.xlsx）；目的檔已存在時拒絕且不覆蓋。
/// `catalog` 須與重匯時使用的產品目錄相同（內建種子 + catalog.d）。
pub fn export_inventory(
    conn: &Connection,
    catalog: &Catalog,
    path: &Path,
) -> Result<ExportSummary, CoreError> {
    let rows = inventory_rows(conn, catalog)?;
    write_sheets(
        path,
        &[
            SheetData {
                name: SheetKind::Assets.sheet_name(),
                headers: headers(SheetKind::Assets),
                rows: rows.assets.clone(),
            },
            SheetData {
                name: SheetKind::Components.sheet_name(),
                headers: headers(SheetKind::Components),
                rows: rows.components,
            },
        ],
    )?;
    tracing::info!(path = %path.display(), assets = rows.assets.len(), "已匯出清冊");
    Ok(ExportSummary {
        assets: rows.assets.len(),
        warnings: rows.warnings,
    })
}
