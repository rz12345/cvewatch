//! 清冊匯出（design D5，`cvewatch asset export`）：使用中資產寫成與匯入相同格式的 xlsx
//! （`assets`、`components` 工作表、標準欄名），匯出後直接重匯為「新增 0、更新 0」。
//!
//! - 產品欄寫目前產品目錄的 `display_name`（重匯時以同一目錄解析回同一產品；不能時改寫 key），
//!   未辨識產品寫原文
//! - 版本寫 `version_raw`，重匯不產生版本變更
//! - 不含 retired 資產與主機套件元件（套件以 `asset import-packages` 管理）
//! - category 寫子類別代碼，C/I/A 寫實際值（13 design D4）
//! - `system` 寫資產說明代碼、`personal_data` 寫 Y／N；`systems` 工作表列出所有資產說明
//!   （含沒有使用中資產者，16 design D3）
//! - 識別欄位寫原值（未填為空白）；`ips` 工作表依資產編號、儲存順序每筆 IP 一列
//!   （20 design D13）

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection};

use super::columns::{SheetKind, StandardField};
use super::ips::AssetIpRepo;
use super::model::{Asset, AssetRecord};
use super::repo::{AssetFilter, AssetRepo};
use super::systems::SystemRepo;
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

/// 四個工作表的資料列（不含表頭）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryRows {
    pub assets: Vec<Vec<String>>,
    pub components: Vec<Vec<String>>,
    pub systems: Vec<Vec<String>>,
    /// `ips` 工作表（asset_tag、ip、purpose）
    pub ips: Vec<Vec<String>>,
    pub warnings: Vec<String>,
}

/// 是否含個人資料的匯出寫法（與 ISMS 清冊相同）。
pub fn personal_data_text(v: bool) -> &'static str {
    if v {
        "Y"
    } else {
        "N"
    }
}

/// 一個清冊元件（目錄產品或未辨識產品）在清冊檔中的寫法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryItem {
    /// 目錄產品的 key；未辨識產品為 None
    pub key: Option<String>,
    /// product 欄的寫法
    pub product: String,
    pub version: String,
}

/// 產品欄的寫法：目錄的顯示名稱（重匯時解析回同一產品）；否則寫 key（key 也是別名）。
pub fn product_text(catalog: &Catalog, key: &str) -> String {
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

/// 清冊元件（依建立順序）在前、未辨識產品在後，依資產 id 分組；`asset_id` 為 None 時一次讀出
/// 所有資產（兩個查詢，不因資產數量增加查詢次數）。
pub(crate) fn inventory_items_by_asset(
    conn: &Connection,
    catalog: &Catalog,
    asset_id: Option<i64>,
) -> Result<HashMap<i64, Vec<InventoryItem>>, CoreError> {
    let mut out: HashMap<i64, Vec<InventoryItem>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT c.asset_id, p.key, c.version_raw FROM component c
         JOIN catalog_product p ON p.id = c.product_id
         WHERE (?1 IS NULL OR c.asset_id = ?1) AND c.kind = 'product' AND c.source = 'excel'
           AND c.removed_at IS NULL
         ORDER BY c.asset_id, c.id",
    )?;
    let rows = stmt.query_map(params![asset_id], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get(2)?))
    })?;
    for row in rows {
        let (id, key, version) = row?;
        out.entry(id).or_default().push(InventoryItem {
            product: product_text(catalog, &key),
            key: Some(key),
            version,
        });
    }
    let mut stmt = conn.prepare(
        "SELECT asset_id, product_raw, version_raw FROM asset_unresolved
         WHERE (?1 IS NULL OR asset_id = ?1) ORDER BY asset_id, id",
    )?;
    let rows = stmt.query_map(params![asset_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            InventoryItem {
                key: None,
                product: r.get(1)?,
                version: r.get(2)?,
            },
        ))
    })?;
    for row in rows {
        let (id, item) = row?;
        out.entry(id).or_default().push(item);
    }
    Ok(out)
}

/// 一個資產的清冊元件（依建立順序）在前，未辨識產品在後。
fn inventory_items(
    conn: &Connection,
    catalog: &Catalog,
    asset_id: i64,
) -> Result<Vec<InventoryItem>, CoreError> {
    Ok(inventory_items_by_asset(conn, catalog, Some(asset_id))?
        .remove(&asset_id)
        .unwrap_or_default())
}

