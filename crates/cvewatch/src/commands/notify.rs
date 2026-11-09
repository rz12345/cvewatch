//! `cvewatch notify urgent | digest | test-email | test-redmine | outbox`

use cvewatch_core::db::open_working_db;
use cvewatch_core::notify::check;
use cvewatch_core::notify::dispatch::{DigestOutcome, Notifier, UrgentQueued};
use cvewatch_core::notify::outbox::{self, FlushStats};
use cvewatch_core::secrets::{SecretKey, SecretStore};
use cvewatch_core::{CoreError, ExitCode};
use rusqlite::Connection;

use crate::app::App;
use crate::cli::NotifyAction;

pub fn run(app: &App, action: NotifyAction) -> Result<ExitCode, CoreError> {
    match action {
        NotifyAction::Urgent => urgent(app),
        NotifyAction::Digest => digest(app),
        NotifyAction::TestEmail => test_email(app),
        NotifyAction::TestRedmine => test_redmine(app),
        NotifyAction::Outbox => list_outbox(app),
    }
}

fn urgent(app: &App) -> Result<ExitCode, CoreError> {
    let mut conn = open_working_db(&app.paths, &app.store)?;
    let notifier = Notifier::from_config(&app.loaded.config, &app.store);
    let now = chrono::Utc::now();
    // 排入後一次送出（含先前留在佇列的項目）
    let (queued, stats) = notifier.after_sync(&mut conn, now)?;
    print_urgent(&queued);
    Ok(report_flush(&stats))
}

/// sync 使用的通知端（秘密只讀取一次，開始與結束共用）。
pub fn notifier(app: &App) -> Notifier {
    Notifier::from_config(&app.loaded.config, &app.store)
}

/// sync 開始時：送出到期的佇列項目。失敗只警告，不影響同步。
pub fn flush_before_sync(notifier: &Notifier, conn: &Connection) {
    match notifier.flush(conn, chrono::Utc::now()) {
        Ok(stats) => {
            report_flush(&stats);
        }
        Err(e) => warn_failure("清送出佇列", &e),
    }
}

/// sync 比對後：排入緊急通知並送出。失敗只警告（已排入的通知下次重送），不影響同步結果。
pub fn notify_after_sync(notifier: &Notifier, conn: &mut Connection) {
    match notifier.after_sync(conn, chrono::Utc::now()) {
        Ok((queued, stats)) => {
            print_urgent(&queued);
            report_flush(&stats);
        }
        Err(e) => warn_failure("緊急通知", &e),
    }
}

fn warn_failure(what: &str, e: &CoreError) {
    tracing::error!(error = %e, "{what}失敗");
    eprintln!("警告：{what}失敗：{e}");
}

fn print_urgent(q: &UrgentQueued) {
    if q.email_events > 0 {
        println!("緊急信：{} 項緊急事件（合併為一封）", q.email_events);
    }
    if q.issues > 0 || q.notes > 0 {
        println!("Redmine：新議題 {}、註記 {}", q.issues, q.notes);
    }
}

/// 印出送出結果；有失敗時回傳結束碼 4（通知仍在佇列中，下次重送）。
fn report_flush(stats: &FlushStats) -> ExitCode {
    if stats.sent > 0 {
        println!("已送出 {} 則通知", stats.sent);
    }
    for e in &stats.errors {
        eprintln!("通知送出失敗：{e}");
    }
    if stats.retrying + stats.postponed + stats.blocked > 0 {
        eprintln!(
            "{} 則通知留在送出佇列，下次 sync 或摘要時重送",
            stats.retrying + stats.postponed + stats.blocked
        );
    }
    if stats.dead > 0 {
        eprintln!(
            "{} 則通知無法送出（已標為 dead），以 `cvewatch notify outbox` 檢視",
            stats.dead
        );
    }
    if stats.errors.is_empty() {
        ExitCode::Success
    } else {
        ExitCode::PartialFailure
    }
}

