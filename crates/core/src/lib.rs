//! cvewatch 核心函式庫：設定、秘密存放、加密資料庫、HTTP 與日誌。
//!
//! 執行檔 `cvewatch` 只負責參數解析與互動提示，所有可測試的邏輯都放在這裡。

pub mod config;
pub mod db;
pub mod error;
pub mod http;
pub mod init;
pub mod logging;
pub mod paths;
pub mod redact;
pub mod secrets;

pub use error::{CoreError, ExitCode};
