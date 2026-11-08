//! 資產編修後的背景比對（design D10）：寫入在行程內完成，比對以 companion `cvewatch match`
//! 在背景執行緒執行（不凍結視窗），完成後重新載入並在狀態列顯示結果。
//!
//! 比對進行中再次編修時不排第二個比對：結束後若「比對待補」仍成立再跑一次（單一旗標）。
//! 找不到 companion 或比對失敗時，編修保留，以警告提示執行 `cvewatch match`。

use std::sync::mpsc::{self, Receiver, TryRecvError};

use cvewatch_core::matching::run::match_is_stale;
use cvewatch_frontend::workspace::Refreshed;
use cvewatch_frontend::{Level, Report};

use crate::app::GuiApp;
use crate::dialogs::import::{output_text, RunResult};

/// 找不到 companion 時的提示。
pub const NO_MATCH: &str =
    "找不到同資料夾的 cvewatch.exe，尚未比對；請執行 `cvewatch match`（下次匯入或同步也會補跑）";

/// 比對中的狀態列文字。
pub const MATCHING: &str = "比對中…";

/// 背景比對的狀態。
#[derive(Default)]
pub struct Matching {
    rx: Option<Receiver<RunResult>>,
    /// 比對進行中又有編修：結束後再確認一次
    again: bool,
}

impl Matching {
    pub fn running(&self) -> bool {
        self.rx.is_some()
    }
}

/// 比對結果的摘要（`cvewatch match` 輸出的前兩行）。
fn summary(stdout: &str) -> String {
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(2)
        .collect();
    if lines.is_empty() {
        "比對完成".into()
    } else {
        lines.join("；")
    }
}

impl GuiApp {
    /// 資產編修的結果：顯示並更新詳情；有寫入時在背景比對。
    pub fn after_asset_write(&mut self, r: Refreshed) {
        let needs = r.needs_match;
        self.apply_refreshed(r);
        if needs {
            self.start_match();
        }
    }

    /// 在狀態列目前的訊息後附加文字（保留寫入結果，較嚴重的等級優先）。
    fn append_status(&mut self, text: &str, level: Level) {
        let r = match self.status.take() {
            Some(prev) => Report {
                message: format!("{}；{text}", prev.message),
                level: worse(prev.level, level),
                hint: prev.hint,
            },
            None => match level {
                Level::Info => Report::info(text),
                Level::Warning => Report::warning(text),
                Level::Error => Report::error(text),
            },
        };
        self.report(r);
    }

    /// 比對結束：把狀態列結尾的「比對中…」換成結果，保留前面寫入的結果。
    fn finish_status(&mut self, result: Report) {
        let prefix = self.status.as_ref().and_then(|r| {
            r.message
                .strip_suffix(MATCHING)
                .map(|p| (p.trim_end_matches('；').to_string(), r.level))
        });
        let r = match prefix {
            Some((p, level)) if !p.is_empty() => Report {
                message: format!("{p}；{}", result.message),
                level: worse(level, result.level),
                hint: None,
            },
            _ => result,
        };
        self.report(r);
    }

    /// 開始背景比對；進行中時只記下結束後要再確認一次。
    pub fn start_match(&mut self) {
        if self.matching.running() {
            // 結束後再確認一次；狀態列同樣以「比對中…」結尾，結果會接在這次寫入結果之後
            self.matching.again = true;
            return self.append_status(MATCHING, Level::Info);
        }
        let Some(companion) = self.companion.clone() else {
            return self.append_status(NO_MATCH, Level::Warning);
        };
        let (tx, rx) = mpsc::channel();
        self.ws.services.spawner.spawn(Box::new(move || {
            let _ = tx.send(companion.run(&["match".into()]));
        }));
        self.matching.rx = Some(rx);
        self.append_status(MATCHING, Level::Info);
    }

    /// 每一幀：背景比對結束時重新載入並顯示結果。
    pub(crate) fn poll_match(&mut self) {
        let Some(rx) = &self.matching.rx else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("比對異常結束".into()),
        };
        self.matching.rx = None;
        let open = self.open_ids();
        let refreshed = self.ws.reload(&open);
        let reload_failed = refreshed.report.level != Level::Info;
        let reload_message = refreshed.report.message.clone();
        // 重新載入的「已重新載入」不取代寫入結果；比對結果接在寫入結果之後
        let before = self.status.take();
        self.apply_refreshed(refreshed);
        self.status = before;
        let mut report = match &result {
            Ok(o) if o.success() => Report::info(summary(&o.stdout)),
            other => Report::warning(format!(
                "編修已寫入，但比對失敗：{}；請執行 `cvewatch match`",
                crate::text::one_line(&output_text(other))
            )),
        };
        if reload_failed {
            report = Report::warning(format!("{}；{reload_message}", report.message));
        }
        self.finish_status(report);
        if std::mem::take(&mut self.matching.again) && self.still_stale() {
            self.start_match();
        }
    }

    /// 「比對待補」仍成立（讀取失敗時視為成立，寧可多比對一次）。
    fn still_stale(&self) -> bool {
        match_is_stale(&self.ws.conn).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "無法讀取比對待補狀態");
            true
        })
    }
}

fn worse(a: Level, b: Level) -> Level {
    let rank = |l: Level| match l {
        Level::Info => 0,
        Level::Warning => 1,
        Level::Error => 2,
    };
    if rank(b) > rank(a) {
        b
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_takes_first_two_lines() {
        assert_eq!(
            summary("比對完成：使用中元件 3、本機 CVE 9\n  新命中 1\n  等待區 0 筆\n"),
            "比對完成：使用中元件 3、本機 CVE 9；新命中 1"
        );
        assert_eq!(summary(""), "比對完成");
    }

    #[test]
    fn worse_level_wins() {
        assert_eq!(worse(Level::Info, Level::Warning), Level::Warning);
        assert_eq!(worse(Level::Error, Level::Info), Level::Error);
    }
}
