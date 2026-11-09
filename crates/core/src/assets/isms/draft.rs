//! 草稿組裝（17 design D1、D5）：ISMS 列 → 草稿資產與元件、`systems`、報表。
//!
//! [`build_draft`] 是純函式：資料庫內容由呼叫端以 [`DraftContext::load`] 先載入成不可變快照。

use std::collections::HashSet;

use rusqlite::Connection;

use super::describe::{describe, DescribedItem};
use super::report::{DraftReport, MergedProducts, UnresolvedLine};
use super::row::{convert_rows, system_codes, ConvertedRow, RowOutcome, SkipReason};
use super::workbook::IsmsBook;
use crate::assets::classes::AssetClasses;
use crate::assets::export::{inventory_items_by_asset, product_text, InventoryItem};
use crate::assets::model::{AssetRecord, AssetStatus};
use crate::assets::record::RecordContext;
use crate::assets::repo::{AssetFilter, AssetRepo};
use crate::assets::systems::{name_key, AssetSystem, SystemCode, SystemDirectory};
use crate::catalog::{normalize, Catalog};
use crate::error::CoreError;

/// 資料庫中的一個資產（含 retired）與其清冊元件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingAsset {
    pub record: AssetRecord,
    pub status: AssetStatus,
    /// 清冊元件（含版本）與未辨識產品
    pub items: Vec<InventoryItem>,
}

/// 轉換時參考的不可變快照。
#[derive(Debug, Clone)]
pub struct DraftContext<'a> {
    pub classes: &'a AssetClasses,
    pub catalog: &'a Catalog,
    pub systems: SystemDirectory,
    pub existing: Vec<ExistingAsset>,
}

impl<'a> DraftContext<'a> {
    /// 沒有資料庫可參考時（例如尚未初始化）。
    pub fn empty(classes: &'a AssetClasses, catalog: &'a Catalog) -> DraftContext<'a> {
        DraftContext {
            classes,
            catalog,
            systems: SystemDirectory::default(),
            existing: Vec::new(),
        }
    }

    /// 從資料庫載入資產說明與所有資產（含 retired）的清冊元件；只讀取。
    pub fn load(
        conn: &Connection,
        classes: &'a AssetClasses,
        catalog: &'a Catalog,
    ) -> Result<DraftContext<'a>, CoreError> {
        let assets = AssetRepo::new(conn).list(&AssetFilter {
            category: None,
            include_retired: true,
        })?;
        let mut items = inventory_items_by_asset(conn, catalog, None)?;
        let existing = assets
            .into_iter()
            .map(|a| ExistingAsset {
                items: items.remove(&a.id).unwrap_or_default(),
                record: a.record,
                status: a.status,
            })
            .collect();
        Ok(DraftContext {
            classes,
            catalog,
            systems: SystemDirectory::load(conn)?,
            existing,
        })
    }

    /// asset_tag 與 `tag` 相同（不分大小寫）的既有資產（含 retired）。
    fn tag_owner(&self, tag: &str) -> Option<&ExistingAsset> {
        self.existing
            .iter()
            .find(|a| a.record.asset_tag.eq_ignore_ascii_case(tag))
    }

    /// ISMS 編號（不分大小寫）所屬的既有資產；沒有時，比對沒有其他 ISMS 編號、
    /// asset_tag 與 ISMS 編號相同（不分大小寫）的資產（例如先前以 ISMS 編號為 asset_tag
    /// 匯入、但 ISMS 編號欄留空）。
    fn find(&self, isms_id: &str) -> Option<&ExistingAsset> {
        let same = |v: &str| v.eq_ignore_ascii_case(isms_id);
        self.existing
            .iter()
            .find(|a| a.record.isms_id.as_deref().is_some_and(same))
            .or_else(|| {
                self.existing.iter().find(|a| {
                    same(&a.record.asset_tag) && a.record.isms_id.as_deref().is_none_or(same)
                })
            })
    }
}

/// 「ISMS 編號 NET-HW-001 與既有資產 NET-HW-001（ISMS 編號 ABC-HW-009）的 asset_tag 相同…」。
fn tag_in_use(isms_id: &str, owner: &ExistingAsset) -> String {
    let mut notes: Vec<String> = Vec::new();
    if let Some(id) = owner.record.isms_id.as_deref() {
        notes.push(format!("ISMS 編號 {id}"));
    }
    if owner.status == AssetStatus::Retired {
        notes.push("已退役".to_string());
    }
    let notes = if notes.is_empty() {
        " ".to_string()
    } else {
        format!("（{}）", notes.join("、"))
    };
    format!(
        "ISMS 編號 {isms_id} 與既有資產 {}{notes}的 asset_tag 相同；請先以 cvewatch asset rename 更改該資產的編號",
        owner.record.asset_tag
    )
}

