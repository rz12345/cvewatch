//! `cvewatch sync [--source <name>...] [--rescan] [--since <date>] [--history]`

use cvewatch_core::catalog::store::sync_catalog;
use cvewatch_core::db::lock::{acquire_sync_lock, release_sync_lock, LockOutcome};
use cvewatch_core::db::open_working_db;
use cvewatch_core::http::HttpClient;
use cvewatch_core::ingest::endpoints::Endpoints;
use cvewatch_core::ingest::relevance::{package_keys, RelevanceIndex};
use cvewatch_core::ingest::runlog::{self, HISTORY_LIMIT};
use cvewatch_core::ingest::source::{SyncCtx, SyncOptions};
use cvewatch_core::ingest::sync::{self, SourcePlan, SyncReport};
use cvewatch_core::secrets::{SecretKey, SecretStore};
use cvewatch_core::{CoreError, ExitCode};
use rusqlite::Connection;
use zeroize::Zeroizing;

use crate::app::App;
use crate::cli::SyncArgs;
use crate::progress::BarProgress;

pub fn run(app: &App, args: SyncArgs) -> Result<ExitCode, CoreError> {
    // 先驗證 --source，避免開啟資料庫後才發現參數錯誤
    let plans = sync::plan(&app.loaded.config.sources, &args.source)?;
    let mut conn = open_working_db(&app.paths, &app.store)?;
    if args.history {
        return history(&conn);
    }
    let pid = std::process::id();
    if let LockOutcome::ReplacedStale {
        previous_pid,
        previous_since,
    } = acquire_sync_lock(&mut conn, pid, chrono::Utc::now())?
    {
        eprintln!("警告：取代過期的 sync 鎖（PID {previous_pid}，開始於 {previous_since}）");
    }
    let result = run_locked(app, &mut conn, &args, &plans);
    if let Err(e) = release_sync_lock(&conn, pid) {
        tracing::error!(error = %e, "釋放 sync 鎖失敗");
        eprintln!("警告：釋放 sync 鎖失敗：{e}");
    }
    result
}

/// 只在需要 NVD 時讀取 API key；憑證庫讀取失敗時以無 key 速率繼續。
fn nvd_key(app: &App, plans: &[SourcePlan]) -> Option<Zeroizing<String>> {
    if !plans.iter().any(|p| p.name == "nvd") {
        return None;
    }
    match app.store.get(SecretKey::NvdApiKey) {
        Ok(key) => key,
        Err(e) => {
            tracing::warn!(error = %e, "無法讀取 NVD API key，改用無 key 速率");
            eprintln!("警告：無法讀取 NVD API key（{e}），改用無 key 速率");
            None
        }
    }
}

pub fn runtime() -> Result<tokio::runtime::Runtime, CoreError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CoreError::Invalid(format!("無法建立非同步執行環境：{e}")))
}

