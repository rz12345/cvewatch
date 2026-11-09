//! 版本待補的計數（18 design D7）：Dashboard、`match`／`sync` 摘要與每日摘要共用。

use std::collections::BTreeSet;

use rusqlite::Connection;

use crate::error::CoreError;

/// active 資產中版本待補的目錄產品元件數與資產數（未辨識產品另有「未辨識」標示，不計入）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PendingCounts {
    pub components: i64,
    pub assets: i64,
}

impl PendingCounts {
    /// 有版本待補的元件。
    pub fn any(&self) -> bool {
        self.components > 0
    }

    /// 「版本待補 3 個元件（2 個資產），未納入比對」；沒有待補時為 None。
    pub fn summary(&self) -> Option<String> {
        self.any().then(|| {
            format!(
                "版本待補 {} 個元件（{} 個資產），未納入比對",
                self.components, self.assets
            )
        })
    }
}

/// 匯入預覽與完成摘要有版本待補的元件時，最後一行的提醒（CLI 與 GUI 共用，GUI 以整行比對判斷
/// 預覽是否有版本待補的元件）。
pub const PENDING_REMINDER: &str = "版本待補的元件未納入比對，請在畫面補上版本";

/// SQL 條件：版本待補的目錄產品元件（與 [`super::model::is_pending_version`] 相同判斷）。
pub const PENDING_COMPONENT_SQL: &str = "c.kind = 'product' AND trim(c.version_raw) = ''";

/// active 資產中有版本待補元件的目錄產品 key。
pub fn pending_product_keys(conn: &Connection) -> Result<BTreeSet<String>, CoreError> {
    let sql = format!(
        "SELECT DISTINCT p.key FROM active_component c JOIN catalog_product p ON p.id = c.product_id
         WHERE {PENDING_COMPONENT_SQL}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let keys = stmt.query_map([], |r| r.get(0))?;
    Ok(keys.collect::<Result<_, _>>()?)
}

/// 計算 active 資產的版本待補元件數與資產數。
pub fn pending_counts(conn: &Connection) -> Result<PendingCounts, CoreError> {
    let sql = format!(
        "SELECT count(*), count(DISTINCT c.asset_id) FROM active_component c
         WHERE {PENDING_COMPONENT_SQL}"
    );
    Ok(conn.query_row(&sql, [], |r| {
        Ok(PendingCounts {
            components: r.get(0)?,
            assets: r.get(1)?,
        })
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_reads_counts_and_is_none_without_pending() {
        let counts = PendingCounts {
            components: 3,
            assets: 2,
        };
        assert_eq!(
            counts.summary().as_deref(),
            Some("版本待補 3 個元件（2 個資產），未納入比對")
        );
        assert_eq!(PendingCounts::default().summary(), None);
    }
}
