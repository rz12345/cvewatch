//! 外部效果：開啟瀏覽器、剪貼簿、背景 sync 子程序與時鐘。以 trait 抽象，測試以替身取代。

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use chrono::{DateTime, FixedOffset, Local, Utc};
use cvewatch_core::secrets::SecretStore;
use cvewatch_core::ExitCode;

use crate::settings::{NotifyBackend, Spawner};

pub trait Opener {
    fn open(&self, url: &str) -> Result<(), String>;
}

pub trait ClipboardSink {
    fn copy(&self, text: &str) -> Result<(), String>;
}

/// 執行中的背景 sync。
pub trait RunningSync {
    /// 尚未結束為 `Ok(None)`；結束時為 `Ok(Some(結束碼))`，被終止而沒有結束碼時為 `Ok(Some(None))`。
    fn try_wait(&mut self) -> Result<Option<Option<i32>>, String>;
}

pub trait SyncLauncher {
    fn launch(&self) -> Result<Box<dyn RunningSync>, String>;
}

pub struct Services {
    pub opener: Box<dyn Opener>,
    pub clipboard: Box<dyn ClipboardSink>,
    pub launcher: Box<dyn SyncLauncher>,
    pub clock: Box<dyn Fn() -> DateTime<Utc>>,
    /// 顯示時間用的時區
    pub tz: DisplayTz,
    /// SMTP 密碼與 Redmine API key（設定畫面讀寫）
    pub secrets: Box<dyn SecretStore>,
    /// 測試寄信、測試 Redmine、載入清單、重新安裝排程
    pub backend: Arc<dyn NotifyBackend>,
    /// 背景工作（連線不阻塞畫面）
    pub spawner: Box<dyn Spawner>,
}

/// 顯示時區：本機時區逐筆換算（含日光節約時間），或固定位移（測試用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayTz {
    Local,
    Fixed(FixedOffset),
}

impl DisplayTz {
    pub fn format(self, t: DateTime<FixedOffset>, fmt: &str) -> String {
        match self {
            DisplayTz::Local => t.with_timezone(&Local).format(fmt).to_string(),
            DisplayTz::Fixed(offset) => t.with_timezone(&offset).format(fmt).to_string(),
        }
    }
}

impl Services {
    pub fn now(&self) -> DateTime<Utc> {
        (self.clock)()
    }
}

/// 系統預設瀏覽器。
pub struct SystemOpener;

impl Opener for SystemOpener {
    fn open(&self, url: &str) -> Result<(), String> {
        open::that_detached(url).map_err(|e| e.to_string())
    }
}

/// 系統剪貼簿（SSH 遠端等環境會失敗，由呼叫端在狀態列提示）。
pub struct SystemClipboard;

impl ClipboardSink for SystemClipboard {
    fn copy(&self, text: &str) -> Result<(), String> {
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(text.to_string()))
            .map_err(|e| e.to_string())
    }
}

/// 以目前執行檔啟動 `cvewatch [--config <path>] sync`，不開新視窗、不接標準輸出入。
pub struct ProcessLauncher {
    pub exe: PathBuf,
    pub config: Option<PathBuf>,
}

impl SyncLauncher for ProcessLauncher {
    fn launch(&self) -> Result<Box<dyn RunningSync>, String> {
        let mut cmd = Command::new(&self.exe);
        if let Some(config) = &self.config {
            cmd.arg("--config").arg(config);
        }
        cmd.arg("sync")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cvewatch_core::process::hide_window(&mut cmd);
        let child = cmd.spawn().map_err(|e| e.to_string())?;
        tracing::info!(pid = child.id(), "TUI 啟動背景 sync");
        Ok(Box::new(ChildSync(child)))
    }
}

struct ChildSync(Child);

impl RunningSync for ChildSync {
    fn try_wait(&mut self) -> Result<Option<Option<i32>>, String> {
        self.0
            .try_wait()
            .map(|s| s.map(|status| status.code()))
            .map_err(|e| e.to_string())
    }
}

/// 背景 sync 結束後的狀態列訊息。
pub fn sync_exit_message(code: Option<i32>) -> String {
    let is = |c: ExitCode| code == Some(i32::from(c.code()));
    if is(ExitCode::Success) {
        "背景 sync 完成".into()
    } else if is(ExitCode::SyncLocked) {
        "已有 sync 正在執行（排程 sync 進行中），本次未啟動".into()
    } else if is(ExitCode::PartialFailure) {
        "背景 sync 完成，但部分來源或自動比對失敗（結束碼 4），詳見 Sync log".into()
    } else {
        match code {
            Some(c) => format!("背景 sync 失敗（結束碼 {c}），詳見日誌"),
            None => "背景 sync 異常結束（沒有結束碼），詳見日誌".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_messages() {
        assert_eq!(sync_exit_message(Some(0)), "背景 sync 完成");
        assert!(sync_exit_message(Some(3)).starts_with("已有 sync 正在執行"));
        assert!(sync_exit_message(Some(4)).contains("結束碼 4"));
        assert!(sync_exit_message(Some(5)).contains("結束碼 5"));
        assert!(sync_exit_message(None).contains("異常結束"));
    }
}
