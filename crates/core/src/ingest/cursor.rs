//! 各來源的游標（`sync_cursor`，JSON）。

use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::CoreError;

pub fn get<T: DeserializeOwned>(conn: &Connection, source: &str) -> Result<Option<T>, CoreError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT cursor_json FROM sync_cursor WHERE source = ?1",
            params![source],
            |r| r.get(0),
        )
        .optional()?;
    match json {
        None => Ok(None),
        Some(text) => match serde_json::from_str(&text) {
            Ok(value) => Ok(Some(value)),
            Err(e) => {
                // 游標格式不符（例如程式升級改了結構）時視為沒有游標，從頭同步。
                tracing::warn!(source, error = %e, "游標格式不符，重新開始");
                Ok(None)
            }
        },
    }
}

pub fn set<T: Serialize>(
    conn: &Connection,
    source: &str,
    value: &T,
    now: &str,
) -> Result<(), CoreError> {
    let json = serde_json::to_string(value)
        .map_err(|e| CoreError::Invalid(format!("無法序列化游標：{e}")))?;
    conn.execute(
        "INSERT INTO sync_cursor (source, cursor_json, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (source) DO UPDATE SET cursor_json = excluded.cursor_json,
             updated_at = excluded.updated_at",
        params![source, json, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate::migrate(&mut conn).unwrap();
        conn
    }

    #[test]
    fn roundtrip_and_overwrite() {
        let conn = db();
        assert_eq!(get::<Vec<u32>>(&conn, "kev").unwrap(), None);
        set(&conn, "kev", &vec![1u32], "t1").unwrap();
        set(&conn, "kev", &vec![2u32], "t2").unwrap();
        assert_eq!(get::<Vec<u32>>(&conn, "kev").unwrap(), Some(vec![2]));
    }

    #[test]
    fn malformed_cursor_is_ignored() {
        let conn = db();
        set(&conn, "nvd", &"text", "t").unwrap();
        assert_eq!(get::<Vec<u32>>(&conn, "nvd").unwrap(), None);
    }
}
