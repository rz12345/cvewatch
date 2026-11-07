//! `cvewatch init`

use cvewatch_core::init::{self, InitReport};
use cvewatch_core::{CoreError, ExitCode};

use crate::app::App;

pub fn run(app: &App, config_created: bool) -> Result<ExitCode, CoreError> {
    let report = init::run(app.location(), &app.paths, &app.store, config_created)?;
    print_report(&report);
    Ok(ExitCode::Success)
}

fn created(flag: bool) -> &'static str {
    if flag {
        "新建立"
    } else {
        "已存在，未覆蓋"
    }
}

fn print_report(r: &InitReport) {
    println!(
        "設定檔：{}（{}）",
        r.config_path.display(),
        created(r.config_created)
    );
    println!("資料目錄：{}", r.data_dir.display());
    println!(
        "資料庫：{}（{}，SQLCipher 加密）",
        r.db_path.display(),
        created(r.db_created)
    );
    println!("日誌目錄：{}", r.log_dir.display());
    println!("快取目錄：{}", r.cache_dir.display());

    if r.cloud_sync_warning {
        eprintln!(
            "警告：資料目錄位於 OneDrive 等雲端同步資料夾內。SQLite 的 WAL 檔（-wal、-shm）\
             若被同步可能損壞資料庫，建議在設定檔改用本機路徑的 data_dir。"
        );
    }
    if r.key_created {
        println!();
        println!("已產生新的資料庫金鑰並存入 Windows Credential Manager。");
        println!("請立即執行 `cvewatch db export-key`，把復原金鑰存進密碼管理器。");
    }
    if !r.recovery_key_exported {
        eprintln!("{}", super::EXPORT_REMINDER);
    }
}
