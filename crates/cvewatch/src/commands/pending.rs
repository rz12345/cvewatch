//! `cvewatch pending list [--stale]`、`cvewatch pending resolve <cve> --apply <asset_tag>|--dismiss`

use cvewatch_core::db::open_working_db;
use cvewatch_core::matching::pending_ops::{self, Action};
use cvewatch_core::{CoreError, ExitCode};

use super::finding_view;
use crate::app::App;
use crate::cli::PendingAction;

pub fn run(app: &App, action: PendingAction) -> Result<ExitCode, CoreError> {
    match action {
        PendingAction::List { stale } => {
            let conn = open_working_db(&app.paths, &app.store)?;
            let stale_days = app.loaded.config.pending.stale_days;
            let rows = pending_ops::list(&conn, stale, stale_days, chrono::Utc::now())?;
            finding_view::print_pending(&rows, stale);
            Ok(ExitCode::Success)
        }
        PendingAction::Resolve {
            cve,
            apply,
            dismiss: _,
            note,
        } => {
            // clap 已保證 --apply 與 --dismiss 恰好其一：沒有 --apply 即為 --dismiss
            let action = apply.map_or(Action::Dismiss, Action::Apply);
            resolve(app, &cve, &action, &note)
        }
    }
}

fn resolve(app: &App, cve: &str, action: &Action, note: &str) -> Result<ExitCode, CoreError> {
    let catalog = app.load_catalog()?;
    let mut conn = open_working_db(&app.paths, &app.store)?;
    let resolution = pending_ops::resolve(
        &mut conn,
        &catalog,
        &app.loaded.config.scoring,
        cve,
        action,
        note,
        chrono::Utc::now(),
    )?;
    match action {
        Action::Dismiss => {
            println!("{} 已移出等待區（人工判定不相關）。", resolution.cve_id);
            if !resolution.ignored.is_empty() {
                let ids: Vec<String> = resolution
                    .ignored
                    .iter()
                    .map(|id| format!("#{id}"))
                    .collect();
                println!(
                    "  一併忽略 probable 命中（false_positive）：{}",
                    ids.join("、")
                );
            }
        }
        Action::Apply(target) => {
            println!("{} 已移出等待區並套用到 {target}：", resolution.cve_id);
            for a in &resolution.applied {
                let state = match (a.created, a.reopened) {
                    (true, _) => "建立 probable 命中",
                    (false, true) => "既有命中已重新開啟",
                    (false, false) => "既有命中維持未處理",
                };
                println!("  #{} {}（{state}）", a.finding_id, a.product_key);
            }
        }
    }
    Ok(ExitCode::Success)
}
