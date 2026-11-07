//! cvelistV5 同步（主來源）。演算法見 design.md「cvelistV5 同步演算法」。

pub mod process;
pub mod releases;
pub mod search_index;
pub mod zipstream;

use chrono::NaiveDate;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use self::process::Processor;
use self::releases::{AssetRef, Release, RELEASES_PER_PAGE};
use self::search_index::IndexWriter;
use self::zipstream::{Layout, BASELINE_TIMEOUT};
use super::cursor;
use super::endpoints::Endpoints;
use super::relevance::Relevance;
use super::source::{SyncCtx, SyncStats};
use crate::error::CoreError;
use crate::http::HttpClient;

pub const SOURCE: &str = "cvelist";
/// 距離上次同步超過此天數時改抓 baseline。
pub const BASELINE_MAX_GAP_DAYS: i64 = 25;
/// 每次 sync 最多補齊幾筆尚無 CVE.org 紀錄的 CVE。
pub const BACKFILL_LIMIT: u32 = 50;
pub const RESCAN_HINT: &str = "目錄已變更，請執行 sync --rescan 以取得既有 CVE";

const GITHUB_ACCEPT: &str = "application/vnd.github+json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CvelistCursor {
    pub baseline_date: NaiveDate,
    /// 目前套用到的日期（UTC）
    pub day: NaiveDate,
    /// `day` 已套用的 delta asset 名稱
    pub last_delta: Option<String>,
    /// 上次以 baseline 篩選時的目錄雜湊
    pub catalog_hash: String,
}

enum Start {
    Baseline,
    Continue(CvelistCursor),
}

fn plan(cursor: Option<CvelistCursor>, ctx: &SyncCtx<'_>) -> Start {
    let Some(c) = cursor else {
        return Start::Baseline;
    };
    if ctx.options.rescan {
        return Start::Baseline;
    }
    let c = match ctx.options.since {
        Some(since) => CvelistCursor {
            day: since,
            last_delta: None,
            ..c
        },
        None => c,
    };
    if (ctx.today() - c.day).num_days() > BASELINE_MAX_GAP_DAYS {
        Start::Baseline
    } else {
        Start::Continue(c)
    }
}

pub async fn sync(ctx: &SyncCtx<'_>, conn: &mut Connection) -> Result<SyncStats, CoreError> {
    let mut stats = SyncStats::default();
    let fingerprint = ctx.index.fingerprint();
    let releases = fetch_releases(ctx).await?;
    let mut cursor = match plan(cursor::get(conn, SOURCE)?, ctx) {
        Start::Baseline => baseline(ctx, conn, &releases, &fingerprint, &mut stats).await?,
        Start::Continue(c) => {
            if c.catalog_hash != fingerprint {
                stats.note(RESCAN_HINT);
            }
            c
        }
    };
    apply_deltas(ctx, conn, &releases, &mut cursor, &mut stats).await?;
    backfill(ctx, conn, &mut stats).await;
    Ok(stats)
}

async fn fetch_releases(ctx: &SyncCtx<'_>) -> Result<Vec<Release>, CoreError> {
    let url = ctx.endpoints.releases_url(RELEASES_PER_PAGE);
    let resp = ctx
        .http
        .send(|c| c.get(&url).header(reqwest::header::ACCEPT, GITHUB_ACCEPT))
        .await?;
    releases::parse_releases(&ctx.http.bytes(resp).await?)
}

async fn baseline(
    ctx: &SyncCtx<'_>,
    conn: &mut Connection,
    releases: &[Release],
    fingerprint: &str,
    stats: &mut SyncStats,
) -> Result<CvelistCursor, CoreError> {
    let asset = releases::latest_baseline(releases)
        .ok_or_else(|| CoreError::Invalid("cvelistV5 releases 中找不到 baseline".into()))?;
    tracing::info!(asset = %asset.name, "下載 cvelistV5 baseline");
    let url = ctx.endpoints.download_url(&asset.tag, &asset.name);
    let now = ctx.now_str();
    let mut writer = IndexWriter::rebuild(&ctx.paths.cvelist_cache_dir)?;
    let tx = conn.transaction()?;
    let (added, updated) = {
        let mut p = Processor::new(&tx, ctx.index, &now, Some(&mut writer));
        let mut handle = |name: &str, bytes: &[u8]| p.handle(name, bytes);
        zipstream::stream_http(ctx.http, &url, Layout::Nested, ctx.progress, &mut handle).await?;
        tracing::info!(
            records = p.seen,
            malformed = p.malformed,
            "baseline 處理完成"
        );
        (p.added, p.updated)
    };
    let cursor = CvelistCursor {
        baseline_date: asset.date,
        day: asset.date,
        last_delta: None,
        catalog_hash: fingerprint.to_string(),
    };
    cursor::set(&tx, SOURCE, &cursor, &now)?;
    ctx.commit(tx, added + updated > 0)?;
    writer.finish()?;
    stats.added += added;
    stats.updated += updated;
    Ok(cursor)
}

/// 依游標逐日套用 delta；每一步各自交易並寫入游標。
async fn apply_deltas(
    ctx: &SyncCtx<'_>,
    conn: &mut Connection,
    releases: &[Release],
    cursor: &mut CvelistCursor,
    stats: &mut SyncStats,
) -> Result<(), CoreError> {
    let today = ctx.today();
    while cursor.day <= today {
        let day = cursor.day;
        if day == today {
            let hourly = releases::latest_hourly_delta(releases, day);
            apply_if_new(ctx, conn, cursor, hourly.as_ref(), stats).await?;
            break;
        }
        let eod = releases::end_of_day_delta(day);
        if !apply_if_new(ctx, conn, cursor, Some(&eod), stats).await? {
            // end_of_day 尚未發布：先套用列表中當日最新的每小時 delta
            let hourly = releases::latest_hourly_delta(releases, day);
            apply_if_new(ctx, conn, cursor, hourly.as_ref(), stats).await?;
            if day.succ_opt() == Some(today) {
                break; // 昨天的 end_of_day 下次再試
            }
            stats.note(format!("{day} 的 end_of_day delta 不存在，已略過"));
        }
        *cursor = CvelistCursor {
            day: day.succ_opt().unwrap_or(today),
            last_delta: None,
            ..cursor.clone()
        };
        cursor::set(conn, SOURCE, &*cursor, &ctx.now_str())?;
    }
    Ok(())
}

