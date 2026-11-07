//! `app_meta` 鍵值表。

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::CoreError;

/// 最後一次匯出復原金鑰的時間（RFC 3339）。
pub const KEY_DB_KEY_EXPORTED_AT: &str = "db_key_exported_at";

pub fn get(conn: &Connection, key: &str) -> Result<Option<String>, CoreError> {
    Ok(conn
        .query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, key: &str) -> Result<(), CoreError> {
    conn.execute("DELETE FROM app_meta WHERE key = ?1", params![key])?;
    Ok(())
}
