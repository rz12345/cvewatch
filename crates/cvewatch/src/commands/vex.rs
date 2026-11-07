//! `cvewatch vex export | import`

use std::path::Path;

use cvewatch_core::db::open_working_db;
use cvewatch_core::notify::triage_notes;
use cvewatch_core::vex::export::ExportMeta;
use cvewatch_core::vex::file::{self, ExportOutcome};
use cvewatch_core::vex::import::{self, ImportOptions};
use cvewatch_core::{CoreError, ExitCode};

use super::{finding, vex_view};
use crate::app::App;
use crate::cli::VexAction;
use crate::prompt;

/// 未加密匯出後的提示。
pub const UNENCRYPTED_NOTICE: &str =
    "注意：此檔未加密，含資產清單與漏洞處理狀態，與資料庫同等敏感；需要時請改用 --encrypt。";

pub fn run(app: &App, action: VexAction) -> Result<ExitCode, CoreError> {
    match action {
        VexAction::Export {
            file,
            asset,
            status,
            priority,
            encrypt,
        } => {
            let filter = finding::filter(&status, &priority, asset, None)?;
            export_file(app, &file, &filter, encrypt)
        }
        VexAction::Import {
            file,
            dry_run,
            force,
        } => import_file(app, &file, ImportOptions { dry_run, force }),
    }
}

fn import_file(app: &App, path: &Path, options: ImportOptions) -> Result<ExitCode, CoreError> {
    // 先讀檔與解析，格式錯誤時不開啟資料庫
    let bom = import::read_file(path, || prompt::secret("輸入匯出檔口令（不會顯示）："))?;
    let name = import::file_name(path);
    let mut conn = open_working_db(&app.paths, &app.store)?;
    let notify = &app.loaded.config.notify;
    let now = chrono::Utc::now();
    // 與 finding set 相同：已有 Redmine 議題的 CVE 在同一交易排入註記
    let report = import::import(&mut conn, &bom, &name, options, now, &|tx, changed| {
        triage_notes::queue_notes_each(tx, notify, changed, now).map(|_| ())
    })?;
    vex_view::print_report(&report);
    Ok(ExitCode::Success)
}

fn export_file(
    app: &App,
    path: &Path,
    filter: &cvewatch_core::matching::query::FindingFilter,
    encrypt: bool,
) -> Result<ExitCode, CoreError> {
    let target = if encrypt {
        file::encrypted_path(path)
    } else {
        path.to_path_buf()
    };
    // 先檢查，避免輸入口令後才發現檔案已存在
    file::ensure_absent(&target)?;
    let passphrase = encrypt
        .then(|| prompt::new_passphrase_for("匯出檔口令"))
        .transpose()?;
    let conn = open_working_db(&app.paths, &app.store)?;
    let ids = cvewatch_core::matching::query::ids(&conn, filter)?;
    let meta = ExportMeta::new(chrono::Utc::now())?;
    let outcome = file::export_to_file(
        &conn,
        &ids,
        &meta,
        path,
        passphrase.as_ref().map(|p| p.as_str()),
    )?;
    print_export(&outcome);
    Ok(ExitCode::Success)
}

fn print_export(o: &ExportOutcome) {
    let how = if o.encrypted {
        "，以口令加密"
    } else {
        ""
    };
    println!(
        "已匯出 {} 筆命中（{} 個元件{how}）：{}",
        o.findings,
        o.components,
        o.path.display()
    );
    if !o.encrypted {
        eprintln!("{UNENCRYPTED_NOTICE}");
    }
}