/// 草稿中的一個資產。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftAsset {
    /// ISMS 清冊的列號
    pub row: usize,
    pub record: AssetRecord,
    /// 曝露度寫出時留空（新資產）；沿用既有資產時寫出既有值
    pub blank_exposure: bool,
    /// 沿用資料庫中的既有資產
    pub reused: bool,
    /// 第一個寫在 assets 工作表，其餘寫在 components 工作表
    pub items: Vec<InventoryItem>,
}

/// 轉換結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub assets: Vec<DraftAsset>,
    pub systems: Vec<AssetSystem>,
    pub report: DraftReport,
}

/// 驗證用的資產說明目錄：資料庫＋這次草稿會列出的系統代碼（名稱不影響驗證）。
fn validation_systems(book: &IsmsBook, ctx: &DraftContext<'_>) -> SystemDirectory {
    let added: Vec<AssetSystem> = system_codes(&book.rows, ctx.classes)
        .into_iter()
        .filter(|code| ctx.systems.get(code).is_none())
        .map(|code| AssetSystem {
            name: code.to_string(),
            code,
        })
        .collect();
    ctx.systems.with(&added)
}

/// 描述比對結果加入元件：既有資產已有的產品（key 相同）或未辨識產品（正規化後原文相同）不重複加入。
fn add_described(
    isms_id: &str,
    description: &str,
    ctx: &DraftContext<'_>,
    items: &mut Vec<InventoryItem>,
    report: &mut DraftReport,
) {
    for item in describe(description, ctx.catalog) {
        match item {
            DescribedItem::Resolved { key, lines } => {
                if lines.len() > 1 {
                    report.merged.push(MergedProducts {
                        isms_id: isms_id.to_string(),
                        key: key.clone(),
                        lines,
                    });
                }
                if items.iter().all(|i| i.key.as_deref() != Some(&key)) {
                    items.push(InventoryItem {
                        product: product_text(ctx.catalog, &key),
                        key: Some(key),
                        version: String::new(),
                    });
                }
            }
            DescribedItem::Unresolved {
                text, suggestions, ..
            } => {
                let norm = normalize(&text);
                if items
                    .iter()
                    .all(|i| i.key.is_some() || normalize(&i.product) != norm)
                {
                    items.push(InventoryItem {
                        key: None,
                        product: text.clone(),
                        version: String::new(),
                    });
                }
                report.unresolved_lines.push(UnresolvedLine {
                    isms_id: isms_id.to_string(),
                    text,
                    suggestions,
                });
            }
        }
    }
}

/// 一個轉換成功的列 → 草稿資產；ISMS 編號屬於 retired 資產時略過。
fn draft_asset(
    c: &ConvertedRow,
    ctx: &DraftContext<'_>,
    report: &mut DraftReport,
) -> Result<DraftAsset, SkipReason> {
    let existing = ctx.find(&c.isms_id);
    if let Some(e) = existing.filter(|e| e.status == AssetStatus::Retired) {
        return Err(SkipReason {
            row: c.number,
            message: format!(
                "ISMS 編號 {} 屬於已退役資產 {}，須先恢復該資產",
                c.isms_id, e.record.asset_tag
            ),
        });
    }
    let (record, mut items) = match existing {
        // ISMS 欄位取自 ISMS 清冊；編號、名稱、曝露度、地點與備註沿用既有資產
        Some(e) => (
            AssetRecord {
                asset_tag: e.record.asset_tag.clone(),
                name: e.record.name.clone(),
                exposure: e.record.exposure,
                location: e.record.location.clone(),
                notes: e.record.notes.clone(),
                isms_id: e
                    .record
                    .isms_id
                    .clone()
                    .or_else(|| c.record.isms_id.clone()),
                ..c.record.clone()
            },
            e.items.clone(),
        ),
        None => {
            // 新資產以 ISMS 編號為 asset_tag：已被其他資產使用時，匯入會更新那個資產（覆寫欄位、
            // 移除草稿沒列出的元件），所以略過
            if let Some(other) = ctx.tag_owner(&c.isms_id) {
                return Err(SkipReason {
                    row: c.number,
                    message: tag_in_use(&c.isms_id, other),
                });
            }
            (c.record.clone(), Vec::new())
        }
    };
    add_described(&c.isms_id, &c.description, ctx, &mut items, report);
    if items.is_empty() {
        report.no_components.push(record.asset_tag.clone());
    }
    Ok(DraftAsset {
        row: c.number,
        record,
        blank_exposure: existing.is_none(),
        reused: existing.is_some(),
        items,
    })
}

