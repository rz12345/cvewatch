//! `cvewatch cve show <id> [--fetch]`、`cvewatch cve search <keyword>`

use cvewatch_core::db::open_working_db;
use cvewatch_core::http::HttpClient;
use cvewatch_core::ingest::cve5;
use cvewatch_core::ingest::cvelist::fetch_record;
use cvewatch_core::ingest::endpoints::Endpoints;
use cvewatch_core::ingest::show::{self, MAX_SEARCH_RESULTS};
use cvewatch_core::{CoreError, ExitCode};

use super::cve_view;
use super::sync::runtime;
use crate::app::App;
use crate::cli::CveAction;

pub fn run(app: &App, action: CveAction) -> Result<ExitCode, CoreError> {
    match action {
        CveAction::Show { id, fetch } => show_cve(app, &id, fetch),
        CveAction::Search { keyword } => search(app, &keyword),
    }
}

fn show_cve(app: &App, raw_id: &str, fetch: bool) -> Result<ExitCode, CoreError> {
    let id = show::normalize_id(raw_id)?;
    let conn = open_working_db(&app.paths, &app.store)?;
    if let Some(detail) = show::load(&conn, &id)? {
        cve_view::print_detail(&detail);
        return Ok(ExitCode::Success);
    }
    if !fetch {
        println!("{id} 不在本機（可能與資產無關，同步時已被篩除）。");
        println!("可用 `cvewatch cve show {id} --fetch` 直接向 CVE.org 取得單筆。");
        return Ok(ExitCode::Success);
    }
    let http = HttpClient::new(&app.loaded.config.http)?;
    let endpoints = Endpoints::default();
    let bytes = runtime()?.block_on(fetch_record(&http, &endpoints, &id))?;
    match bytes {
        None => {
            println!("CVE.org 沒有 {id} 的紀錄（尚未發布或 ID 不存在）。");
            Ok(ExitCode::Success)
        }
        Some(bytes) => {
            let record = cve5::parse(&bytes)?;
            cve_view::print_fetched(&record, &show::references(&bytes));
            Ok(ExitCode::Success)
        }
    }
}

fn search(app: &App, keyword: &str) -> Result<ExitCode, CoreError> {
    let conn = open_working_db(&app.paths, &app.store)?;
    let Some(hits) = show::search(&conn, &app.paths.cvelist_cache_dir, keyword)? else {
        println!("尚無 baseline 搜尋索引；請先執行 `cvewatch sync`（首次同步會建立索引）。");
        return Ok(ExitCode::Success);
    };
    if hits.is_empty() {
        println!("找不到符合「{keyword}」的 CVE。");
        return Ok(ExitCode::Success);
    }
    let local = hits.iter().filter(|h| h.local).count();
    println!(
        "共 {} 筆（本機 {local} 筆）{}",
        hits.len(),
        if hits.len() > MAX_SEARCH_RESULTS {
            format!("，顯示前 {MAX_SEARCH_RESULTS} 筆")
        } else {
            String::new()
        }
    );
    for h in hits.iter().take(MAX_SEARCH_RESULTS) {
        cve_view::print_search_hit(h);
    }
    if hits.iter().any(|h| !h.local) {
        println!();
        println!("未在本機的 CVE 表示目錄沒有對應的 vendor/product 寫法；可在 catalog.d 補上 cna 或 keywords 後執行 `cvewatch sync --rescan`。");
    }
    Ok(ExitCode::Success)
}