/// 下載並套用一個 delta。回傳 true 表示該 asset 已套用（含先前已套用），false 表示不存在。
async fn apply_if_new(
    ctx: &SyncCtx<'_>,
    conn: &mut Connection,
    cursor: &mut CvelistCursor,
    asset: Option<&AssetRef>,
    stats: &mut SyncStats,
) -> Result<bool, CoreError> {
    let Some(asset) = asset else {
        return Ok(false);
    };
    if cursor.last_delta.as_deref() == Some(asset.name.as_str()) {
        return Ok(true);
    }
    let url = ctx.endpoints.download_url(&asset.tag, &asset.name);
    let resp = match ctx.http.get_with_timeout(&url, BASELINE_TIMEOUT).await {
        Ok(r) => r,
        Err(e) if e.status() == Some(reqwest::StatusCode::NOT_FOUND) => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    let bytes = ctx.http.bytes(resp).await?;
    let now = ctx.now_str();
    let mut writer = IndexWriter::append(&ctx.paths.cvelist_cache_dir)?;
    let tx = conn.transaction()?;
    let (added, updated) = {
        let mut p = Processor::new(&tx, ctx.index, &now, Some(&mut writer));
        let mut handle = |name: &str, b: &[u8]| p.handle(name, b);
        zipstream::for_each_entry(bytes.as_slice(), Layout::Flat, &mut handle)?;
        (p.added, p.updated)
    };
    *cursor = CvelistCursor {
        last_delta: Some(asset.name.clone()),
        ..cursor.clone()
    };
    cursor::set(&tx, SOURCE, &*cursor, &now)?;
    ctx.commit(tx, added + updated > 0)?;
    writer.finish()?;
    tracing::info!(asset = %asset.name, added, updated, "已套用 delta");
    stats.added += added;
    stats.updated += updated;
    Ok(true)
}

/// 取得單筆 CVE.org 紀錄；不存在（404）時為 None。
pub async fn fetch_record(
    http: &HttpClient,
    endpoints: &Endpoints,
    cve_id: &str,
) -> Result<Option<Vec<u8>>, CoreError> {
    let url = endpoints
        .record_url(cve_id)
        .ok_or_else(|| CoreError::Invalid(format!("`{cve_id}` 不是有效的 CVE ID")))?;
    match http.get(&url).await {
        Ok(resp) => Ok(Some(http.bytes(resp).await?)),
        Err(e) if e.status() == Some(reqwest::StatusCode::NOT_FOUND) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// 補齊由 NVD / CSAF 先加入、尚無 CVE.org 紀錄的 CVE（盡力而為，失敗只記錄）。
async fn backfill(ctx: &SyncCtx<'_>, conn: &mut Connection, stats: &mut SyncStats) {
    if let Err(e) = try_backfill(ctx, conn, stats).await {
        tracing::warn!(error = %e, "補齊 CVE 紀錄失敗");
        stats.note(format!("補齊 CVE 紀錄失敗：{e}"));
    }
}

async fn try_backfill(
    ctx: &SyncCtx<'_>,
    conn: &mut Connection,
    stats: &mut SyncStats,
) -> Result<(), CoreError> {
    let pending: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, relevance_reason FROM cve WHERE raw_cna_json IS NULL
             ORDER BY updated_at, id LIMIT ?1",
        )?;
        let rows = stmt.query_map([BACKFILL_LIMIT], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let now = ctx.now_str();
    for (id, reason) in pending {
        let bytes = match fetch_record(ctx.http, ctx.endpoints, &id).await {
            Ok(Some(bytes)) => bytes,
            // 尚未發布（404）或 ID 無效：排到最後，避免一直佔住前面的名額
            Ok(None) | Err(CoreError::Invalid(_)) => {
                defer_backfill(conn, &id, &now)?;
                continue;
            }
            // 網路錯誤：本次停止補齊（已補齊者保留），下次再試
            Err(e) => return Err(e),
        };
        let fallback = Relevance::parse(&reason).unwrap_or(Relevance::Csaf);
        let tx = conn.transaction()?;
        let mut p = Processor::new(&tx, ctx.index, &now, None);
        match p.handle_known(&bytes, fallback) {
            Ok(()) => {
                let changed = p.updated + p.added;
                ctx.commit(tx, changed > 0)?;
                stats.updated += changed;
            }
            // 資料庫錯誤（例如被鎖定）不是紀錄的問題：中止並回報
            Err(e @ CoreError::Sqlite(_)) => return Err(e),
            // 紀錄無法解析：排到最後，避免每次都佔住前面的名額
            Err(e) => {
                drop(tx);
                tracing::warn!(cve = %id, error = %e, "補齊的 CVE 紀錄無法解析，排到最後");
                defer_backfill(conn, &id, &now)?;
            }
        }
    }
    Ok(())
}

/// 暫時無法補齊的 CVE 排到最後（`updated_at` 為補齊的排序依據）。
fn defer_backfill(conn: &Connection, id: &str, now: &str) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE cve SET updated_at = ?2 WHERE id = ?1",
        rusqlite::params![id, now],
    )?;
    Ok(())
}
