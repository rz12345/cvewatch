//! FIRST EPSS：本機已發布的相關 CVE（含 OSV 命中的 CVE）批次查詢，每個 CVE 每天最多一次。

use std::collections::HashSet;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::cursor;
use super::source::{SyncCtx, SyncStats};
use crate::error::CoreError;

pub const SOURCE: &str = "epss";
/// 每次請求最多的 CVE 數。
pub const BATCH_SIZE: usize = 100;

/// 當天查過但 EPSS 沒有分數的 CVE（避免同一天重查）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpssCursor {
    pub unscored_on: Option<String>,
    pub unscored: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Response {
    #[serde(default)]
    data: Vec<Score>,
}

#[derive(Debug, Deserialize)]
struct Score {
    cve: String,
    epss: String,
    percentile: String,
    date: String,
}

/// 今天尚未查詢的 CVE。
fn pending(
    conn: &Connection,
    today: &str,
    skip: &HashSet<String>,
) -> Result<Vec<String>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT id FROM cve WHERE state = 'PUBLISHED'
         UNION
         SELECT DISTINCT j.value FROM osv_hit h, json_each(h.cve_ids_json) j
         EXCEPT
         SELECT cve_id FROM epss WHERE fetched_on = ?1
         ORDER BY 1",
    )?;
    let ids = stmt
        .query_map(params![today], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids.into_iter().filter(|id| !skip.contains(id)).collect())
}

pub async fn sync(ctx: &SyncCtx<'_>, conn: &mut Connection) -> Result<SyncStats, CoreError> {
    let mut stats = SyncStats::default();
    let today = ctx.today().to_string();
    let previous: EpssCursor = cursor::get(conn, SOURCE)?.unwrap_or_default();
    let skip: HashSet<String> = if previous.unscored_on.as_deref() == Some(today.as_str()) {
        previous.unscored.iter().cloned().collect()
    } else {
        HashSet::new()
    };
    let ids = pending(conn, &today, &skip)?;
    let mut scores = Vec::new();
    for batch in ids.chunks(BATCH_SIZE) {
        let url = url::Url::parse_with_params(&ctx.endpoints.epss, &[("cve", batch.join(","))])
            .map_err(|e| CoreError::Invalid(format!("EPSS 網址錯誤：{e}")))?;
        let resp = ctx.http.send(|c| c.get(url.clone())).await?;
        let body: Response = serde_json::from_slice(&ctx.http.bytes(resp).await?)
            .map_err(|e| CoreError::Invalid(format!("EPSS 回應格式錯誤：{e}")))?;
        scores.extend(body.data);
    }
    let tx = conn.transaction()?;
    let scored: HashSet<String> = scores.iter().map(|s| s.cve.clone()).collect();
    for s in &scores {
        let (Ok(score), Ok(percentile)) = (s.epss.parse::<f64>(), s.percentile.parse::<f64>())
        else {
            tracing::warn!(cve = %s.cve, "EPSS 分數格式錯誤，略過");
            continue;
        };
        let existed: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM epss WHERE cve_id = ?1)",
            params![s.cve],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO epss (cve_id, score, percentile, score_date, fetched_on)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (cve_id) DO UPDATE SET score = excluded.score,
                 percentile = excluded.percentile, score_date = excluded.score_date,
                 fetched_on = excluded.fetched_on",
            params![s.cve, score, percentile, s.date, today],
        )?;
        if existed {
            stats.updated += 1;
        } else {
            stats.added += 1;
        }
    }
    let mut unscored: Vec<String> = skip.into_iter().collect();
    unscored.extend(ids.into_iter().filter(|id| !scored.contains(id)));
    unscored.sort();
    let next = EpssCursor {
        unscored_on: Some(today),
        unscored,
    };
    cursor::set(&tx, SOURCE, &next, &ctx.now_str())?;
    tx.commit()?;
    Ok(stats)
}
