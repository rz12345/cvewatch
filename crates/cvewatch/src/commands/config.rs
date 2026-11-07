//! `cvewatch config set-secret` / `config check`

use cvewatch_core::db::keyops::key_exported_at;
use cvewatch_core::db::migrate::current_version;
use cvewatch_core::db::open_working_db;
use cvewatch_core::paths::ConfigSource;
use cvewatch_core::secrets::{SecretKey, SecretStore};
use cvewatch_core::{CoreError, ExitCode};

use crate::app::App;
use crate::prompt;

pub fn set_secret(app: &App, name: &str) -> Result<ExitCode, CoreError> {
    let key = SecretKey::parse_user_settable(name)?;
    let value = prompt::secret(&format!("輸入 {key}（不會顯示）："))?;
    app.store.set(key, value.trim())?;
    tracing::info!(secret = key.as_str(), "秘密已更新");
    println!("已存入 Windows Credential Manager：{key}");
    Ok(ExitCode::Success)
}

pub fn check(app: &App) -> Result<ExitCode, CoreError> {
    print_config(app);
    println!();
    print_secrets(app);
    println!();
    print_notify(app);
    println!();
    print_database(app);
    println!();
    println!(
        "注意：秘密綁定目前的 Windows 帳號（{}）；工作排程必須以同一帳號執行。",
        std::env::var("USERNAME").unwrap_or_else(|_| "?".to_string())
    );
    Ok(ExitCode::Success)
}

fn print_config(app: &App) {
    let loc = app.location();
    let source = match loc.source {
        ConfigSource::CommandLine => "--config",
        ConfigSource::Environment => "CVEWATCH_CONFIG",
        ConfigSource::Default => "預設位置",
    };
    let note = if app.loaded.used_defaults {
        "（檔案不存在，使用內建預設值）"
    } else {
        ""
    };
    println!("設定檔：{}（{source}）{note}", loc.path.display());
    println!("資料目錄：{}", app.paths.data_dir.display());
    if !app.loaded.unknown_keys.is_empty() {
        println!("未知欄位：{}", app.loaded.unknown_keys.join(", "));
    }
}

fn print_secrets(app: &App) {
    println!("秘密（Windows Credential Manager）：");
    for key in SecretKey::USER_SETTABLE {
        let status = match app.store.is_set(key) {
            Ok(true) => "已設定".to_string(),
            Ok(false) => "未設定".to_string(),
            Err(e) => format!("無法讀取（{e}）"),
        };
        println!("  {key}：{status}");
    }
}

/// 通知設定是否可用；明文傳輸例外列為警告。
fn print_notify(app: &App) {
    let n = &app.loaded.config.notify;
    println!("通知：");
    let state = |enabled: bool, ready: Result<(), String>| match (enabled, ready) {
        (false, _) => "未啟用".to_string(),
        (true, Ok(())) => "已啟用".to_string(),
        (true, Err(e)) => format!("已啟用，但{e}"),
    };
    println!("  Email：{}", state(n.email.enabled, n.email.ready()));
    println!("  Redmine：{}", state(n.redmine.enabled, n.redmine.ready()));
    println!("  緊急門檻：KEV 或 CVSS ≥ {:.1}", n.urgent_cvss);
    let notifier =
        cvewatch_core::notify::dispatch::Notifier::from_config(&app.loaded.config, &app.store);
    for (channel, problem) in notifier.problems() {
        eprintln!("警告：{channel} 已啟用但無法送出：{problem}");
    }
    if let Some(ca) = n.ca_file() {
        let note = if ca.exists() {
            ""
        } else {
            "（檔案不存在）"
        };
        println!("  自訂 CA：{}{note}", ca.display());
    }
    if n.allow_insecure_transport {
        eprintln!(
            "警告：已啟用明文傳輸例外（allow_insecure_transport = true）；SMTP 密碼與 Redmine API key 仍不會以明文送出"
        );
    }
}

fn print_database(app: &App) {
    println!("資料庫：{}", app.paths.db_path.display());
    let conn = match open_working_db(&app.paths, &app.store) {
        Ok(c) => c,
        Err(e) => {
            println!("  狀態：無法開啟（{e}）");
            return;
        }
    };
    match current_version(&conn) {
        Ok(v) => println!("  狀態：正常（已加密，schema 版本 {v}）"),
        Err(e) => println!("  狀態：無法讀取版本（{e}）"),
    }
    match key_exported_at(&conn) {
        Ok(Some(at)) => println!("  復原金鑰最後匯出：{at}"),
        Ok(None) => eprintln!("{}", super::EXPORT_REMINDER),
        Err(e) => println!("  無法讀取復原金鑰狀態（{e}）"),
    }
}
