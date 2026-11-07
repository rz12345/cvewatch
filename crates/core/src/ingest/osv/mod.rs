//! OSV.dev：以資產套件（ecosystem + name + version）批次查詢，保存命中的漏洞、CVE 別名與修補版本。

pub mod ecosystem;

use std::collections::{BTreeSet, HashMap};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use self::ecosystem::OsvPackage;
use super::compress;
use super::source::{SyncCtx, SyncStats};
use crate::error::CoreError;

pub const SOURCE: &str = "osv";
/// querybatch 每次最多的查詢數。
pub const BATCH_SIZE: usize = 1000;
/// 同一查詢最多跟隨幾次 `next_page_token`。
const MAX_PAGES: usize = 20;
const CVE_PREFIX: &str = "CVE-";

struct Target {
    component_id: i64,
    package: OsvPackage,
    version: String,
}

#[derive(Serialize)]
struct Query<'a> {
    package: QueryPackage<'a>,
    version: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    page_token: Option<&'a str>,
}

#[derive(Serialize)]
struct QueryPackage<'a> {
    name: &'a str,
    ecosystem: &'a str,
}

#[derive(Deserialize)]
struct BatchResponse {
    #[serde(default)]
    results: Vec<BatchResult>,
}

#[derive(Deserialize, Default)]
struct BatchResult {
    #[serde(default)]
    vulns: Vec<VulnRef>,
    next_page_token: Option<String>,
}

#[derive(Deserialize, Clone)]
struct VulnRef {
    id: String,
    modified: Option<String>,
}

/// OSV 漏洞詳細資料中需要的部分。
#[derive(Deserialize)]
struct Vuln {
    id: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    upstream: Vec<String>,
    #[serde(default)]
    affected: Vec<Affected>,
}

#[derive(Deserialize)]
struct Affected {
    package: Option<AffectedPackage>,
    #[serde(default)]
    ranges: Vec<Range>,
}

#[derive(Deserialize)]
struct AffectedPackage {
    #[serde(default)]
    ecosystem: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct Range {
    #[serde(default)]
    events: Vec<Value>,
}

fn targets(conn: &Connection, stats: &mut SyncStats) -> Result<Vec<Target>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, purl, version_raw FROM active_component WHERE kind = 'package' ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut unsupported = 0;
    let out = rows
        .into_iter()
        .filter_map(
            |(component_id, purl, version)| match ecosystem::from_purl(&purl) {
                Some(package) => Some(Target {
                    component_id,
                    package,
                    version,
                }),
                None => {
                    unsupported += 1;
                    None
                }
            },
        )
        .collect();
    if unsupported > 0 {
        stats.note(format!("{unsupported} 個套件的生態系 OSV 不支援，已略過"));
    }
    Ok(out)
}

pub async fn sync(ctx: &SyncCtx<'_>, conn: &mut Connection) -> Result<SyncStats, CoreError> {
    let mut stats = SyncStats::default();
    let targets = targets(conn, &mut stats)?;
    // 有套件在上次成功同步後才變更：這次成功後即可比對
    let awaiting = any_awaiting(conn)?;
    // 與 targets 同順序
    let mut hits: Vec<Vec<VulnRef>> = Vec::with_capacity(targets.len());
    for chunk in targets.chunks(BATCH_SIZE) {
        hits.extend(query_chunk(ctx, chunk).await?);
    }
    let now = ctx.now_str();
    let mut details: Vec<(String, Vec<u8>, Option<String>)> = Vec::new();
    let unique: BTreeSet<(String, Option<String>)> = hits
        .iter()
        .flat_map(|v| v.iter().map(|r| (r.id.clone(), r.modified.clone())))
        .collect();
    for (id, modified) in unique {
        if !cached_is_current(conn, &id, modified.as_deref())? {
            details.push((id.clone(), fetch_vuln(ctx, &id).await?, modified));
        }
    }
    let tx = conn.transaction()?;
    for (id, raw, modified) in &details {
        tx.execute(
            "INSERT INTO osv_vuln (osv_id, modified, raw, fetched_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (osv_id) DO UPDATE SET modified = excluded.modified, raw = excluded.raw,
                 fetched_at = excluded.fetched_at",
            params![id, modified, compress::compress(raw)?, now],
        )?;
    }
    let mut changed = awaiting;
    for (target, vulns) in targets.iter().zip(&hits) {
        changed |= replace_hits(&tx, target, vulns, &now, &mut stats)?;
    }
    ctx.commit(tx, changed)?;
    Ok(stats)
}

/// 查詢一批；有 `next_page_token` 的查詢繼續翻頁。回傳與輸入同順序的命中清單。
async fn query_chunk(ctx: &SyncCtx<'_>, chunk: &[Target]) -> Result<Vec<Vec<VulnRef>>, CoreError> {
    let mut found: Vec<Vec<VulnRef>> = vec![Vec::new(); chunk.len()];
    let mut tokens: Vec<Option<String>> = vec![None; chunk.len()];
    let mut active: Vec<usize> = (0..chunk.len()).collect();
    for _ in 0..MAX_PAGES {
        if active.is_empty() {
            break;
        }
        let queries: Vec<Query<'_>> = active
            .iter()
            .map(|&i| Query {
                package: QueryPackage {
                    name: &chunk[i].package.name,
                    ecosystem: &chunk[i].package.ecosystem,
                },
                version: &chunk[i].version,
                page_token: tokens[i].as_deref(),
            })
            .collect();
        let body = serde_json::to_vec(&serde_json::json!({ "queries": queries }))
            .map_err(|e| CoreError::Invalid(format!("OSV 查詢序列化失敗：{e}")))?;
        let url = format!("{}/querybatch", ctx.endpoints.osv);
        let resp = ctx
            .http
            .send(|c| {
                c.post(&url)
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(body.clone())
            })
            .await?;
        let parsed: BatchResponse = serde_json::from_slice(&ctx.http.bytes(resp).await?)
            .map_err(|e| CoreError::Invalid(format!("OSV 回應格式錯誤：{e}")))?;
        let mut next = Vec::new();
        for (pos, &i) in active.iter().enumerate() {
            let result = parsed.results.get(pos);
            found[i].extend(result.map(|r| r.vulns.clone()).unwrap_or_default());
            tokens[i] = result.and_then(|r| r.next_page_token.clone());
            if tokens[i].is_some() {
                next.push(i);
            }
        }
        active = next;
    }
    Ok(found)
}

