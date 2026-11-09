//! ISMS 匯出的列資料（23 design D4、D7）：有 ISMS 編號的 active 資產 → ISMS v2.1 的 16 欄，
//! 依 ISMS 編號（不分大小寫）排序；沒有 ISMS 編號的 active 資產列入略過清單。
//!
//! 列的組成（[`build_export`]）是純函式；[`isms_export_rows`] 只讀資料庫，[`export_isms`] 寫檔。

use std::collections::HashMap;
use std::path::Path;

use chrono::NaiveDate;
use rusqlite::Connection;

use super::description::{compose_description, DescriptionIdentity};
use super::export_write::write_workbook;
use crate::assets::classes::AssetClasses;
use crate::assets::export::{inventory_items_in, personal_data_text, InventoryItem, ItemScope};
use crate::assets::model::AssetRecord;
use crate::assets::record_admin::isms_system_code;
use crate::assets::repo::{AssetFilter, AssetRepo};
use crate::assets::systems::SystemDirectory;
use crate::assets::template::already_exists;
use crate::catalog::Catalog;
use crate::error::CoreError;

/// 匯出檔的一列（ISMS v2.1 的 16 欄，順序同 [`super::workbook::ISMS_V21_HEADERS`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsmsExportRow {
    /// 第一欄資產編號：系統代碼加 `-`（`NET-`）
    pub system_id: String,
    /// 第二欄資產編號：其餘部分（`HW-001`），保留原寫法
    pub serial: String,
    /// 資產類別：子類別代碼
    pub category: String,
    /// 資產說明：資產說明名稱
    pub system_name: String,
    /// 資產名稱：子類別名稱（子類別已不在設定中時空白）
    pub class_name: String,
    pub description: String,
    pub responsible_unit: String,
    pub custodian: String,
    pub user_unit: String,
    pub confidentiality: u8,
    pub integrity: u8,
    pub availability: u8,
    /// 資產價值 MAX(C, I, A)
    pub value: u8,
    /// 法規遵循性；None 為未評定（寫 `-`）
    pub compliance: Option<u8>,
    /// 資通系統防護等級；None 為未評定（寫 `-`）
    pub protection_level: Option<u8>,
    /// `Y`／`N`
    pub personal_data: String,
}

/// 列資料與報表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IsmsExport {
    pub rows: Vec<IsmsExportRow>,
    /// 沒有 ISMS 編號的 active 資產編號（依資產編號排序）
    pub skipped: Vec<String>,
    /// 子類別未定義、讀回不一致、沒有元件行
    pub warnings: Vec<String>,
}

/// 匯出結果（CLI 與 GUI 的完成訊息）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IsmsExportSummary {
    pub exported: usize,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
}

impl IsmsExportSummary {
    /// 「略過 2 個沒有 ISMS 編號的資產：SRV-01、SRV-02」（全部列出）；沒有略過時為 None。
    /// CLI 與 GUI 的完成訊息共用。
    pub fn skipped_line(&self) -> Option<String> {
        (!self.skipped.is_empty()).then(|| {
            format!(
                "略過 {} 個沒有 ISMS 編號的資產：{}",
                self.skipped.len(),
                self.skipped.join("、")
            )
        })
    }
}

/// 組列時參考的設定與資料。
#[derive(Debug, Clone, Copy)]
pub struct ExportContext<'a> {
    pub classes: &'a AssetClasses,
    pub catalog: &'a Catalog,
    pub systems: &'a SystemDirectory,
}

/// ISMS 編號拆成兩欄：第一個 `-` 之前加 `-`、之後（沒有 `-` 時第一欄空白）。
pub fn split_isms_id(isms_id: &str) -> (String, String) {
    match isms_id.split_once('-') {
        Some((system, rest)) => (format!("{system}-"), rest.to_string()),
        None => (String::new(), isms_id.to_string()),
    }
}

