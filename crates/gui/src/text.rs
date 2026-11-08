//! 顯示用文字：時間格式（與 TUI 共用 `cvewatch_frontend::display`）、外部資料的控制字元處理、
//! `Report` 的下一步提示。

pub use cvewatch_frontend::display::{full_time, one_line, short_time};
use cvewatch_frontend::{Hint, Report};

/// 以 GUI 的操作方式說明下一步。
pub fn hint_text(hint: Hint) -> &'static str {
    match hint {
        Hint::StaleAfterWrite | Hint::Reload => "請按「重新載入」（F5）",
        Hint::RevealSelection => "請放寬搜尋或篩選，或按「清除選取」",
    }
}

/// 狀態列文字：訊息與下一步提示。
pub fn report_text(r: &Report) -> String {
    match r.hint {
        Some(h) => format!("{}；{}", r.message, hint_text(h)),
        None => r.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_is_appended() {
        let r = Report::warning("x").with_hint(Hint::Reload);
        assert_eq!(report_text(&r), "x；請按「重新載入」（F5）");
    }
}