fn cached_is_current(
    conn: &Connection,
    id: &str,
    modified: Option<&str>,
) -> Result<bool, CoreError> {
    let cached: Option<Option<String>> = conn
        .query_row(
            "SELECT modified FROM osv_vuln WHERE osv_id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match (cached, modified) {
        (Some(Some(old)), Some(new)) => old.as_str() >= new,
        (Some(_), None) => true,
        _ => false,
    })
}

async fn fetch_vuln(ctx: &SyncCtx<'_>, id: &str) -> Result<Vec<u8>, CoreError> {
    let url = format!("{}/vulns/{}", ctx.endpoints.osv, urlencode(id));
    let resp = ctx.http.get(&url).await?;
    Ok(ctx.http.bytes(resp).await?)
}

fn urlencode(segment: &str) -> String {
    url::form_urlencoded::byte_serialize(segment.as_bytes()).collect()
}

fn load_vuln(conn: &Connection, id: &str) -> Result<Option<Vuln>, CoreError> {
    let raw: Option<Vec<u8>> = conn
        .query_row(
            "SELECT raw FROM osv_vuln WHERE osv_id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let bytes = compress::decompress(&raw)?;
    Ok(serde_json::from_slice(&bytes).ok())
}

/// CVE 別名：`aliases`、`upstream`（Debian 等）與 id 本身中的 CVE-*。
fn cve_ids(v: &Vuln) -> Vec<String> {
    let ids: BTreeSet<String> = std::iter::once(&v.id)
        .chain(v.aliases.iter())
        .chain(v.upstream.iter())
        .filter(|s| s.starts_with(CVE_PREFIX))
        .cloned()
        .collect();
    ids.into_iter().collect()
}

/// 與查詢相同 ecosystem/name 的 `fixed` 事件。
fn fixed_versions(v: &Vuln, package: &OsvPackage) -> Vec<String> {
    let fixed: BTreeSet<String> = v
        .affected
        .iter()
        .filter(|a| {
            a.package.as_ref().is_some_and(|p| {
                p.ecosystem.eq_ignore_ascii_case(&package.ecosystem) && p.name == package.name
            })
        })
        .flat_map(|a| a.ranges.iter().flat_map(|r| r.events.iter()))
        .filter_map(|e| e.get("fixed").and_then(Value::as_str).map(str::to_string))
        .collect();
    fixed.into_iter().collect()
}

/// 是否有使用中的套件在最近一次成功的 OSV 同步之後才變更（版本變更、恢復、資產重新啟用）。
fn any_awaiting(conn: &Connection) -> Result<bool, CoreError> {
    let last = super::runlog::last_success(conn, SOURCE)?;
    let mut stmt =
        conn.prepare("SELECT updated_at FROM active_component WHERE kind = 'package'")?;
    let updated = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(updated
        .iter()
        .any(|u| !crate::matching::evidence::osv::osv_is_current(u, last.as_deref())))
}

/// 以這次查詢結果取代元件的命中；回傳命中內容是否有變動（新增、移除或 CVE／修正版本改變）。
fn replace_hits(
    conn: &Connection,
    target: &Target,
    vulns: &[VulnRef],
    now: &str,
    stats: &mut SyncStats,
) -> Result<bool, CoreError> {
    let before: HashMap<String, (String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT osv_id, cve_ids_json, fixed_versions_json FROM osv_hit WHERE component_id = ?1",
        )?;
        let rows = stmt.query_map(params![target.component_id], |r| {
            Ok((r.get(0)?, (r.get(1)?, r.get(2)?)))
        })?;
        rows.collect::<Result<_, _>>()?
    };
    let mut changed = before.len() != vulns.len();
    conn.execute(
        "DELETE FROM osv_hit WHERE component_id = ?1",
        params![target.component_id],
    )?;
    for r in vulns {
        let detail = load_vuln(conn, &r.id)?;
        let (cves, fixed) = detail
            .as_ref()
            .map(|v| (cve_ids(v), fixed_versions(v, &target.package)))
            .unwrap_or_default();
        let row = (json_array(&cves)?, json_array(&fixed)?);
        changed |= before.get(&r.id) != Some(&row);
        conn.execute(
            "INSERT OR REPLACE INTO osv_hit (component_id, osv_id, modified, cve_ids_json,
                                             fixed_versions_json, queried_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![target.component_id, r.id, r.modified, row.0, row.1, now],
        )?;
        if before.contains_key(&r.id) {
            stats.updated += 1;
        } else {
            stats.added += 1;
        }
    }
    Ok(changed)
}

fn json_array(items: &[String]) -> Result<String, CoreError> {
    serde_json::to_string(items).map_err(|e| CoreError::Invalid(format!("OSV 命中序列化失敗：{e}")))
}
