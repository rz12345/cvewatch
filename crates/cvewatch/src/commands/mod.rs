pub mod asset;
mod asset_isms;
mod asset_system;
mod asset_view;
pub mod config;
pub mod cve;
mod cve_view;
pub mod db;
pub mod finding;
mod finding_view;
pub mod init;
pub mod matching;
pub mod notify;
pub mod pending;
pub mod schedule;
pub mod sync;
pub mod tui;
pub mod vex;
mod vex_view;

/// 提醒使用者匯出復原金鑰的共用訊息。
pub const EXPORT_REMINDER: &str =
    "警告：尚未匯出資料庫復原金鑰。請執行 `cvewatch db export-key` 並存入密碼管理器；\
     Windows 重灌或換帳號時，沒有它就打不開資料庫。";
