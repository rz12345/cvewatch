//! 外部效果：開啟瀏覽器、剪貼簿、背景 sync 子程序與時鐘。以 trait 抽象，測試以替身取代。

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use chrono::{DateTime, FixedOffset, Local, Utc};
use cvewatch_core::secrets::SecretStore;
use cvewatch_core::ExitCode;

use crate::report::Report;
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
    /// 匯出 VEX 的預設資料夾（使用者「文件」資料夾）
    pub documents_dir: PathBuf,
    /// 可開啟的連結（TUI 允許 http(s)，GUI 只允許 https）
    pub link_policy: LinkPolicy,
}

/// 可開啟的連結種類。參考連結來自外部資料，一律不開啟其他協定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkPolicy {
    /// http 與 https
    #[default]
    Web,
    /// 只允許 https
    HttpsOnly,
}

impl LinkPolicy {
    pub fn allows(self, url: &str) -> bool {
        match self {
            LinkPolicy::Web => crate::links::is_safe_url(url),
            LinkPolicy::HttpsOnly => crate::links::is_https(url),
        }
    }

    /// 被拒絕時的說明。
    pub fn refusal(self, url: &str) -> String {
        match self {
            LinkPolicy::Web => format!("拒絕開啟非 http(s) 連結：{url}"),
            LinkPolicy::HttpsOnly => format!("只開啟 https 連結：{url}"),
        }
    }
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

/// 子程序要執行的 CLI：執行檔與要傳入的設定檔。背景 sync 與重新安裝排程共用，
/// 確保兩者傳遞設定檔的方式一致。
///
/// TUI 傳入自己的執行檔；GUI 必須傳入同資料夾的 `cvewatch.exe`（GUI 執行檔不認得 `sync`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliTarget {
    pub exe: PathBuf,
    /// 明確指定的設定檔（絕對路徑）；None 時子程序自行解析（環境變數會自動繼承）
    pub config: Option<PathBuf>,
}

impl CliTarget {
    /// `<exe> [--config <path>] <args…>`，不開新視窗、不接標準輸入。
    pub fn command<S: AsRef<std::ffi::OsStr>>(&self, args: &[S]) -> Command {
        let mut cmd = Command::new(&self.exe);
        if let Some(config) = &self.config {
            cmd.arg("--config").arg(config);
        }
        cmd.args(args).stdin(Stdio::null());
        cvewatch_core::process::hide_window(&mut cmd);
        cmd
    }
}

/// 背景啟動 `cvewatch [--config <path>] sync`，不開新視窗、不接標準輸出入。
pub struct ProcessLauncher(pub CliTarget);

impl ProcessLauncher {
    /// 要執行的命令：`<exe> [--config <path>] sync`。
    pub fn command(&self) -> Command {
        let mut cmd = self.0.command(&["sync"]);
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        cmd
    }
}

impl SyncLauncher for ProcessLauncher {
    fn launch(&self) -> Result<Box<dyn RunningSync>, String> {
        let child = self.command().spawn().map_err(|e| e.to_string())?;
        tracing::info!(pid = child.id(), exe = %self.0.exe.display(), "啟動背景 sync");
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

/// 背景 sync 結束後的結果（文字與等級在同一處依結束碼決定）。
pub fn sync_exit_report(code: Option<i32>) -> Report {
    let is = |c: ExitCode| code == Some(i32::from(c.code()));
    if is(ExitCode::Success) {
        Report::info("背景 sync 完成")
    } else if is(ExitCode::SyncLocked) {
        Report::warning("已有 sync 正在執行（排程 sync 進行中），本次未啟動")
    } else if is(ExitCode::PartialFailure) {
        Report::warning("背景 sync 完成，但部分來源或自動比對失敗（結束碼 4），詳見 Sync log")
    } else {
        Report::error(match code {
            Some(c) => format!("背景 sync 失敗（結束碼 {c}），詳見日誌"),
            None => "背景 sync 異常結束（沒有結束碼），詳見日誌".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Level;

    #[test]
    fn exit_code_reports() {
        let r = |code| sync_exit_report(code);
        assert_eq!(r(Some(0)), Report::info("背景 sync 完成"));
        assert!(r(Some(3)).message.starts_with("已有 sync 正在執行"));
        assert_eq!(r(Some(3)).level, Level::Warning);
        assert!(r(Some(4)).message.contains("結束碼 4"));
        assert_eq!(r(Some(4)).level, Level::Warning);
        assert!(r(Some(5)).message.contains("結束碼 5"));
        assert_eq!(r(Some(5)).level, Level::Error);
        assert!(r(None).message.contains("異常結束"));
    }

    fn args(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn target(config: Option<&str>) -> CliTarget {
        CliTarget {
            exe: PathBuf::from(r"C:\Tools\cvewatch.exe"),
            config: config.map(PathBuf::from),
        }
    }

    #[test]
    fn launcher_runs_given_exe_with_optional_config() {
        let cmd = ProcessLauncher(target(None)).command();
        assert_eq!(cmd.get_program(), r"C:\Tools\cvewatch.exe");
        assert_eq!(args(&cmd), ["sync"]);
        let explicit = ProcessLauncher(target(Some(r"D:\cfg\cvewatch.toml")));
        assert_eq!(
            args(&explicit.command()),
            ["--config", r"D:\cfg\cvewatch.toml", "sync"]
        );
    }

    #[test]
    fn schedule_install_uses_same_config_argument() {
        let cmd = target(Some(r"D:\cfg\cvewatch.toml")).command(&["schedule", "install"]);
        assert_eq!(
            args(&cmd),
            ["--config", r"D:\cfg\cvewatch.toml", "schedule", "install"]
        );
    }
}
