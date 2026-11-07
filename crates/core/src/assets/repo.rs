//! 資產與元件的 repository。所有方法都接受 `&Connection`（交易可透過 deref 傳入）。

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};

use super::model::{
    Asset, AssetRecord, AssetStatus, Category, Component, ComponentKind, ComponentSource, Exposure,
    HistoryEntry, UnresolvedEntry,
};
use crate::error::CoreError;

const ASSET_COLUMNS: &str = "id, asset_tag, name, category, location, owner, exposure, \
                             criticality, notes, status, first_seen, last_seen, retired_at";

const COMPONENT_SELECT: &str = "SELECT c.id, c.asset_id, c.kind, c.product_id, p.key, c.purl, \
     c.purl_full, c.version_raw, c.version_norm, c.source, c.updated_at \
     FROM component c LEFT JOIN catalog_product p ON p.id = c.product_id";

fn ts(now: DateTime<Utc>) -> String {
    now.to_rfc3339()
}

fn invalid_value(column: usize, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        format!("資料庫中的值不合法：{value}").into(),
    )
}

fn parse_col<T>(row: &Row<'_>, idx: usize, parse: fn(&str) -> Option<T>) -> rusqlite::Result<T> {
    let text: String = row.get(idx)?;
    parse(&text).ok_or_else(|| invalid_value(idx, &text))
}

fn asset_from_row(row: &Row<'_>) -> rusqlite::Result<Asset> {
    Ok(Asset {
        id: row.get(0)?,
        record: AssetRecord {
            asset_tag: row.get(1)?,
            name: row.get(2)?,
            category: parse_col(row, 3, Category::parse)?,
            location: row.get(4)?,
            owner: row.get(5)?,
            exposure: parse_col(row, 6, Exposure::parse)?,
            criticality: row.get(7)?,
            notes: row.get(8)?,
        },
        status: parse_col(row, 9, AssetStatus::parse)?,
        first_seen: row.get(10)?,
        last_seen: row.get(11)?,
        retired_at: row.get(12)?,
    })
}

