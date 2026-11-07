//! 統一錯誤型別與結束碼。

use std::path::PathBuf;

use crate::config::ConfigError;
use crate::http::HttpError;
use crate::secrets::SecretError;

/// 程式結束碼。工作排程器依此判斷是否需要通知。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// 成功
    Success = 0,
    /// 一般錯誤
    GeneralError = 1,
    /// 設定錯誤
    ConfigError = 2,
    /// 另一個 sync 正在執行
    SyncLocked = 3,
    /// sync 完成但部分來源失敗
    PartialFailure = 4,
    /// 資料庫金鑰遺失或錯誤
    DbKeyError = 5,
}

impl ExitCode {
    pub fn code(self) -> u8 {
        self as u8
    }
}

impl From<ExitCode> for std::process::ExitCode {
    fn from(code: ExitCode) -> Self {
        std::process::ExitCode::from(code.code())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("憑證庫中沒有資料庫金鑰；請執行 `cvewatch db import-key` 輸入復原金鑰")]
    DbKeyMissing,

    #[error("資料庫金鑰錯誤，無法開啟資料庫（檔案未被修改）")]
    DbKeyInvalid,

    #[error("無法以提供的金鑰或口令開啟備份檔（檔案未被修改）")]
    BackupKeyInvalid,

    #[error("資料庫尚未建立：{0}；請先執行 `cvewatch init`")]
    DbNotFound(PathBuf),

    #[error("資料庫 schema 版本 {found} 高於程式支援的 {supported}，請升級 cvewatch")]
    DbTooNew { found: u32, supported: u32 },

    #[error("migration {version}（{name}）失敗，已回復整批變更：{source}")]
    Migration {
        version: u32,
        name: &'static str,
        #[source]
        source: rusqlite::Error,
    },

    #[error("另一個 sync 正在執行（PID {pid}，開始於 {since}）")]
    SyncLocked { pid: u32, since: String },

    #[error(transparent)]
    Secret(#[from] SecretError),

    #[error("資料庫錯誤：{0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("檔案操作失敗：{path}：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Http(#[from] HttpError),

    #[error("{0}")]
    Invalid(String),

    #[error("`{0}` 尚未實作（not implemented）")]
    NotImplemented(&'static str),
}

impl CoreError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        CoreError::Io {
            path: path.into(),
            source,
        }
    }

    pub fn exit_code(&self) -> ExitCode {
        match self {
            CoreError::Config(_) => ExitCode::ConfigError,
            CoreError::SyncLocked { .. } => ExitCode::SyncLocked,
            CoreError::DbKeyMissing | CoreError::DbKeyInvalid | CoreError::BackupKeyInvalid => {
                ExitCode::DbKeyError
            }
            _ => ExitCode::GeneralError,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_values_are_stable() {
        let codes = [
            ExitCode::Success,
            ExitCode::GeneralError,
            ExitCode::ConfigError,
            ExitCode::SyncLocked,
            ExitCode::PartialFailure,
            ExitCode::DbKeyError,
        ];
        let values: Vec<u8> = codes.iter().map(|c| c.code()).collect();
        assert_eq!(values, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn errors_map_to_exit_codes() {
        assert_eq!(CoreError::DbKeyMissing.exit_code(), ExitCode::DbKeyError);
        assert_eq!(CoreError::DbKeyInvalid.exit_code(), ExitCode::DbKeyError);
        assert_eq!(
            CoreError::SyncLocked {
                pid: 1,
                since: "x".into()
            }
            .exit_code(),
            ExitCode::SyncLocked
        );
        assert_eq!(
            CoreError::Config(ConfigError::NoHomeDir).exit_code(),
            ExitCode::ConfigError
        );
        assert_eq!(
            CoreError::NotImplemented("sync").exit_code(),
            ExitCode::GeneralError
        );
    }
}
