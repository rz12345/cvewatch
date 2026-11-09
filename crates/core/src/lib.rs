//! cvewatch 核心函式庫：設定、秘密存放、加密資料庫、HTTP 與日誌。
//!
//! 執行檔 `cvewatch` 只負責參數解析與互動提示，所有可測試的邏輯都放在這裡。

#[macro_use]
mod string_enum;

pub mod assets;
pub mod catalog;
pub mod config;
mod config_example;
pub mod cpe;
pub mod db;
pub mod error;
pub mod files;
mod hash;
pub mod http;
pub mod ingest;
pub mod init;
pub mod lifecycle;
pub mod logging;
pub mod matching;
pub mod notify;
pub mod paths;
pub mod process;
pub mod redact;
pub mod runtime;
pub mod schedule;
pub mod secrets;
pub mod version_text;
pub mod vex;

pub use error::{CoreError, ExitCode};
