//! endoflife.date API v1（22 design D1、D2）：對使用中清冊元件所屬、目錄宣告了代稱的產品，
//! 逐一取得 `GET /products/{slug}`（帶 `If-None-Match`），整批取代該代稱的版本分支。
//!
//! 每個代稱各自交易；404 不算失敗（記錄提醒、保留先前資料）；其他錯誤在處理完全部代稱後
//! 讓來源失敗。EoL 不影響比對，提交一律不標記「比對待補」。

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use reqwest::header::{ETAG, IF_NONE_MATCH};
use reqwest::StatusCode;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::cursor;
use super::source::{SyncCtx, SyncStats};
use crate::catalog::Catalog;
use crate::error::CoreError;

pub const SOURCE: &str = "eol";
/// 請求間隔（API 未公布速率限制下的禮貌間隔）。
pub const EOL_REQUEST_INTERVAL: Duration = Duration::from_millis(250);

/// `sync_cursor('eol')`：代稱 → 上次的 ETag 與取得時間。
pub type EolCursor = BTreeMap<String, SlugCursor>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlugCursor {
    pub etag: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Deserialize)]
struct ProductResponse {
    last_modified: Option<String>,
    result: ProductDetails,
}

#[derive(Debug, Deserialize)]
struct ProductDetails {
    label: Option<String>,
    #[serde(default)]
    links: Links,
    releases: Vec<ReleaseDto>,
}

#[derive(Debug, Default, Deserialize)]
struct Links {
    html: Option<String>,
}

/// `ProductRelease`：只取 design D1 列出的欄位，其他欄位忽略。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseDto {
    name: String,
    label: Option<String>,
    release_date: Option<String>,
    is_eol: bool,
    eol_from: Option<String>,
    eoas_from: Option<String>,
    eoes_from: Option<String>,
    latest: Option<Latest>,
}

#[derive(Debug, Deserialize)]
struct Latest {
    name: String,
}

/// 一個版本分支（`eol_release` 一列；`position` 為回應中的順序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseRow {
    pub name: String,
    pub label: Option<String>,
    pub release_date: Option<String>,
    pub is_eol: bool,
    pub eol_from: Option<String>,
    pub eoas_from: Option<String>,
    pub eoes_from: Option<String>,
    pub latest_name: Option<String>,
}

/// 解析後的產品資料。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProduct {
    pub label: Option<String>,
    pub html_link: Option<String>,
    pub last_modified: Option<String>,
    pub releases: Vec<ReleaseRow>,
}

fn check_date(value: &Option<String>, field: &str, release: &str) -> Result<(), String> {
    match value {
        Some(v) if crate::assets::dates::parse_iso_date(v).is_none() => {
            Err(format!("分支 {release} 的 {field}「{v}」不是 YYYY-MM-DD"))
        }
        _ => Ok(()),
    }
}

