//! cvewatch 執行檔：參數解析、互動提示，邏輯在 `cvewatch-core`。

mod app;
mod cli;
mod commands;
mod progress;
mod prompt;

use clap::Parser;
use cvewatch_core::init::ensure_config_file;
use cvewatch_core::{CoreError, ExitCode};

use crate::cli::{Cli, Command, ConfigAction, DbAction};

fn main() -> std::process::ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();
            // clap 預設以 2 結束，但 2 保留給設定錯誤；參數錯誤一律視為一般錯誤。
            let code = if err.use_stderr() {
                ExitCode::GeneralError
            } else {
                ExitCode::Success
            };
            return code.into();
        }
    };
    match run(cli) {
        Ok(code) => code.into(),
        Err(err) => {
            eprintln!("錯誤：{err}");
            err.exit_code().into()
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, CoreError> {
    let location = app::resolve_location(cli.config)?;
    let config_created = match cli.command {
        Command::Init => ensure_config_file(&location)?,
        _ => false,
    };
    let app = app::start(&location, cli.command.name())?;
    // 在 App（含日誌 guard）仍存活時記錄錯誤，確保寫入日誌檔。
    dispatch(&app, cli.command, config_created).inspect_err(|err| {
        tracing::error!(error = %err, exit_code = err.exit_code().code(), "命令失敗");
    })
}

fn dispatch(app: &app::App, command: Command, config_created: bool) -> Result<ExitCode, CoreError> {
    match command {
        Command::Init => commands::init::run(app, config_created),
        Command::Sync(args) => commands::sync::run(app, args),
        Command::Cve { action } => commands::cve::run(app, action),
        Command::Match => commands::matching::run(app),
        Command::Finding { action } => commands::finding::run(app, action),
        Command::Pending { action } => commands::pending::run(app, action),
        Command::Tui => Err(CoreError::NotImplemented("tui")),
        Command::Asset { action } => commands::asset::run(app, action),
        Command::Config { action } => match action {
            ConfigAction::SetSecret { key } => commands::config::set_secret(app, &key),
            ConfigAction::Check => commands::config::check(app),
        },
        Command::Db { action } => run_db(app, action),
    }
}

fn run_db(app: &app::App, action: DbAction) -> Result<ExitCode, CoreError> {
    match action {
        DbAction::ExportKey => commands::db::export_key(app),
        DbAction::ImportKey => commands::db::import_key(app),
        DbAction::Rekey => commands::db::rekey(app),
        DbAction::Backup {
            path,
            passphrase,
            plain,
        } => commands::db::backup(app, &path, passphrase, plain),
        DbAction::Restore {
            file,
            passphrase,
            recovery_key,
        } => commands::db::restore(app, &file, passphrase, recovery_key),
    }
}
