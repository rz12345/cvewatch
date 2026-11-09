//! `cvewatch asset isms-draft <workbook> <output>`（17 design D7）：由 ISMS 資訊資產清冊產生
//! cvewatch 清冊草稿。只讀取資料庫（沿用既有資產），不寫入。
//!
//! 結束碼：草稿寫出時為 0（即使有略過的列）；目的檔已存在、檔案無法開啟、表頭錯誤時為非 0。

use std::path::Path;

use cvewatch_core::assets::isms::{generate_draft, DraftContext, DraftReport};
use cvewatch_core::assets::template::ensure_absent;
use cvewatch_core::db::open_working_db;
use cvewatch_core::{CoreError, ExitCode};

use super::asset::UNENCRYPTED_NOTICE;
use crate::app::App;

pub fn isms_draft(app: &App, workbook: &Path, output: &Path) -> Result<ExitCode, CoreError> {
    // 先檢查目的檔，避免開啟資料庫（要求金鑰）或讀取清冊後才失敗
    ensure_absent(output)?;
    let catalog = app.load_catalog()?;
    let classes = app.loaded.config.asset_classes()?;
    let report = match open_working_db(&app.paths, &app.store) {
        Ok(conn) => {
            let ctx = DraftContext::load(&conn, &classes, &catalog)?;
            generate_draft(workbook, output, &ctx)?
        }
        // 尚未初始化：沒有既有資產可沿用，草稿照樣可以產生
        Err(CoreError::DbNotFound(path)) => {
            println!(
                "找不到資料庫（{}），草稿不參考既有資產；匯入前請先執行 `cvewatch init`。",
                path.display()
            );
            generate_draft(workbook, output, &DraftContext::empty(&classes, &catalog))?
        }
        Err(e) => return Err(e),
    };
    print_report(output, &report);
    Ok(ExitCode::Success)
}

fn print_report(output: &Path, report: &DraftReport) {
    println!("已產生 ISMS 草稿：{}", output.display());
    for line in report.cli_lines() {
        println!("{line}");
    }
    println!("{UNENCRYPTED_NOTICE}");
}