/// 解析 `GET /products/{slug}` 的回應；非 JSON、缺必有欄位或日期格式不符時為錯誤。
pub fn parse_product(body: &[u8]) -> Result<ParsedProduct, String> {
    let resp: ProductResponse = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    let releases = resp
        .result
        .releases
        .into_iter()
        .map(|r| {
            check_date(&r.eol_from, "eolFrom", &r.name)?;
            check_date(&r.eoas_from, "eoasFrom", &r.name)?;
            check_date(&r.eoes_from, "eoesFrom", &r.name)?;
            Ok(ReleaseRow {
                name: r.name,
                label: r.label,
                release_date: r.release_date,
                is_eol: r.is_eol,
                eol_from: r.eol_from,
                eoas_from: r.eoas_from,
                eoes_from: r.eoes_from,
                latest_name: r.latest.map(|l| l.name),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ParsedProduct {
        label: resp.result.label,
        html_link: resp.result.links.html,
        last_modified: resp.last_modified,
        releases,
    })
}

/// 要查詢的代稱 → 使用該代稱的目錄產品 key：使用中資產的清冊元件（含版本待補）所屬產品。
pub fn wanted_slugs(
    conn: &Connection,
    catalog: &Catalog,
) -> Result<BTreeMap<String, Vec<String>>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT p.key FROM component c
         JOIN catalog_product p ON p.id = c.product_id
         JOIN asset a ON a.id = c.asset_id
         WHERE a.status = 'active' AND c.removed_at IS NULL AND c.source = 'excel'
           AND c.kind = 'product'
         ORDER BY p.key",
    )?;
    let keys = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for key in keys {
        if let Some(slug) = catalog.get(&key).and_then(|p| p.endoflife.clone()) {
            out.entry(slug).or_default().push(key);
        }
    }
    Ok(out)
}

/// 一個代稱的處理結果。
enum Fetched {
    Updated,
    NotModified,
    NotFound,
}

pub async fn sync(ctx: &SyncCtx<'_>, conn: &mut Connection) -> Result<SyncStats, CoreError> {
    let mut stats = SyncStats::default();
    let slugs = wanted_slugs(conn, ctx.catalog)?;
    let mut failures: Vec<String> = Vec::new();
    for (i, (slug, keys)) in slugs.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(EOL_REQUEST_INTERVAL).await;
        }
        match sync_slug(ctx, conn, slug, &mut stats).await {
            Ok(Fetched::Updated | Fetched::NotModified) => {}
            Ok(Fetched::NotFound) => stats.note(format!(
                "endoflife.date 沒有產品 {slug}（目錄產品 {}），請檢查 catalog.d 的 endoflife 設定",
                keys.join("、")
            )),
            Err(e) => {
                tracing::warn!(slug = %slug, error = %e, "endoflife.date 代稱同步失敗");
                failures.push(format!("{slug}：{e}"));
            }
        }
    }
    if !failures.is_empty() {
        return Err(CoreError::Invalid(format!(
            "endoflife.date 有 {} 個代稱失敗（其他代稱已更新）：{}",
            failures.len(),
            failures.join("；")
        )));
    }
    Ok(stats)
}

async fn sync_slug(
    ctx: &SyncCtx<'_>,
    conn: &mut Connection,
    slug: &str,
    stats: &mut SyncStats,
) -> Result<Fetched, CoreError> {
    let mut all: EolCursor = cursor::get(conn, SOURCE)?.unwrap_or_default();
    let etag = all.get(slug).and_then(|c| c.etag.clone());
    let url = ctx.endpoints.endoflife_product_url(slug);
    let sent = ctx
        .http
        .send(|c| {
            let req = c.get(&url);
            match &etag {
                Some(v) => req.header(IF_NONE_MATCH, v),
                None => req,
            }
        })
        .await;
    let resp = match sent {
        Ok(resp) => resp,
        Err(e) if e.status() == Some(StatusCode::NOT_FOUND) => {
            let tx = conn.transaction()?;
            mark_not_found(&tx, slug, &ctx.now_str())?;
            // 移除 ETag：之後恢復時以無條件請求取得，狀態才會回到 ok
            if all.remove(slug).is_some() {
                cursor::set(&tx, SOURCE, &all, &ctx.now_str())?;
            }
            ctx.commit(tx, false)?;
            return Ok(Fetched::NotFound);
        }
        Err(e) => return Err(e.into()),
    };
    let now = ctx.now_str();
    if resp.status() == StatusCode::NOT_MODIFIED {
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE eol_product SET fetched_at = ?2, status = 'ok' WHERE slug = ?1",
            params![slug, now],
        )?;
        all.insert(
            slug.to_string(),
            SlugCursor {
                etag,
                fetched_at: now.clone(),
            },
        );
        cursor::set(&tx, SOURCE, &all, &now)?;
        ctx.commit(tx, false)?;
        return Ok(Fetched::NotModified);
    }
    let new_etag = resp
        .headers()
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = ctx.http.bytes(resp).await?;
    let parsed = parse_product(&body)
        .map_err(|e| CoreError::Invalid(format!("endoflife.date {slug} 格式錯誤：{e}")))?;
    let tx = conn.transaction()?;
    replace_product(&tx, slug, &parsed, &now, stats)?;
    all.insert(
        slug.to_string(),
        SlugCursor {
            etag: new_etag,
            fetched_at: now.clone(),
        },
    );
    cursor::set(&tx, SOURCE, &all, &now)?;
    ctx.commit(tx, false)?;
    Ok(Fetched::Updated)
}

