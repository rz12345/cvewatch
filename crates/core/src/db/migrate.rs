//! 版本化 schema migration：內嵌 SQL、單一交易套用、資料庫版本過新時拒絕。

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::error::CoreError;

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

/// 程式內建的 migration，版本號必須遞增。
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "0001_init",
        sql: include_str!("migrations/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "0002_assets",
        sql: include_str!("migrations/0002_assets.sql"),
    },
    Migration {
        version: 3,
        name: "0003_cve",
        sql: include_str!("migrations/0003_cve.sql"),
    },
    Migration {
        version: 4,
        name: "0004_findings",
        sql: include_str!("migrations/0004_findings.sql"),
    },
    Migration {
        version: 5,
        name: "0005_notify",
        sql: include_str!("migrations/0005_notify.sql"),
    },
    Migration {
        version: 6,
        name: "0006_vex",
        sql: include_str!("migrations/0006_vex.sql"),
    },
    Migration {
        version: 7,
        name: "0007_isms_class",
        sql: include_str!("migrations/0007_isms_class.sql"),
    },
    Migration {
        version: 8,
        name: "0008_isms_admin",
        sql: include_str!("migrations/0008_isms_admin.sql"),
    },
];

pub fn latest_version() -> u32 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

/// 目前 schema 版本；尚未建立 `schema_version` 表時為 0。
pub fn current_version(conn: &Connection) -> Result<u32, CoreError> {
    let has_table: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if has_table.is_none() {
        return Ok(0);
    }
    let version: Option<u32> =
        conn.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))?;
    Ok(version.unwrap_or(0))
}

/// 套用內建 migration，回傳套用後的版本。
pub fn migrate(conn: &mut Connection) -> Result<u32, CoreError> {
    migrate_with(conn, MIGRATIONS)
}

/// 以指定的 migration 清單升級（測試可注入）。
pub fn migrate_with(conn: &mut Connection, migrations: &[Migration]) -> Result<u32, CoreError> {
    let supported = migrations.last().map_or(0, |m| m.version);
    let before = current_version(conn)?;
    ensure_not_too_new(before, supported)?;
    if before == supported {
        return Ok(before);
    }

    // 以 IMMEDIATE 取得寫入鎖後重新讀取版本，避免兩個程序同時升級。
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = current_version(&tx)?;
    ensure_not_too_new(current, supported)?;
    let now = chrono::Utc::now().to_rfc3339();
    for m in migrations.iter().filter(|m| m.version > current) {
        apply_one(&tx, m, &now)?;
    }
    tx.commit()?;
    tracing::info!(from = current, to = supported, "資料庫 schema 已升級");
    Ok(supported)
}

fn ensure_not_too_new(found: u32, supported: u32) -> Result<(), CoreError> {
    if found > supported {
        return Err(CoreError::DbTooNew { found, supported });
    }
    Ok(())
}

fn apply_one(tx: &rusqlite::Transaction<'_>, m: &Migration, now: &str) -> Result<(), CoreError> {
    let wrap = |source| CoreError::Migration {
        version: m.version,
        name: m.name,
        source,
    };
    let result = tx.execute_batch(m.sql).and_then(|()| {
        tx.execute(
            "INSERT INTO schema_version (version, name, applied_at) VALUES (?1, ?2, ?3)",
            params![m.version, m.name, now],
        )
        .map(|_| ())
    });
    result.map_err(|e| {
        tracing::error!(
            schema_version = m.version,
            migration = m.name,
            "migration 失敗"
        );
        wrap(e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_versions_are_strictly_increasing() {
        let versions: Vec<u32> = MIGRATIONS.iter().map(|m| m.version).collect();
        assert!(versions.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(versions.first(), Some(&1));
    }

    #[test]
    fn migrate_plain_in_memory_is_idempotent() {
        // 結構測試用記憶體資料庫；加密行為另見整合測試。
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(current_version(&conn).unwrap(), 0);
        assert_eq!(migrate(&mut conn).unwrap(), latest_version());
        assert_eq!(migrate(&mut conn).unwrap(), latest_version());
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, MIGRATIONS.len() as i64);
    }
}