fn run_locked(
    app: &App,
    conn: &mut Connection,
    args: &SyncArgs,
    plans: &[SourcePlan],
) -> Result<ExitCode, CoreError> {
    // 先送出上次留在佇列中的通知（失敗只警告）
    let notifier = super::notify::notifier(app);
    super::notify::flush_before_sync(&notifier, conn);
    let catalog = app.load_catalog()?;
    let product_ids = {
        let tx = conn.transaction()?;
        let ids = sync_catalog(&tx, &catalog)?;
        tx.commit()?;
        ids
    };
    let index = RelevanceIndex::new(&catalog, package_keys(conn)?);
    let key = nvd_key(app, plans);
    let http =
        HttpClient::new(&app.loaded.config.http)?.with_known_secrets(key.iter().cloned().collect());
    let endpoints = Endpoints::default();
    let options = SyncOptions {
        rescan: args.rescan,
        since: args.since,
    };
    let progress = BarProgress::new();
    let ctx = SyncCtx {
        http: &http,
        endpoints: &endpoints,
        catalog: &catalog,
        product_ids: &product_ids,
        index: &index,
        paths: &app.paths,
        options: &options,
        now: chrono::Utc::now(),
        progress: &progress,
        nvd_api_key: key.as_deref().map(String::as_str),
        nvd_interval: None,
    };
    let rt = runtime()?;
    // 各來源在寫入交易內、資料有變動時記錄「比對待補」；結尾的全量比對成功時清除
    let report = rt.block_on(sync::run(&ctx, conn, plans))?;
    print_report(&report);
    // 部分來源失敗時仍以成功來源的資料重新比對；比對失敗不影響已寫入的同步結果
    let matched = super::matching::rematch_if_needed(app, conn, &catalog).unwrap_or_else(|| {
        println!("同步沒有影響比對的資料變更，略過比對。");
        ExitCode::Success
    });
    // 依目前資料判定緊急事件（含先前 match／匯入產生、尚未通知者），寄信與開案；失敗不影響同步結果
    super::notify::notify_after_sync(&notifier, conn);
    // 略過比對時也提醒：版本待補的元件不論資料是否變動都未納入比對（18 design D7）
    super::matching::print_pending(conn);
    Ok(match (report.exit_code(), matched) {
        (ExitCode::Success, code) => code,
        (code, _) => code,
    })
}

/// 各來源的結果與提醒（例如 `eol` 查無代稱；純函式，便於測試）。
pub fn report_lines(report: &SyncReport) -> Vec<String> {
    let mut out = Vec::new();
    for o in &report.outcomes {
        match &o.result {
            Ok(stats) => {
                out.push(format!(
                    "✓ {}：新增 {}、更新 {}",
                    o.name, stats.added, stats.updated
                ));
                out.extend(stats.notes.iter().map(|note| format!("    - {note}")));
            }
            Err(e) => out.push(format!("✗ {}：失敗：{e}", o.name)),
        }
    }
    out
}

fn print_report(report: &SyncReport) {
    for line in report_lines(report) {
        println!("{line}");
    }
    let failed: Vec<&str> = report.failed().map(|o| o.name.as_str()).collect();
    if !failed.is_empty() {
        eprintln!(
            "部分來源失敗（{}），其他來源已更新；失敗來源的資料與游標維持不變（結束碼 4）",
            failed.join("、")
        );
    }
}

fn history(conn: &Connection) -> Result<ExitCode, CoreError> {
    let rows = runlog::history(conn, HISTORY_LIMIT)?;
    if rows.is_empty() {
        println!("尚無同步紀錄。");
        return Ok(ExitCode::Success);
    }
    println!("開始時間                   來源            狀態      新增    更新  訊息");
    for r in rows {
        let status = match r.status.as_str() {
            "success" => "成功",
            "failed" => "失敗",
            _ => "執行中",
        };
        println!(
            "{:<26} {:<15} {:<6} {:>6} {:>7}  {}",
            r.started_at.get(..19).unwrap_or(&r.started_at),
            r.source,
            status,
            r.added,
            r.updated,
            r.error.unwrap_or_default()
        );
    }
    Ok(ExitCode::Success)
}

#[cfg(test)]
mod tests {
    use cvewatch_core::ingest::source::SyncStats;
    use cvewatch_core::ingest::sync::SourceOutcome;

    use super::*;

    #[test]
    fn eol_notes_are_listed_under_the_source() {
        let mut stats = SyncStats {
            added: 3,
            ..SyncStats::default()
        };
        stats.note(
            "endoflife.date 沒有產品 no-such（目錄產品 nginx），請檢查 catalog.d 的 endoflife 設定",
        );
        let report = SyncReport {
            outcomes: vec![SourceOutcome {
                name: "eol".into(),
                result: Ok(stats),
            }],
        };
        assert_eq!(
            report_lines(&report),
            [
                "✓ eol：新增 3、更新 0",
                "    - endoflife.date 沒有產品 no-such（目錄產品 nginx），請檢查 catalog.d 的 endoflife 設定",
            ]
        );
    }
}