fn text(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

/// `assets` 工作表一列（欄位順序同 [`SheetKind::columns`]）；`first` 為第一個元件。
pub(crate) fn asset_row(r: &AssetRecord, first: Option<&InventoryItem>) -> Vec<String> {
    let (product, version) = first
        .map(|i| (i.product.clone(), i.version.clone()))
        .unwrap_or_default();
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
            StandardField::System => text(&r.system_code),
            StandardField::ResponsibleUnit => text(&r.responsible_unit),
            StandardField::Custodian => text(&r.custodian),
            StandardField::UserUnit => text(&r.user_unit),
            StandardField::Compliance => r.compliance.map(|d| d.to_string()).unwrap_or_default(),
            StandardField::PersonalData => personal_data_text(r.personal_data).to_string(),
            StandardField::Brand => text(&r.brand),
            StandardField::Model => text(&r.model),
            StandardField::SerialNumber => text(&r.serial_number),
            StandardField::PropertyNumber => text(&r.property_number),
            StandardField::Hostname => text(&r.hostname),
            StandardField::ManagementUrl => text(&r.management_url),
            StandardField::Location => text(&r.location),
            StandardField::Notes => text(&r.notes),
            // 停用欄位、舊欄名與其他工作表的欄位不在 columns() 中
            StandardField::Criticality
            | StandardField::Owner
            | StandardField::Code
            | StandardField::Ip
            | StandardField::Purpose => String::new(),
        })
        .collect()
}

/// `components` 工作表的列（asset_tag、product、version）。
pub(crate) fn component_rows<'a>(
    tag: &'a str,
    items: impl Iterator<Item = &'a InventoryItem> + 'a,
) -> impl Iterator<Item = Vec<String>> + 'a {
    items.map(move |i| vec![tag.to_string(), i.product.clone(), i.version.clone()])
}

/// 讀取使用中資產並轉成工作表資料列。
pub fn inventory_rows(conn: &Connection, catalog: &Catalog) -> Result<InventoryRows, CoreError> {
    let assets: Vec<Asset> = AssetRepo::new(conn).list(&AssetFilter::default())?;
    let mut ips = AssetIpRepo::new(conn).all_by_asset()?;
    let mut rows = InventoryRows::default();
    for asset in &assets {
        let r = &asset.record;
        let items = inventory_items(conn, catalog, asset.id)?;
        if items.is_empty() {
            rows.warnings.push(format!(
                "{} 沒有任何元件，產品與版本留空；重匯前需補上產品與版本",
                r.asset_tag
            ));
        }
        rows.assets.push(asset_row(r, items.first()));
        rows.components
            .extend(component_rows(&r.asset_tag, items.iter().skip(1)));
        rows.ips.extend(
            ips.remove(&asset.id)
                .unwrap_or_default()
                .into_iter()
                .map(|ip| {
                    vec![
                        r.asset_tag.clone(),
                        ip.address.to_string(),
                        ip.purpose.unwrap_or_default(),
                    ]
                }),
        );
    }
    rows.systems = SystemRepo::new(conn)
        .list()?
        .into_iter()
        .map(|s| vec![s.code.as_str().to_string(), s.name])
        .collect();
    Ok(rows)
}

/// `assets`、`components`、`systems`、`ips` 四個工作表（標準欄名表頭）。
pub(crate) fn inventory_sheets(rows: InventoryRows) -> Vec<SheetData<'static>> {
    vec![
        SheetData {
            name: SheetKind::Assets.sheet_name(),
            headers: headers(SheetKind::Assets),
            rows: rows.assets,
        },
        SheetData {
            name: SheetKind::Components.sheet_name(),
            headers: headers(SheetKind::Components),
            rows: rows.components,
        },
        SheetData {
            name: SheetKind::Systems.sheet_name(),
            headers: headers(SheetKind::Systems),
            rows: rows.systems,
        },
        SheetData {
            name: SheetKind::Ips.sheet_name(),
            headers: headers(SheetKind::Ips),
            rows: rows.ips,
        },
    ]
}

/// 匯出使用中資產到 `path`（.xlsx）；目的檔已存在時拒絕且不覆蓋。
/// `catalog` 須與重匯時使用的產品目錄相同（內建種子 + catalog.d）。
pub fn export_inventory(
    conn: &Connection,
    catalog: &Catalog,
    path: &Path,
) -> Result<ExportSummary, CoreError> {
    let rows = inventory_rows(conn, catalog)?;
    let assets = rows.assets.len();
    let warnings = rows.warnings.clone();
    write_sheets(path, &inventory_sheets(rows))?;
    tracing::info!(path = %path.display(), assets, "已匯出清冊");
    Ok(ExportSummary { assets, warnings })
}