/// 404：標為查無此產品，保留先前的分支資料。
fn mark_not_found(conn: &Connection, slug: &str, now: &str) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO eol_product (slug, status, fetched_at) VALUES (?1, 'not_found', ?2)
         ON CONFLICT (slug) DO UPDATE SET status = 'not_found', fetched_at = excluded.fetched_at",
        params![slug, now],
    )?;
    Ok(())
}

fn existing_releases(
    conn: &Connection,
    slug: &str,
) -> Result<HashMap<String, ReleaseRow>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT name, label, release_date, is_eol, eol_from, eoas_from, eoes_from, latest_name
         FROM eol_release WHERE slug = ?1",
    )?;
    let rows = stmt.query_map([slug], |r| {
        Ok(ReleaseRow {
            name: r.get(0)?,
            label: r.get(1)?,
            release_date: r.get(2)?,
            is_eol: r.get(3)?,
            eol_from: r.get(4)?,
            eoas_from: r.get(5)?,
            eoes_from: r.get(6)?,
            latest_name: r.get(7)?,
        })
    })?;
    rows.map(|r| r.map(|row| (row.name.clone(), row)))
        .collect::<Result<_, _>>()
        .map_err(Into::into)
}

/// 整批取代該代稱的分支；`added` 為新出現的分支數，`updated` 為內容變更的分支數。
fn replace_product(
    conn: &Connection,
    slug: &str,
    product: &ParsedProduct,
    now: &str,
    stats: &mut SyncStats,
) -> Result<(), CoreError> {
    let before = existing_releases(conn, slug)?;
    conn.execute(
        "INSERT INTO eol_product (slug, label, html_link, status, last_modified, fetched_at)
         VALUES (?1, ?2, ?3, 'ok', ?4, ?5)
         ON CONFLICT (slug) DO UPDATE SET label = excluded.label, html_link = excluded.html_link,
             status = 'ok', last_modified = excluded.last_modified, fetched_at = excluded.fetched_at",
        params![slug, product.label, product.html_link, product.last_modified, now],
    )?;
    conn.execute("DELETE FROM eol_release WHERE slug = ?1", [slug])?;
    let mut stmt = conn.prepare(
        "INSERT OR REPLACE INTO eol_release (slug, name, label, release_date, is_eol, eol_from,
                                             eoas_from, eoes_from, latest_name, position)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    for (position, r) in product.releases.iter().enumerate() {
        match before.get(&r.name) {
            None => stats.added += 1,
            Some(old) if old != r => stats.updated += 1,
            Some(_) => {}
        }
        stmt.execute(params![
            slug,
            r.name,
            r.label,
            r.release_date,
            r.is_eol,
            r.eol_from,
            r.eoas_from,
            r.eoes_from,
            r.latest_name,
            i64::try_from(position).unwrap_or(i64::MAX),
        ])?;
    }
    Ok(())
}

/// 已保存的產品狀態（`ok`／`not_found`）。
pub fn product_status(conn: &Connection, slug: &str) -> Result<Option<String>, CoreError> {
    Ok(conn
        .query_row(
            "SELECT status FROM eol_product WHERE slug = ?1",
            [slug],
            |r| r.get(0),
        )
        .optional()?)
}

#[cfg(test)]
#[path = "eol_tests.rs"]
mod tests;
