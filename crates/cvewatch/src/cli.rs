//! 命令列定義（clap derive）。

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "cvewatch",
    version,
    about = "資產 × CVE 比對工具",
    long_about = "資產 × CVE 比對工具：匯入資產清冊、同步 CVE、規則比對、風險排序與通知。\n\n\
                  結束碼：0 成功、1 一般錯誤、2 設定錯誤、3 另一個 sync 正在執行、\
                  4 部分來源失敗、5 資料庫金鑰遺失或錯誤。"
)]
pub struct Cli {
    /// 設定檔路徑（優先於環境變數 CVEWATCH_CONFIG）
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// 建立資料目錄、範例設定檔、加密資料庫與金鑰（可重複執行，不覆蓋既有檔案）
    Init,
    /// 同步 CVE 資料來源（尚未實作）
    Sync,
    /// 開啟文字介面（尚未實作）
    Tui,
    /// 管理資產清冊（尚未實作）
    Asset,
    /// 設定與秘密
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// 資料庫金鑰、備份與還原
    Db {
        #[command(subcommand)]
        action: DbAction,
    },
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Command::Init => "init",
            Command::Sync => "sync",
            Command::Tui => "tui",
            Command::Asset => "asset",
            Command::Config { .. } => "config",
            Command::Db { .. } => "db",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// 將秘密存入 Windows Credential Manager（輸入不回顯）
    SetSecret {
        /// 秘密鍵
        #[arg(value_parser = ["nvd_api_key", "smtp_password", "redmine_api_key"])]
        key: String,
    },
    /// 檢查設定、秘密是否已設定、資料庫與復原金鑰狀態
    Check,
}

#[derive(Debug, Subcommand)]
pub enum DbAction {
    /// 顯示資料庫復原金鑰（需輸入 YES 確認；不寫入任何檔案）
    ExportKey,
    /// 輸入復原金鑰並寫回憑證庫（先驗證能開啟資料庫）
    ImportKey,
    /// 以新金鑰重新加密資料庫（舊復原金鑰隨即失效）
    Rekey,
    /// 產生加密備份（預設沿用資料庫金鑰）
    Backup {
        /// 備份檔路徑（不可已存在）
        path: PathBuf,
        /// 改以口令加密，可在其他電腦還原
        #[arg(long, conflicts_with = "plain")]
        passphrase: bool,
        /// 產生未加密備份（需輸入 PLAIN 確認）
        #[arg(long)]
        plain: bool,
    },
    /// 從備份還原（原資料庫保留為 cvewatch.db.before-restore）
    Restore {
        /// 備份檔路徑
        file: PathBuf,
        /// 備份以口令加密
        #[arg(long, conflicts_with = "recovery_key")]
        passphrase: bool,
        /// 備份以舊的資料庫金鑰加密（rekey 之前或其他電腦）：輸入當時的復原金鑰
        #[arg(long)]
        recovery_key: bool,
    },
}