fn text(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

/// Excel 一個儲存格最多的字數（超過時 xlsx 無法寫出）。
pub const EXCEL_CELL_MAX_CHARS: usize = 32_767;

/// 超過 Excel 儲存格上限的文字截斷並警告，不讓單一資產使整個匯出失敗。
fn fit_cell(tag: &str, column: &str, value: String, warnings: &mut Vec<String>) -> String {
    if value.chars().count() <= EXCEL_CELL_MAX_CHARS {
        return value;
    }
    warnings.push(format!(
        "{tag} 的{column}超過 Excel 儲存格上限 {EXCEL_CELL_MAX_CHARS} 字，已截斷；以 ISMS 匯入讀回時會與原本不同"
    ));
    value.chars().take(EXCEL_CELL_MAX_CHARS).collect()
}

/// 資產說明名稱：資產的資產說明代碼，沒有時由 ISMS 編號的系統代碼帶出。
fn system_name(r: &AssetRecord, isms_id: &str, systems: &SystemDirectory) -> Option<String> {
    r.system_code
        .as_deref()
        .and_then(crate::assets::systems::SystemCode::parse)
        .or_else(|| isms_system_code(isms_id))
        .and_then(|code| systems.get(&code))
        .map(|s| s.name.clone())
}

/// 一個資產的匯出列與警告。
fn export_row(
    r: &AssetRecord,
    isms_id: &str,
    items: &[InventoryItem],
    ctx: ExportContext<'_>,
) -> (IsmsExportRow, Vec<String>) {
    let tag = &r.asset_tag;
    let composed = compose_description(
        DescriptionIdentity {
            asset_tag: tag,
            brand: r.brand.as_deref(),
            model: r.model.as_deref(),
        },
        items,
        ctx.catalog,
    );
    let mut warnings = Vec::new();
    let class_name = match ctx.classes.get(&r.category) {
        Some(class) => class.name.clone(),
        None => {
            warnings.push(format!(
                "{tag} 的資產類別 {} 已不在設定中，資產名稱留空；以 ISMS 匯入讀回時會被略過",
                r.category
            ));
            String::new()
        }
    };
    let system_name = system_name(r, isms_id, ctx.systems).unwrap_or_else(|| {
        warnings.push(format!(
            "{tag} 的 ISMS 編號「{isms_id}」沒有對應的資產說明，資產說明留空"
        ));
        String::new()
    });
    warnings.extend(composed.warnings);
    let (system_id, serial) = split_isms_id(isms_id);
    let mut fit = |column: &str, value: String| fit_cell(tag, column, value, &mut warnings);
    let row = IsmsExportRow {
        system_id,
        serial,
        category: r.category.as_str().to_string(),
        system_name: fit("資產說明", system_name),
        class_name,
        description: fit("資產描述", composed.text),
        responsible_unit: fit("權責單位", text(&r.responsible_unit)),
        custodian: fit("保管人", text(&r.custodian)),
        user_unit: fit("使用單位", text(&r.user_unit)),
        confidentiality: r.cia.confidentiality,
        integrity: r.cia.integrity,
        availability: r.cia.availability,
        value: r.cia.value(),
        compliance: r.compliance,
        protection_level: r.protection_level(),
        personal_data: personal_data_text(r.personal_data).to_string(),
    };
    (row, warnings)
}

/// 一個 active 資產與其清冊元件（依建立順序）。
pub struct ExportAsset<'a> {
    pub record: &'a AssetRecord,
    pub items: &'a [InventoryItem],
}

/// active 資產 → 匯出列（純函式）：沒有 ISMS 編號的列入略過；列依 ISMS 編號不分大小寫排序，
/// 警告依列的順序。
pub fn build_export(assets: &[ExportAsset<'_>], ctx: ExportContext<'_>) -> IsmsExport {
    let mut with_id: Vec<(&str, &ExportAsset<'_>)> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for asset in assets {
        match asset.record.isms_id.as_deref() {
            Some(id) if !id.trim().is_empty() => with_id.push((id, asset)),
            _ => skipped.push(asset.record.asset_tag.clone()),
        }
    }
    with_id.sort_by_cached_key(|(id, _)| id.to_lowercase());
    skipped.sort_by_key(|tag| tag.to_lowercase());
    let mut export = IsmsExport {
        skipped,
        ..IsmsExport::default()
    };
    for (id, asset) in with_id {
        let (row, warnings) = export_row(asset.record, id, asset.items, ctx);
        export.rows.push(row);
        export.warnings.extend(warnings);
    }
    export
}

/// 讀取 active 資產與清冊元件並組成匯出列；只讀取。
pub fn isms_export_rows(
    conn: &Connection,
    classes: &AssetClasses,
    catalog: &Catalog,
) -> Result<IsmsExport, CoreError> {
    let assets = AssetRepo::new(conn).list(&AssetFilter::default())?;
    // 只讀有 ISMS 編號的 active 資產的元件：略過的資產不需要描述
    let mut items: HashMap<i64, Vec<InventoryItem>> =
        inventory_items_in(conn, catalog, ItemScope::ActiveWithIsmsId)?;
    let systems = SystemDirectory::load(conn)?;
    let owned: Vec<(&AssetRecord, Vec<InventoryItem>)> = assets
        .iter()
        .map(|a| (&a.record, items.remove(&a.id).unwrap_or_default()))
        .collect();
    let export_assets: Vec<ExportAsset<'_>> = owned
        .iter()
        .map(|(record, items)| ExportAsset { record, items })
        .collect();
    Ok(build_export(
        &export_assets,
        ExportContext {
            classes,
            catalog,
            systems: &systems,
        },
    ))
}

/// 匯出有 ISMS 編號的 active 資產到 `path`（.xlsx，design D3）；目的檔已存在時拒絕且不覆蓋。
/// `catalog` 須與讀回時使用的產品目錄相同（內建種子 + catalog.d）。
pub fn export_isms(
    conn: &Connection,
    classes: &AssetClasses,
    catalog: &Catalog,
    path: &Path,
    exported_on: NaiveDate,
) -> Result<IsmsExportSummary, CoreError> {
    let export = isms_export_rows(conn, classes, catalog)?;
    let bytes = write_workbook(&export.rows, exported_on)
        .map_err(|e| CoreError::Invalid(format!("無法寫出 {}：{e}", path.display())))?;
    crate::files::write_new(path, &bytes, already_exists)?;
    let summary = IsmsExportSummary {
        exported: export.rows.len(),
        skipped: export.skipped,
        warnings: export.warnings,
    };
    tracing::info!(
        path = %path.display(),
        exported = summary.exported,
        skipped = summary.skipped.len(),
        warnings = summary.warnings.len(),
        "已匯出 ISMS 清冊"
    );
    Ok(summary)
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