/// `systems` 工作表：轉換結果中出現的系統代碼（依第一次出現的順序），名稱取第一個非空的
/// 資產說明；都空白時用資料庫中的名稱，再沒有時用代碼。
fn draft_systems(
    converted: &[&ConvertedRow],
    ctx: &DraftContext<'_>,
    report: &mut DraftReport,
) -> Vec<AssetSystem> {
    let mut codes: Vec<&SystemCode> = Vec::new();
    for c in converted {
        if !codes.contains(&&c.system) {
            codes.push(&c.system);
        }
    }
    let systems: Vec<AssetSystem> = codes
        .into_iter()
        .map(|code| {
            let rows: Vec<&&ConvertedRow> = converted
                .iter()
                .filter(|c| &c.system == code && !c.system_name.is_empty())
                .collect();
            if let Some(first) = rows.first() {
                for other in rows
                    .iter()
                    .filter(|r| name_key(&r.system_name) != name_key(&first.system_name))
                {
                    report.system_name_conflicts.push(format!(
                        "第 {} 列的資產說明「{}」與第 {} 列不同，{code} 使用「{}」",
                        other.number, other.system_name, first.number, first.system_name
                    ));
                }
            }
            let name = rows
                .first()
                .map(|r| r.system_name.clone())
                .unwrap_or_else(|| {
                    ctx.systems
                        .get(code)
                        .map_or_else(|| code.to_string(), |s| s.name.clone())
                });
            AssetSystem {
                code: code.clone(),
                name,
            }
        })
        .collect();
    // 與資料庫中其他資產說明衝突時，匯入會中止：先在報表提醒
    let merged = ctx.systems.with(&systems);
    for s in &systems {
        if let Some(e) = merged.conflict(&s.code, &s.name) {
            report
                .system_name_conflicts
                .push(format!("資產說明 {}：{}；匯入前須修改", s.code, e.message));
        }
    }
    systems
}

/// 兩個 ISMS 列對應到同一個 asset_tag（不分大小寫）時略過後者，避免匯入因重複而中止。
fn duplicate_tag(assets: &[DraftAsset], a: &DraftAsset) -> Option<SkipReason> {
    let first = assets
        .iter()
        .find(|x| x.record.asset_tag.eq_ignore_ascii_case(&a.record.asset_tag))?;
    Some(SkipReason {
        row: a.row,
        message: format!(
            "與第 {} 列對應到同一個資產 {}",
            first.row, first.record.asset_tag
        ),
    })
}

/// 轉換整份 ISMS 清冊（列序依 ISMS 清冊）。
pub fn build_draft(book: &IsmsBook, ctx: &DraftContext<'_>) -> Draft {
    let systems = validation_systems(book, ctx);
    let outcomes = convert_rows(
        &book.rows,
        &RecordContext {
            classes: ctx.classes,
            systems: &systems,
        },
    );
    let mut report = DraftReport::default();
    let mut assets = Vec::new();
    let mut converted: Vec<&ConvertedRow> = Vec::new();
    for outcome in &outcomes {
        match outcome {
            RowOutcome::OtherMajor(m) => {
                *report.skipped_by_major.entry(m.clone()).or_default() += 1
            }
            RowOutcome::Skipped(s) => report.skipped_rows.push(s.clone()),
            RowOutcome::Converted(c) => match draft_asset(c, ctx, &mut report) {
                Ok(a) => match duplicate_tag(&assets, &a) {
                    Some(s) => report.skipped_rows.push(s),
                    None => {
                        converted.push(c);
                        assets.push(a);
                    }
                },
                Err(s) => report.skipped_rows.push(s),
            },
        }
    }
    let systems = draft_systems(&converted, ctx, &mut report);
    report.converted_existing = assets.iter().filter(|a: &&DraftAsset| a.reused).count();
    report.converted_new = assets.len() - report.converted_existing;
    report.missing_versions = assets
        .iter()
        .flat_map(|a| &a.items)
        .filter(|i| i.version.trim().is_empty())
        .count();
    let tags: HashSet<&str> = assets.iter().map(|a| a.record.asset_tag.as_str()).collect();
    let mut missing: Vec<String> = ctx
        .existing
        .iter()
        .filter(|e| e.status == AssetStatus::Active && !tags.contains(e.record.asset_tag.as_str()))
        .map(|e| e.record.asset_tag.clone())
        .collect();
    missing.sort();
    report.not_in_workbook = missing;
    Draft {
        assets,
        systems,
        report,
    }
}

#[cfg(test)]
#[path = "draft_tests.rs"]
mod tests;
