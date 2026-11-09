//! ISMS 轉換（17 design D1、D5；18 design D4）：ISMS 列 → 匯入用的資產與元件、資產說明、報表。
//!
//! [`build_conversion`] 是純函式：資料庫內容由呼叫端以 [`ConversionContext::load`] 先載入成
//! 不可變快照。結果由 [`super::import::to_inventory`] 轉成清冊，交給一般清冊匯入的交易寫入。

use std::collections::HashSet;

use rusqlite::Connection;

use super::describe::{describe, DescribedItem};
use super::report::{IsmsReport, MergedProducts, UnresolvedLine};
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
pub struct ConversionContext<'a> {
    pub classes: &'a AssetClasses,
    pub catalog: &'a Catalog,
    pub systems: SystemDirectory,
    pub existing: Vec<ExistingAsset>,
}

impl<'a> ConversionContext<'a> {
    /// 沒有既有資產與資產說明時（單元測試用的起點）。
    pub fn empty(classes: &'a AssetClasses, catalog: &'a Catalog) -> ConversionContext<'a> {
        ConversionContext {
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
    ) -> Result<ConversionContext<'a>, CoreError> {
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
        Ok(ConversionContext {
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

/// 轉換結果中的一個資產。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertedAsset {
    /// ISMS 清冊的列號
    pub row: usize,
    /// 新資產的曝露度為預設值 internal；沿用既有資產時為既有值
    pub record: AssetRecord,
    /// 沿用資料庫中的既有資產
    pub reused: bool,
    /// 清冊元件與未辨識產品（既有的含版本；描述新加入的為版本待補）
    pub items: Vec<InventoryItem>,
}

/// 轉換結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversion {
    pub assets: Vec<ConvertedAsset>,
    pub systems: Vec<AssetSystem>,
    pub report: IsmsReport,
}

/// 驗證用的資產說明目錄：資料庫＋這次轉換會列出的系統代碼（名稱不影響驗證）。
fn validation_systems(book: &IsmsBook, ctx: &ConversionContext<'_>) -> SystemDirectory {
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

/// 一列描述比對出的結果（被略過的列不併入報表）。
#[derive(Default)]
struct Described {
    items: Vec<InventoryItem>,
    merged: Vec<MergedProducts>,
    unresolved_lines: Vec<UnresolvedLine>,
    /// 新加入的版本待補目錄產品元件數
    pending: usize,
}

/// 匯入時這個項目會成為哪個目錄產品：目錄產品元件為其 key；未辨識產品的原文若已可由目錄辨識
/// （例如之後在 catalog.d 加了別名），規劃時也會成為該產品。
fn resolved_key(item: &InventoryItem, catalog: &Catalog) -> Option<String> {
    item.key
        .clone()
        .or_else(|| catalog.resolve(&item.product).map(|p| p.key.clone()))
}

/// 描述比對結果加入版本待補的元件：既有資產已有的產品（匯入時會成為同一 key，含已可辨識的
/// 未辨識產品）或未辨識產品（正規化後原文相同）不重複加入。
fn described(
    isms_id: &str,
    description: &str,
    ctx: &ConversionContext<'_>,
    existing: Vec<InventoryItem>,
) -> Described {
    let mut out = Described {
        items: existing,
        ..Described::default()
    };
    for item in describe(description, ctx.catalog) {
        match item {
            DescribedItem::Resolved { key, lines } => {
                if lines.len() > 1 {
                    out.merged.push(MergedProducts {
                        isms_id: isms_id.to_string(),
                        key: key.clone(),
                        lines,
                    });
                }
                let present = out
                    .items
                    .iter()
                    .any(|i| resolved_key(i, ctx.catalog).as_deref() == Some(key.as_str()));
                if !present {
                    out.pending += 1;
                    out.items.push(InventoryItem {
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
                if out
                    .items
                    .iter()
                    .all(|i| i.key.is_some() || normalize(&i.product) != norm)
                {
                    out.items.push(InventoryItem {
                        key: None,
                        product: text.clone(),
                        version: String::new(),
                    });
                }
                out.unresolved_lines.push(UnresolvedLine {
                    isms_id: isms_id.to_string(),
                    text,
                    suggestions,
                });
            }
        }
    }
    out
}

/// 一個轉換成功的列 → 匯入的資產；ISMS 編號屬於 retired 資產、新資產沒有可建立的元件時略過。
fn convert_asset(
    c: &ConvertedRow,
    ctx: &ConversionContext<'_>,
    report: &mut IsmsReport,
) -> Result<ConvertedAsset, SkipReason> {
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
    let (record, items) = match existing {
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
            // 移除這次沒列出的元件），所以略過
            if let Some(other) = ctx.tag_owner(&c.isms_id) {
                return Err(SkipReason {
                    row: c.number,
                    message: tag_in_use(&c.isms_id, other),
                });
            }
            (c.record.clone(), Vec::new())
        }
    };
    if let Some(message) = existing.and_then(|e| unsafe_existing_items(e, ctx.catalog)) {
        return Err(SkipReason {
            row: c.number,
            message,
        });
    }
    let found = described(&c.isms_id, &c.description, ctx, items);
    // 新資產至少要有一個元件；描述空白時略過，不讓整批因此中止（18 design D4）
    if found.items.is_empty() && existing.is_none() {
        return Err(SkipReason {
            row: c.number,
            message: format!(
                "ISMS 編號 {} 沒有資產描述，無法建立元件；請在 ISMS 清冊補上資產描述",
                c.isms_id
            ),
        });
    }
    report.merged.extend(found.merged);
    report.unresolved_lines.extend(found.unresolved_lines);
    report.pending_components += found.pending;
    Ok(ConvertedAsset {
        row: c.number,
        record,
        reused: existing.is_some(),
        items: found.items,
    })
}

/// 既有資產的項目無法原樣匯入時的原因（18 code review）：ISMS 匯入不得移除元件、不得因重複
/// 產品中止整批，所以略過該列，請使用者先在畫面處理。
/// - 元件的產品已不在目錄中：匯入時會成為未辨識產品，原元件會被移除
/// - 未辨識產品已可辨識為既有元件的產品：匯入時兩者重複
fn unsafe_existing_items(e: &ExistingAsset, catalog: &Catalog) -> Option<String> {
    let tag = &e.record.asset_tag;
    if let Some(key) = e
        .items
        .iter()
        .filter_map(|i| i.key.as_deref())
        .find(|k| catalog.get(k).is_none())
    {
        return Some(format!(
            "{tag} 的元件 {key} 已不在產品目錄中，匯入會移除它；請先在 catalog.d 恢復該產品或在畫面移除該元件"
        ));
    }
    let mut seen: Vec<String> = Vec::new();
    for item in &e.items {
        let Some(key) = resolved_key(item, catalog) else {
            continue;
        };
        if seen.contains(&key) && item.key.is_none() {
            return Some(format!(
                "{tag} 的未辨識產品「{}」已可辨識為 {key}，與既有元件重複；請先在畫面刪除或處理該未辨識產品",
                item.product
            ));
        }
        seen.push(key);
    }
    None
}

/// 資產說明：轉換結果中出現的系統代碼（依第一次出現的順序），名稱取第一個非空的資產說明；
/// 都空白時用資料庫中的名稱，再沒有時用代碼。與其他資產說明的名稱衝突由匯入的規劃檢查並
/// 中止整批（18 design D4）。
fn conversion_systems(
    converted: &[&ConvertedRow],
    ctx: &ConversionContext<'_>,
    report: &mut IsmsReport,
) -> Vec<AssetSystem> {
    let mut codes: Vec<&SystemCode> = Vec::new();
    for c in converted {
        if !codes.contains(&&c.system) {
            codes.push(&c.system);
        }
    }
    codes
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
        .collect()
}

/// 兩個 ISMS 列對應到同一個 asset_tag（不分大小寫）時略過後者，避免匯入因重複而中止。
fn duplicate_tag(assets: &[ConvertedAsset], a: &ConvertedAsset) -> Option<SkipReason> {
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
pub fn build_conversion(book: &IsmsBook, ctx: &ConversionContext<'_>) -> Conversion {
    let systems = validation_systems(book, ctx);
    let outcomes = convert_rows(
        &book.rows,
        &RecordContext {
            classes: ctx.classes,
            systems: &systems,
        },
    );
    let mut report = IsmsReport::default();
    let mut assets: Vec<ConvertedAsset> = Vec::new();
    let mut converted: Vec<&ConvertedRow> = Vec::new();
    for outcome in &outcomes {
        match outcome {
            RowOutcome::OtherMajor(m) => {
                *report.skipped_by_major.entry(m.clone()).or_default() += 1
            }
            RowOutcome::Skipped(s) => report.skipped_rows.push(s.clone()),
            RowOutcome::Converted(c) => {
                // 重複的列在併入報表前就略過，報表不列出它的描述比對結果
                let mut found = IsmsReport::default();
                match convert_asset(c, ctx, &mut found) {
                    Ok(a) => match duplicate_tag(&assets, &a) {
                        Some(s) => report.skipped_rows.push(s),
                        None => {
                            report.absorb(found);
                            converted.push(c);
                            assets.push(a);
                        }
                    },
                    Err(s) => report.skipped_rows.push(s),
                }
            }
        }
    }
    let systems = conversion_systems(&converted, ctx, &mut report);
    report.converted_existing = assets.iter().filter(|a| a.reused).count();
    report.converted_new = assets.len() - report.converted_existing;
    let tags: HashSet<&str> = assets.iter().map(|a| a.record.asset_tag.as_str()).collect();
    let mut missing: Vec<String> = ctx
        .existing
        .iter()
        .filter(|e| e.status == AssetStatus::Active && !tags.contains(e.record.asset_tag.as_str()))
        .map(|e| e.record.asset_tag.clone())
        .collect();
    missing.sort();
    report.not_in_workbook = missing;
    Conversion {
        assets,
        systems,
        report,
    }
}

#[cfg(test)]
#[path = "conversion_tests.rs"]
mod tests;
