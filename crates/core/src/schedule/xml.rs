//! Task Scheduler XML（schema 1.4）。以目前使用者、登入時執行（InteractiveToken），
//! 錯過時間開機後補跑、僅在有網路時執行、使用電池時不停止、同時只執行一個、最長一小時。
//!
//! `schtasks /Create /XML` 讀取 UTF-16 檔案，見 [`encode_utf16`]。

use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use quick_xml::escape::escape;

use super::ScheduleConfig;

/// 工作排程器中的資料夾。
pub const TASK_FOLDER: &str = "cvewatch";
pub const SYNC_TASK: &str = "sync";
pub const DIGEST_TASK: &str = "digest";
/// 單次執行上限。
pub const EXECUTION_TIME_LIMIT: &str = "PT1H";

/// `cvewatch\sync` 這類完整工作名稱。
pub fn task_path(name: &str) -> String {
    format!("{TASK_FOLDER}\\{name}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// 自 `start` 起每 `hours` 小時一次
    EveryHours { hours: u32, start: NaiveDateTime },
    /// 每天 `at` 的時間
    Daily { at: NaiveDateTime },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSpec {
    /// 資料夾內的名稱（`sync` / `digest`）
    pub name: String,
    pub description: String,
    /// `DOMAIN\user`
    pub user_id: String,
    /// 無視窗包裝程式 `cvewatchw.exe`
    pub command: PathBuf,
    pub arguments: String,
    pub working_dir: PathBuf,
    pub trigger: Trigger,
}

/// 安裝時的環境。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallContext {
    pub user_id: String,
    pub wrapper: PathBuf,
    /// 明確指定的設定檔（以 `--config` 傳給排程執行的命令）
    pub config: Option<PathBuf>,
    /// 觸發器起始日期（本機）
    pub today: NaiveDate,
}

/// 依設定產生「定期 sync」與「每日摘要」兩個工作。
pub fn plan(cfg: &ScheduleConfig, ctx: &InstallContext) -> Vec<TaskSpec> {
    let working_dir = ctx
        .wrapper
        .parent()
        .map_or_else(PathBuf::new, Path::to_path_buf);
    let args = |command: &str| match &ctx.config {
        Some(path) => format!("--config {} {command}", quote_arg(&path.to_string_lossy())),
        None => command.to_string(),
    };
    let (h, m) = cfg.digest_time();
    let digest_at = NaiveTime::from_hms_opt(h, m, 0).unwrap_or(NaiveTime::MIN);
    vec![
        TaskSpec {
            name: SYNC_TASK.into(),
            description: format!(
                "cvewatch：每 {} 小時同步 CVE 資料來源並比對，有緊急命中時通知",
                cfg.sync_every_hours
            ),
            user_id: ctx.user_id.clone(),
            command: ctx.wrapper.clone(),
            arguments: args("sync"),
            working_dir: working_dir.clone(),
            trigger: Trigger::EveryHours {
                hours: cfg.sync_every_hours,
                start: ctx.today.and_time(NaiveTime::MIN),
            },
        },
        TaskSpec {
            name: DIGEST_TASK.into(),
            description: format!("cvewatch：每天 {} 寄送每日摘要", cfg.digest_at),
            user_id: ctx.user_id.clone(),
            command: ctx.wrapper.clone(),
            arguments: args("notify digest"),
            working_dir,
            trigger: Trigger::Daily {
                at: ctx.today.and_time(digest_at),
            },
        },
    ]
}

/// Windows 命令列參數引號（路徑不會包含 `"`；含空白時加引號，結尾反斜線加倍）。
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let trailing = arg.len() - arg.trim_end_matches('\\').len();
    format!("\"{}{}\"", arg.replace('"', "\\\""), "\\".repeat(trailing))
}

fn boundary(t: &NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn trigger_xml(trigger: &Trigger) -> String {
    match trigger {
        Trigger::EveryHours { hours, start } => format!(
            "    <TimeTrigger>
      <Repetition>
        <Interval>PT{hours}H</Interval>
        <StopAtDurationEnd>false</StopAtDurationEnd>
      </Repetition>
      <StartBoundary>{}</StartBoundary>
      <Enabled>true</Enabled>
    </TimeTrigger>
",
            boundary(start)
        ),
        Trigger::Daily { at } => format!(
            "    <CalendarTrigger>
      <StartBoundary>{}</StartBoundary>
      <Enabled>true</Enabled>
      <ScheduleByDay>
        <DaysInterval>1</DaysInterval>
      </ScheduleByDay>
    </CalendarTrigger>
",
            boundary(at)
        ),
    }
}

/// 工作定義 XML（宣告 UTF-16，寫檔時以 [`encode_utf16`] 編碼）。
pub fn task_xml(spec: &TaskSpec) -> String {
    let uri = format!("\\{}", task_path(&spec.name));
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Author>cvewatch</Author>
    <Description>{description}</Description>
    <URI>{uri}</URI>
  </RegistrationInfo>
  <Triggers>
{trigger}  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>true</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>{limit}</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{command}</Command>
      <Arguments>{arguments}</Arguments>
      <WorkingDirectory>{working_dir}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#,
        description = escape(spec.description.as_str()),
        uri = escape(uri.as_str()),
        trigger = trigger_xml(&spec.trigger),
        user = escape(spec.user_id.as_str()),
        limit = EXECUTION_TIME_LIMIT,
        command = escape(spec.command.to_string_lossy()),
        arguments = escape(spec.arguments.as_str()),
        working_dir = escape(spec.working_dir.to_string_lossy()),
    )
}

/// UTF-16 LE（含 BOM），換行為 CRLF。
pub fn encode_utf16(xml: &str) -> Vec<u8> {
    let text = xml.replace("\r\n", "\n").replace('\n', "\r\n");
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(quote_arg(r"C:\a\b.toml"), r"C:\a\b.toml");
        assert_eq!(quote_arg(r"C:\My Files\b.toml"), r#""C:\My Files\b.toml""#);
        assert_eq!(quote_arg(r"C:\My Dir\"), r#""C:\My Dir\\""#);
        assert_eq!(quote_arg(""), r#""""#);
    }

    #[test]
    fn utf16_has_bom_and_crlf() {
        let bytes = encode_utf16("a\nb");
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
        assert_eq!(bytes.len(), 2 + 4 * 2);
        assert_eq!(&bytes[4..6], &[b'\r', 0]);
    }

    #[test]
    fn user_and_paths_are_escaped() {
        let spec = TaskSpec {
            name: "sync".into(),
            description: "a & b".into(),
            user_id: r"LAB\R&D".into(),
            command: PathBuf::from(r"C:\x<y>\cvewatchw.exe"),
            arguments: "sync".into(),
            working_dir: PathBuf::from(r"C:\x<y>"),
            trigger: Trigger::Daily {
                at: NaiveDate::from_ymd_opt(2026, 11, 7)
                    .unwrap()
                    .and_hms_opt(8, 30, 0)
                    .unwrap(),
            },
        };
        let xml = task_xml(&spec);
        assert!(xml.contains(r"<UserId>LAB\R&amp;D</UserId>"));
        assert!(xml.contains("C:\\x&lt;y&gt;\\cvewatchw.exe"));
        assert!(xml.contains("a &amp; b"));
    }
}
