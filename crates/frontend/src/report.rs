//! 操作報告：有副作用的操作不直接改寫畫面，回傳提示文字與等級，由各前端自行顯示
//! （TUI 顯示在狀態列，GUI 顯示為通知）。
//!
//! 文字不含任何前端的操作方式（按鍵、按鈕名稱）；需要告訴使用者下一步時附上 [`Hint`]，
//! 由各前端以自己的操作方式說明。

use cvewatch_core::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

/// 使用者可採取的下一步（由前端轉成按鍵或按鈕說明）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    /// 寫入成功但重新載入失敗：畫面可能過時，請重新載入
    StaleAfterWrite,
    /// 資料已改變（例如命中已不存在）：請重新載入
    Reload,
    /// 所選命中都未顯示：放寬搜尋／篩選，或清除多選
    RevealSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "操作結果需要顯示給使用者"]
pub struct Report {
    pub message: String,
    pub level: Level,
    pub hint: Option<Hint>,
}

impl Report {
    fn new(message: impl Into<String>, level: Level) -> Report {
        Report {
            message: message.into(),
            level,
            hint: None,
        }
    }

    pub fn info(message: impl Into<String>) -> Report {
        Report::new(message, Level::Info)
    }

    pub fn warning(message: impl Into<String>) -> Report {
        Report::new(message, Level::Warning)
    }

    pub fn error(message: impl Into<String>) -> Report {
        Report::new(message, Level::Error)
    }

    /// 「{what}：{e}」，並寫入日誌。
    pub fn failure(what: &str, e: &CoreError) -> Report {
        tracing::warn!(error = %e, "{what}");
        Report::error(format!("{what}：{e}"))
    }

    pub fn with_hint(self, hint: Hint) -> Report {
        Report {
            hint: Some(hint),
            ..self
        }
    }
}
