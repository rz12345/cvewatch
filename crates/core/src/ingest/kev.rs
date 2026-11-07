//! CISA KEV：每次 sync 取完整清單（帶 `If-Modified-Since` / `If-None-Match`），整批取代 `kev`。

use std::collections::HashMap;

use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use reqwest::StatusCode;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::cursor;
use super::source::{SyncCtx, SyncStats};
use crate::error::CoreError;

pub const SOURCE: &str = "kev";
pub const NOT_MODIFIED_NOTE: &str = "KEV 清單未變更";
const RANSOMWARE_KNOWN: &str = "Known";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KevCursor {
    pub last_modified: Option<String>,
    pub etag: Option<String>,
    pub catalog_version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    catalog_version: Option<String>,
    #[serde(default)]
    vulnerabilities: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    #[serde(rename = "cveID")]
    cve_id: String,
    vendor_project: Option<String>,
    product: Option<String>,
    vulnerability_name: Option<String>,
    date_added: String,
    due_date: Option<String>,
    known_ransomware_campaign_use: Option<String>,
    required_action: Option<String>,
}

type Row = (
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    bool,
    Option<String>,
);

impl Entry {
    fn row(&self) -> Row {
        (
            self.vendor_project.clone(),
            self.product.clone(),
            self.vulnerability_name.clone(),
            self.date_added.clone(),
            self.due_date.clone(),
            self.known_ransomware_campaign_use
                .as_deref()
                .is_some_and(|v| v.eq_ignore_ascii_case(RANSOMWARE_KNOWN)),
            self.required_action.clone(),
        )
    }
}

pub async fn sync(ctx: &SyncCtx<'_>, conn: &mut Connection) -> Result<SyncStats, CoreError> {
    let mut stats = SyncStats::default();
    let previous: KevCursor = cursor::get(conn, SOURCE)?.unwrap_or_default();
    let url = &ctx.endpoints.kev;
    let resp = ctx
        .http
        .send(|c| {
            let mut req = c.get(url);
            if let Some(v) = &previous.last_modified {
                req = req.header(IF_MODIFIED_SINCE, v);
            }
            if let Some(v) = &previous.etag {
                req = req.header(IF_NONE_MATCH, v);
            }
            req
        })
        .await?;
    if resp.status() == StatusCode::NOT_MODIFIED {
        stats.note(NOT_MODIFIED_NOTE);
        return Ok(stats);
    }
    let header = |name| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let (last_modified, etag) = (header(LAST_MODIFIED), header(ETAG));
    let catalog: Catalog = serde_json::from_slice(&ctx.http.bytes(resp).await?)
        .map_err(|e| CoreError::Invalid(format!("KEV 格式錯誤：{e}")))?;
    let tx = conn.transaction()?;
    replace(&tx, &catalog.vulnerabilities, &mut stats)?;
    let next = KevCursor {
        last_modified,
        etag,
        catalog_version: catalog.catalog_version,
    };
    cursor::set(&tx, SOURCE, &next, &ctx.now_str())?;
    tx.commit()?;
    Ok(stats)
}

fn existing(conn: &Connection) -> Result<HashMap<String, Row>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT cve_id, vendor, product, vulnerability_name, date_added, due_date,
                known_ransomware, required_action FROM kev",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            (
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ),
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn replace(conn: &Connection, entries: &[Entry], stats: &mut SyncStats) -> Result<(), CoreError> {
    let before = existing(conn)?;
    conn.execute("DELETE FROM kev", [])?;
    let mut stmt = conn.prepare(
        "INSERT OR REPLACE INTO kev (cve_id, vendor, product, vulnerability_name, date_added,
                                     due_date, known_ransomware, required_action)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for e in entries {
        let row = e.row();
        match before.get(&e.cve_id) {
            None => stats.added += 1,
            Some(old) if *old != row => stats.updated += 1,
            Some(_) => {}
        }
        stmt.execute(params![
            e.cve_id, row.0, row.1, row.2, row.3, row.4, row.5, row.6
        ])?;
    }
    Ok(())
}
