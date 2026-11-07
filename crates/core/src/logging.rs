//! 日誌：`<data_dir>\logs\cvewatch.YYYY-MM-DD.log` 每日輪替，啟動時刪除過期檔。
//!
//! 日誌只記錄筆數、CVE ID、asset_tag 與錯誤類型；不得記錄元件版本、命中明細、
//! 匯入檔原始內容或 HTTP 回應本文（見 `tests/log_lint.rs`）。

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;

use crate::config::LogLevel;
use crate::error::CoreError;

pub const LOG_FILE_PREFIX: &str = "cvewatch";
pub const LOG_FILE_SUFFIX: &str = "log";
/// 覆寫日誌過濾條件的環境變數（tracing `EnvFilter` 語法）。
pub const LOG_FILTER_ENV: &str = "CVEWATCH_LOG";
const LOG_DATE_FORMAT: &str = "%Y-%m-%d";

/// 持有期間背景寫入執行緒保持運作；drop 時會把緩衝寫完。
pub struct LogGuard {
    _guard: WorkerGuard,
}

/// 第三方套件固定在 warn，避免 debug 層級帶出請求或回應內容。
pub fn default_filter(level: LogLevel) -> String {
    let lvl = level.as_str();
    format!("warn,cvewatch={lvl},cvewatch_core={lvl},cvewatch_tui={lvl}")
}

pub fn init(log_dir: &Path, level: LogLevel) -> Result<LogGuard, CoreError> {
    std::fs::create_dir_all(log_dir).map_err(|e| CoreError::io(log_dir, e))?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix(LOG_FILE_SUFFIX)
        .build(log_dir)
        .map_err(|e| CoreError::Invalid(format!("無法建立日誌檔：{e}")))?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env(LOG_FILTER_ENV)
        .unwrap_or_else(|_| EnvFilter::new(default_filter(level)));
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(false)
        .with_env_filter(filter)
        .try_init()
        .map_err(|e| CoreError::Invalid(format!("無法初始化日誌：{e}")))?;
    Ok(LogGuard { _guard: guard })
}

/// 由檔名 `cvewatch.YYYY-MM-DD.log` 取出日期。
pub fn log_file_date(file_name: &str) -> Option<NaiveDate> {
    let date = file_name
        .strip_prefix(LOG_FILE_PREFIX)?
        .strip_prefix('.')?
        .strip_suffix(LOG_FILE_SUFFIX)?
        .strip_suffix('.')?;
    NaiveDate::parse_from_str(date, LOG_DATE_FORMAT).ok()
}

/// 刪除早於 `today - retention_days` 的日誌檔，回傳被刪除的路徑。
pub fn cleanup_old_logs(
    log_dir: &Path,
    retention_days: u32,
    today: NaiveDate,
) -> Result<Vec<PathBuf>, CoreError> {
    if !log_dir.exists() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(log_dir).map_err(|e| CoreError::io(log_dir, e))?;
    let expired: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(log_file_date)
                .is_some_and(|date| (today - date).num_days() > i64::from(retention_days))
        })
        .collect();
    for path in &expired {
        std::fs::remove_file(path).map_err(|e| CoreError::io(path, e))?;
    }
    Ok(expired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_log_file_dates() {
        assert_eq!(
            log_file_date("cvewatch.2026-11-07.log"),
            NaiveDate::from_ymd_opt(2026, 11, 7)
        );
        assert_eq!(log_file_date("cvewatch.log"), None);
        assert_eq!(log_file_date("other.2026-11-07.log"), None);
        assert_eq!(log_file_date("cvewatch.2026-13-01.log"), None);
    }

    #[test]
    fn default_filter_pins_dependencies_to_warn() {
        assert_eq!(
            default_filter(LogLevel::Debug),
            "warn,cvewatch=debug,cvewatch_core=debug,cvewatch_tui=debug"
        );
    }
}
