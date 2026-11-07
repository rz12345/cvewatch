//! 將載入的目錄同步到資料庫（`catalog_*` 表），讓元件以 `product_id` 參照。

use std::collections::HashMap;

use rusqlite::{params, Connection};

use super::Catalog;
use crate::error::CoreError;

/// 同步目錄並回傳 `key → product_id`。
///
/// 目錄中已移除、但仍被元件參照的產品保留在 `catalog_product`（不再有別名，不會被辨識）；
/// 未被參照者刪除。呼叫端應在交易中執行。
pub fn sync_catalog(
    conn: &Connection,
    catalog: &Catalog,
) -> Result<HashMap<String, i64>, CoreError> {
    for table in [
        "catalog_alias",
        "catalog_cpe",
        "catalog_cna",
        "catalog_csaf",
    ] {
        conn.execute(&format!("DELETE FROM {table}"), [])?;
    }
    let mut ids = HashMap::new();
    for product in catalog.products() {
        conn.execute(
            "INSERT INTO catalog_product (key, display_name, vendor_display, version_scheme,
                                          advisory_source)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (key) DO UPDATE SET display_name = excluded.display_name,
                 vendor_display = excluded.vendor_display,
                 version_scheme = excluded.version_scheme,
                 advisory_source = excluded.advisory_source",
            params![
                product.key,
                product.display_name,
                product.vendor_display,
                product.version_scheme,
                product.advisory_source
            ],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM catalog_product WHERE key = ?1",
            params![product.key],
            |r| r.get(0),
        )?;
        insert_identifiers(conn, id, product)?;
        ids.insert(product.key.clone(), id);
    }
    remove_stale_products(conn, &ids)?;
    Ok(ids)
}

fn insert_identifiers(
    conn: &Connection,
    id: i64,
    product: &super::Product,
) -> Result<(), CoreError> {
    let mut seen = std::collections::HashSet::new();
    for alias in &product.aliases {
        let norm = super::normalize(alias);
        if !norm.is_empty() && seen.insert(norm.clone()) {
            conn.execute(
                "INSERT INTO catalog_alias (product_id, alias_norm) VALUES (?1, ?2)",
                params![id, norm],
            )?;
        }
    }
    for cpe in &product.cpe {
        conn.execute(
            "INSERT INTO catalog_cpe (product_id, cpe_part, cpe_vendor, cpe_product)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, cpe.part, cpe.vendor, cpe.product],
        )?;
    }
    for (table, pairs) in [
        ("catalog_cna", &product.cna),
        ("catalog_csaf", &product.csaf),
    ] {
        let prefix = table.trim_start_matches("catalog_");
        let sql = format!(
            "INSERT INTO {table} (product_id, {prefix}_vendor_norm, {prefix}_product_norm)
             VALUES (?1, ?2, ?3)"
        );
        for pair in pairs {
            conn.execute(&sql, params![id, pair.vendor, pair.product])?;
        }
    }
    Ok(())
}

/// 刪除目錄中已移除、且沒有任何元件參照的產品。
///
/// 軟刪除的元件（`removed_at`）仍算參照：它可能在下次匯入時恢復並沿用原命中，
/// 外鍵也不允許刪除被參照的產品，因此這些產品列刻意保留（不再有別名，不會被辨識）。
fn remove_stale_products(
    conn: &Connection,
    current: &HashMap<String, i64>,
) -> Result<(), CoreError> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.key FROM catalog_product p
         WHERE NOT EXISTS (SELECT 1 FROM component c WHERE c.product_id = p.id)",
    )?;
    let unreferenced: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (id, key) in unreferenced {
        if !current.contains_key(&key) {
            conn.execute("DELETE FROM catalog_product WHERE id = ?1", params![id])?;
        }
    }
    Ok(())
}
