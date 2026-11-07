//! 背景 sync（另啟 `cvewatch sync` 子程序）與新資料偵測。只提示有新資料，不自動重新載入
//! （避免使用者正在看的列跳動）。

use std::time::{Duration, Instant};

use cvewatch_core::matching::dashboard;

use super::Workspace;
use crate::report::Report;
use crate::services::sync_exit_report;

/// 檢查是否有新同步資料的間隔。
pub const NEW_DATA_POLL: Duration = Duration::from_secs(5);

impl<V> Workspace<V> {
    /// 背景啟動一次 sync；已有本程序啟動的 sync 在執行時不重複啟動。
    pub fn start_sync(&mut self) -> Report {
        if self.sync.is_some() {
            return Report::info("背景 sync 仍在執行");
        }
        match self.services.launcher.launch() {
            Ok(child) => {
                self.sync = Some(child);
                Report::info("已在背景啟動 sync")
            }
            Err(e) => Report::error(format!("無法啟動 sync：{e}")),
        }
    }

    /// 定期呼叫：背景 sync 結束檢查，以及每 [`NEW_DATA_POLL`] 檢查一次新資料。
    /// sync 結束時回傳結果。
    pub fn tick(&mut self, now: Instant) -> Option<Report> {
        let report = self.finish_sync(now);
        let due = self
            .last_poll
            .is_none_or(|t| now.duration_since(t) >= NEW_DATA_POLL);
        if due {
            self.last_poll = Some(now);
            self.check_new_data();
        }
        report
    }

    /// 背景 sync 已結束時回傳結果，並立即檢查新資料（不等下一次輪詢；算作一次輪詢）。
    fn finish_sync(&mut self, now: Instant) -> Option<Report> {
        let child = self.sync.as_mut()?;
        match child.try_wait() {
            Ok(None) => None,
            Ok(Some(code)) => {
                self.sync = None;
                // sync（含結尾的自動比對）已結束：立即檢查新資料
                self.check_new_data();
                self.last_poll = Some(now);
                Some(sync_exit_report(code))
            }
            Err(e) => {
                self.sync = None;
                Some(Report::error(format!("無法取得背景 sync 狀態：{e}")))
            }
        }
    }

    /// 同步／比對標記與載入時不同即標示有新資料。
    pub fn check_new_data(&mut self) {
        match dashboard::data_marker(&self.conn) {
            Ok(marker) => {
                if marker != self.data.marker {
                    self.new_data = true;
                }
            }
            Err(e) => tracing::warn!(error = %e, "檢查新同步資料失敗"),
        }
    }
}