fn digest(app: &App) -> Result<ExitCode, CoreError> {
    let mut conn = open_working_db(&app.paths, &app.store)?;
    let window = app.loaded.config.lifecycle.reminder_days;
    let mut notifier = Notifier::from_config(&app.loaded.config, &app.store);
    // 目錄有錯時摘要照寄（命中、等待區與同步狀態與目錄無關），只略過生命週期段落
    match app.load_catalog() {
        Ok(catalog) => notifier = notifier.with_lifecycle(catalog, window),
        Err(e) => {
            tracing::warn!(error = %e, "產品目錄載入失敗，每日摘要略過生命週期提醒");
            eprintln!("警告：產品目錄載入失敗，本次摘要不含生命週期提醒：{e}");
        }
    }
    let now = chrono::Utc::now();
    // 先產生摘要（取代尚未寄出的舊摘要）再一次送出整個佇列，避免舊摘要先寄出而收到兩封
    match notifier.queue_digest(&mut conn, now)? {
        DigestOutcome::EmailDisabled => {
            println!("Email 通知未啟用（[notify.email] enabled = false），不寄送摘要。");
        }
        DigestOutcome::SkippedEmpty => {
            println!("摘要期間沒有新事項，且 send_empty = false，不寄送。");
        }
        DigestOutcome::InFlight => {
            println!("另一個程序正在寄出先前的摘要，本次不另產生摘要。");
        }
        DigestOutcome::Queued { empty, replaced } => {
            if replaced > 0 {
                println!("以新摘要取代 {replaced} 封尚未寄出的舊摘要。");
            }
            println!(
                "已產生每日摘要{}。",
                if empty { "（無新事項）" } else { "" }
            );
        }
    }
    Ok(report_flush(&notifier.flush(&conn, now)?))
}

fn secret(app: &App, key: SecretKey) -> Result<Option<zeroize::Zeroizing<String>>, CoreError> {
    Ok(app.store.get(key)?)
}

fn test_email(app: &App) -> Result<ExitCode, CoreError> {
    let cfg = &app.loaded.config;
    let password = secret(app, SecretKey::SmtpPassword)?;
    let message = check::send_test_email(&cfg.notify, password, &cfg.http)
        .map_err(|e| CoreError::Invalid(format!("測試信寄送失敗：{e}")))?;
    println!("{message}");
    Ok(ExitCode::Success)
}

fn test_redmine(app: &App) -> Result<ExitCode, CoreError> {
    let cfg = &app.loaded.config;
    let key = secret(app, SecretKey::RedmineApiKey)?;
    let report = check::test_redmine(&cfg.notify, key, &cfg.http)
        .map_err(|e| CoreError::Invalid(format!("Redmine 測試失敗：{e}")))?;
    println!("{}", check::describe_report(&report));
    Ok(ExitCode::Success)
}

fn list_outbox(app: &App) -> Result<ExitCode, CoreError> {
    let conn = open_working_db(&app.paths, &app.store)?;
    let items = outbox::list(&conn)?;
    if items.is_empty() {
        println!("送出佇列是空的。");
        return Ok(ExitCode::Success);
    }
    println!("編號  管道     種類          建立時間             次數  狀態");
    for i in items {
        let state = match &i.dead_at {
            Some(at) => format!("無法送出（{}）", short(at)),
            None => format!("下次 {}", short(&i.next_attempt_at)),
        };
        println!(
            "{:<5} {:<8} {:<13} {:<20} {:>4}  {state}",
            i.id,
            i.channel.label(),
            i.kind,
            short(&i.created_at),
            i.attempts
        );
        if let Some(e) = &i.last_error {
            println!("      錯誤：{e}");
        }
    }
    Ok(ExitCode::Success)
}

/// 本機時間 `YYYY-MM-DD HH:MM`。
fn short(raw: &str) -> String {
    cvewatch_core::notify::content::Zone::Local.time(raw)
}
