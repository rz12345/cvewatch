//! `cvewatch match`；`sync` 與 `asset import` / `import-packages` 結束後也會自動比對。

use cvewatch_core::catalog::Catalog;
use cvewatch_core::db::open_working_db;
use cvewatch_core::matching::run::{rematch_state, run_match, MatchReport, MatchSettings};
use cvewatch_core::{CoreError, ExitCode};
use rusqlite::Connection;

use crate::app::App;

pub fn run(app: &App) -> Result<ExitCode, CoreError> {
    let catalog = app.load_catalog()?;
    let mut conn = open_working_db(&app.paths, &app.store)?;
    rematch(app, &mut conn, &catalog)?;
    Ok(ExitCode::Success)
}

/// 是否需要全量比對：有尚未比對的寫入，或產品目錄／評分設定自上次全量比對後已變更。
pub fn rematch_needed(app: &App, conn: &Connection, catalog: &Catalog) -> Result<bool, CoreError> {
    let settings = MatchSettings::from_config(&app.loaded.config);
    Ok(rematch_state(conn, catalog, &settings)?.needed())
}

/// 資料已寫入後：需要時執行全量比對。不需要比對時回傳 None（由呼叫端說明原因）；
/// 檢查本身失敗時與比對失敗相同處理（已完成的寫入不回報為失敗，結束碼 4）。
pub fn rematch_if_needed(app: &App, conn: &mut Connection, catalog: &Catalog) -> Option<ExitCode> {
    match rematch_needed(app, conn, catalog) {
        Ok(true) => Some(rematch_after_write(app, conn, catalog)),
        Ok(false) => None,
        Err(e) => Some(report_failure(&e)),
    }
}

/// 資料已寫入後的自動比對（sync、資產匯入結束時）。失敗時不讓已完成的寫入被回報為失敗：
/// 印出警告與日誌，並回傳應使用的結束碼（成功 0、比對失敗 4）。「比對待補」已在寫入交易中記錄，
/// 比對成功時清除、失敗時保留。
pub fn rematch_after_write(app: &App, conn: &mut Connection, catalog: &Catalog) -> ExitCode {
    match rematch(app, conn, catalog) {
        Ok(_) => ExitCode::Success,
        Err(e) => report_failure(&e),
    }
}

/// 比對交易失敗時已回復，寫入時記錄的「比對待補」仍在。
fn report_failure(e: &CoreError) -> ExitCode {
    tracing::error!(error = %e, "資料已寫入，但自動比對失敗");
    eprintln!("警告：資料已寫入，但自動比對失敗：{e}");
    eprintln!("      請稍後執行 `cvewatch match` 重新比對（結束碼 4）。");
    ExitCode::PartialFailure
}

/// 全量重算並輸出摘要。
pub fn rematch(
    app: &App,
    conn: &mut Connection,
    catalog: &Catalog,
) -> Result<MatchReport, CoreError> {
    let settings = MatchSettings::from_config(&app.loaded.config);
    let report = run_match(conn, catalog, &settings, chrono::Utc::now())?;
    print_report(&report);
    Ok(report)
}

fn print_report(r: &MatchReport) {
    let f = &r.findings;
    let p = &r.pending;
    println!("比對完成：使用中元件 {}、本機 CVE {}", r.components, r.cves);
    println!(
        "  新命中 {}、自動轉為已處理 {}、重新開啟 {}、CVE 撤銷而忽略 {}、已判定不相關而忽略 {}、\
         已處理但仍受影響 {}、來源矛盾 {}",
        f.new, f.resolved, f.reopened, f.rejected, f.dismissed, f.still_vulnerable, f.conflicts
    );
    println!(
        "  等待區 {} 筆（本次新進 {}、移出 {}、逾期 {}）",
        p.active, p.entered, p.resolved, p.stale
    );
    print_skipped(r);
    if f.conflicts > 0 {
        println!("  來源矛盾可用 `cvewatch finding conflicts` 檢視。");
    }
}

/// 本次未比對的元件（命中維持不變）。
fn print_skipped(r: &MatchReport) {
    if r.skipped_components > 0 {
        println!(
            "  警告：{} 個元件的產品已不在目錄中（{}），本次未比對、其命中維持不變；請檢查 catalog.d",
            r.skipped_components,
            r.skipped_products.join("、")
        );
    }
    if r.unreadable_cves > 0 {
        println!(
            "  警告：{} 個 CVE 的 affected 資料無法讀取，本次未比對、其命中與等待區維持不變；請重新同步（詳見記錄）",
            r.unreadable_cves
        );
    }
    if r.awaiting_osv > 0 && r.osv_enabled {
        println!(
            "  {} 個套件的版本已變更，待下次 `cvewatch sync` 以 OSV 重查後再比對",
            r.awaiting_osv
        );
    } else if r.awaiting_osv > 0 {
        println!(
            "  {} 個套件未比對：OSV 來源已停用（[sources] osv_enabled = false），套件元件無從判定",
            r.awaiting_osv
        );
    }
}