fn component_from_row(row: &Row<'_>) -> rusqlite::Result<Component> {
    let kind_text: String = row.get(2)?;
    let kind = match kind_text.as_str() {
        "product" => ComponentKind::Product {
            product_id: row.get(3)?,
            product_key: row.get(4)?,
        },
        "package" => ComponentKind::Package {
            purl: row.get(5)?,
            purl_full: row.get(6)?,
        },
        other => return Err(invalid_value(2, other)),
    };
    Ok(Component {
        id: row.get(0)?,
        asset_id: row.get(1)?,
        kind,
        version_raw: row.get(7)?,
        version_norm: row.get(8)?,
        source: parse_col(row, 9, ComponentSource::parse)?,
        updated_at: row.get(10)?,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetFilter {
    pub category: Option<Category>,
    pub include_retired: bool,
}

pub struct AssetRepo<'c> {
    conn: &'c Connection,
}

impl<'c> AssetRepo<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        AssetRepo { conn }
    }

    pub fn find_by_tag(&self, tag: &str) -> Result<Option<Asset>, CoreError> {
        let sql = format!("SELECT {ASSET_COLUMNS} FROM asset WHERE asset_tag = ?1");
        Ok(self
            .conn
            .query_row(&sql, params![tag], asset_from_row)
            .optional()?)
    }

    pub fn list(&self, filter: &AssetFilter) -> Result<Vec<Asset>, CoreError> {
        let sql = format!(
            "SELECT {ASSET_COLUMNS} FROM asset
             WHERE (?1 IS NULL OR category = ?1) AND (?2 OR status = 'active')
             ORDER BY asset_tag"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let category = filter.category.map(|c| c.as_str());
        let rows = stmt.query_map(params![category, filter.include_retired], asset_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn insert(&self, record: &AssetRecord, now: DateTime<Utc>) -> Result<i64, CoreError> {
        self.conn.execute(
            "INSERT INTO asset (asset_tag, name, category, location, owner, exposure, criticality,
                                notes, status, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', ?9, ?9)",
            params![
                record.asset_tag,
                record.name,
                record.category.as_str(),
                record.location,
                record.owner,
                record.exposure.as_str(),
                record.criticality,
                record.notes,
                ts(now)
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 以清冊內容覆寫欄位並重新啟用（若曾退役），同時更新 `last_seen`。
    pub fn update_record(
        &self,
        id: i64,
        record: &AssetRecord,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        self.conn.execute(
            "UPDATE asset SET name = ?2, category = ?3, location = ?4, owner = ?5, exposure = ?6,
                    criticality = ?7, notes = ?8, status = 'active', retired_at = NULL,
                    last_seen = ?9
             WHERE id = ?1",
            params![
                id,
                record.name,
                record.category.as_str(),
                record.location,
                record.owner,
                record.exposure.as_str(),
                record.criticality,
                record.notes,
                ts(now)
            ],
        )?;
        Ok(())
    }

    /// 退役期間 OSV 不查詢其套件；重新啟用時把元件標為已更新，等下次 OSV 同步後再比對。
    pub fn mark_components_updated(
        &self,
        asset_id: i64,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        self.conn.execute(
            "UPDATE component SET updated_at = ?2 WHERE asset_id = ?1",
            params![asset_id, ts(now)],
        )?;
        Ok(())
    }

    pub fn touch(&self, id: i64, now: DateTime<Utc>) -> Result<(), CoreError> {
        self.conn.execute(
            "UPDATE asset SET last_seen = ?2 WHERE id = ?1",
            params![id, ts(now)],
        )?;
        Ok(())
    }

    pub fn retire(&self, id: i64, now: DateTime<Utc>) -> Result<(), CoreError> {
        self.conn.execute(
            "UPDATE asset SET status = 'retired', retired_at = ?2 WHERE id = ?1 AND status = 'active'",
            params![id, ts(now)],
        )?;
        Ok(())
    }

    /// 以新清單取代該資產的未辨識產品紀錄。
    pub fn replace_unresolved(
        &self,
        asset_id: i64,
        entries: &[(String, String)],
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        self.conn.execute(
            "DELETE FROM asset_unresolved WHERE asset_id = ?1",
            params![asset_id],
        )?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO asset_unresolved (asset_id, product_raw, version_raw, seen_at)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (product, version) in entries {
            stmt.execute(params![asset_id, product, version, ts(now)])?;
        }
        Ok(())
    }

    /// 所有 active 資產的未辨識產品。
    pub fn unresolved(&self) -> Result<Vec<UnresolvedEntry>, CoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT a.asset_tag, u.product_raw, u.version_raw
             FROM asset_unresolved u JOIN asset a ON a.id = u.asset_id
             WHERE a.status = 'active'
             ORDER BY u.product_raw, a.asset_tag",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(UnresolvedEntry {
                asset_tag: r.get(0)?,
                product_raw: r.get(1)?,
                version_raw: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

/// upsert 的結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpsertOutcome {
    Inserted,
    VersionChanged { previous: String },
    Unchanged,
}

pub struct ComponentRepo<'c> {
    conn: &'c Connection,
}

impl<'c> ComponentRepo<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        ComponentRepo { conn }
    }

    pub fn for_asset(&self, asset_id: i64) -> Result<Vec<Component>, CoreError> {
        let sql = format!(
            "{COMPONENT_SELECT} WHERE c.asset_id = ?1 AND c.removed_at IS NULL
             ORDER BY c.kind DESC, p.key, c.purl"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![asset_id], component_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn history(&self, component_id: i64) -> Result<Vec<HistoryEntry>, CoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT version_raw, changed_at FROM component_history
             WHERE component_id = ?1 ORDER BY changed_at DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![component_id], |r| {
            Ok(HistoryEntry {
                version_raw: r.get(0)?,
                changed_at: r.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// 產品元件 upsert；版本變更時把舊版本寫入 history。
    pub fn upsert_product(
        &self,
        asset_id: i64,
        product_id: i64,
        version_raw: &str,
        version_norm: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<UpsertOutcome, CoreError> {
        let existing: Option<(i64, String, bool)> = self
            .conn
            .query_row(
                "SELECT id, version_raw, removed_at IS NOT NULL FROM component
                 WHERE asset_id = ?1 AND product_id = ?2",
                params![asset_id, product_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        match existing {
            None => {
                self.conn.execute(
                    "INSERT INTO component (asset_id, kind, product_id, version_raw, version_norm,
                                            source, updated_at)
                     VALUES (?1, 'product', ?2, ?3, ?4, 'excel', ?5)",
                    params![asset_id, product_id, version_raw, version_norm, ts(now)],
                )?;
                Ok(UpsertOutcome::Inserted)
            }
            Some((id, previous, removed)) => {
                self.restore(id, removed, now)?;
                let outcome = self.change_version(id, previous, version_raw, version_norm, now)?;
                Ok(restored_outcome(removed, outcome))
            }
        }
    }

    /// 套件元件 upsert（`purl` 為不含版本的識別）。
    pub fn upsert_package(
        &self,
        asset_id: i64,
        package: &PackageUpsert<'_>,
        now: DateTime<Utc>,
    ) -> Result<UpsertOutcome, CoreError> {
        let existing: Option<(i64, String, bool)> = self
            .conn
            .query_row(
                "SELECT id, version_raw, removed_at IS NOT NULL FROM component
                 WHERE asset_id = ?1 AND purl = ?2",
                params![asset_id, package.purl],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        match existing {
            None => {
                self.conn.execute(
                    "INSERT INTO component (asset_id, kind, purl, purl_full, version_raw, source,
                                            updated_at)
                     VALUES (?1, 'package', ?2, ?3, ?4, ?5, ?6)",
                    params![
                        asset_id,
                        package.purl,
                        package.purl_full,
                        package.version_raw,
                        package.source.as_str(),
                        ts(now)
                    ],
                )?;
                Ok(UpsertOutcome::Inserted)
            }
            Some((id, previous, removed)) => {
                self.restore(id, removed, now)?;
                self.conn.execute(
                    "UPDATE component SET purl_full = ?2, source = ?3 WHERE id = ?1",
                    params![id, package.purl_full, package.source.as_str()],
                )?;
                let outcome = self.change_version(id, previous, package.version_raw, None, now)?;
                Ok(restored_outcome(removed, outcome))
            }
        }
    }

    fn change_version(
        &self,
        id: i64,
        previous: String,
        version_raw: &str,
        version_norm: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<UpsertOutcome, CoreError> {
        if previous == version_raw {
            return Ok(UpsertOutcome::Unchanged);
        }
        self.conn.execute(
            "INSERT INTO component_history (component_id, version_raw, changed_at)
             VALUES (?1, ?2, ?3)",
            params![id, previous, ts(now)],
        )?;
        self.conn.execute(
            "UPDATE component SET version_raw = ?2, version_norm = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, version_raw, version_norm, ts(now)],
        )?;
        Ok(UpsertOutcome::VersionChanged { previous })
    }

    /// 軟刪除：不再參與比對，命中與處理紀錄保留（見 0004_findings）。
    pub fn remove(&self, component_id: i64, now: DateTime<Utc>) -> Result<(), CoreError> {
        self.conn.execute(
            "UPDATE component SET removed_at = ?2 WHERE id = ?1 AND removed_at IS NULL",
            params![component_id, ts(now)],
        )?;
        Ok(())
    }

    /// 已移除的元件再次出現：清除 removed_at，`updated_at` 更新為現在（OSV 結果需重查）。
    fn restore(
        &self,
        component_id: i64,
        removed: bool,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        if removed {
            self.conn.execute(
                "UPDATE component SET removed_at = NULL, updated_at = ?2 WHERE id = ?1",
                params![component_id, ts(now)],
            )?;
        }
        Ok(())
    }
}

/// 恢復的元件版本未變時視為新增（對使用者而言它重新出現）。
fn restored_outcome(removed: bool, outcome: UpsertOutcome) -> UpsertOutcome {
    match (removed, outcome) {
        (true, UpsertOutcome::Unchanged) => UpsertOutcome::Inserted,
        (_, other) => other,
    }
}

/// 套件元件寫入參數。
#[derive(Debug, Clone, Copy)]
pub struct PackageUpsert<'a> {
    pub purl: &'a str,
    pub purl_full: &'a str,
    pub version_raw: &'a str,
    pub source: ComponentSource,
}
