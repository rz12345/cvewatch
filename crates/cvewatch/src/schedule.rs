//! `cvewatch schedule install | status | uninstall`：呼叫 schtasks 建立／移除工作，
//! 以 PowerShell `Get-ScheduledTaskInfo` 讀取執行狀態（與系統語系無關）。
//!
//! 外部程式以 [`TaskRunner`] 抽象，單元測試以替身驗證參數與輸出解析。

use std::path::{Path, PathBuf};
use std::process::Command;

use cvewatch_core::schedule::xml::{
    encode_utf16, plan, task_path, task_xml, InstallContext, TaskSpec, DIGEST_TASK, SYNC_TASK,
    TASK_FOLDER,
};
use cvewatch_core::schedule::ScheduleConfig;
use cvewatch_core::CoreError;

/// 無視窗包裝程式的檔名。
pub const WRAPPER_EXE: &str = "cvewatchw.exe";
pub const TASKS: [&str; 2] = [SYNC_TASK, DIGEST_TASK];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl RunOutput {
    pub fn ok(&self) -> bool {
        self.code == 0
    }

    fn message(&self) -> String {
        let text = if self.stderr.trim().is_empty() {
            self.stdout.trim()
        } else {
            self.stderr.trim()
        };
        text.to_string()
    }
}

/// 執行外部程式（schtasks、powershell）。
pub trait TaskRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<RunOutput, String>;
}

/// 實際執行外部程式。
pub struct SystemRunner;

impl TaskRunner for SystemRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<RunOutput, String> {
        let mut cmd = Command::new(program);
        cmd.args(args);
        cvewatch_core::process::hide_window(&mut cmd);
        let out = cmd
            .output()
            .map_err(|e| format!("無法執行 {program}：{e}"))?;
        Ok(RunOutput {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

/// 目前 Windows 使用者（`DOMAIN\user`）。
pub fn current_user() -> Result<String, CoreError> {
    let user = std::env::var("USERNAME")
        .map_err(|_| CoreError::Invalid("無法取得目前的 Windows 使用者（USERNAME）".into()))?;
    Ok(match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.is_empty() => format!("{domain}\\{user}"),
        _ => user,
    })
}

/// 與 `cvewatch.exe` 同目錄的無視窗包裝程式。
pub fn wrapper_path(exe: &Path) -> Result<PathBuf, CoreError> {
    let dir = exe
        .parent()
        .ok_or_else(|| CoreError::Invalid("無法取得執行檔所在目錄".into()))?;
    let wrapper = dir.join(WRAPPER_EXE);
    if !wrapper.exists() {
        return Err(CoreError::Invalid(format!(
            "找不到 {}；請將 {WRAPPER_EXE} 與 cvewatch.exe 放在同一資料夾",
            wrapper.display()
        )));
    }
    Ok(wrapper)
}

/// 建立（或取代）兩個工作；XML 以 UTF-16 寫入暫存檔後交給 `schtasks /Create /XML ... /F`。
pub fn install(
    runner: &dyn TaskRunner,
    cfg: &ScheduleConfig,
    ctx: &InstallContext,
    temp_dir: &Path,
) -> Result<Vec<TaskSpec>, CoreError> {
    let specs = plan(cfg, ctx);
    for spec in &specs {
        let file = temp_dir.join(format!("cvewatch-{}-{}.xml", spec.name, std::process::id()));
        std::fs::write(&file, encode_utf16(&task_xml(spec)))
            .map_err(|e| CoreError::io(&file, e))?;
        let args = vec![
            "/Create".to_string(),
            "/TN".to_string(),
            task_path(&spec.name),
            "/XML".to_string(),
            file.to_string_lossy().into_owned(),
            "/F".to_string(),
        ];
        let result = runner.run("schtasks", &args);
        if let Err(e) = std::fs::remove_file(&file) {
            tracing::warn!(file = %file.display(), error = %e, "無法刪除排程暫存檔");
        }
        let out = result.map_err(CoreError::Invalid)?;
        if !out.ok() {
            return Err(CoreError::Invalid(format!(
                "建立排程工作 {} 失敗（schtasks 結束碼 {}）：{}",
                task_path(&spec.name),
                out.code,
                out.message()
            )));
        }
        tracing::info!(task = %task_path(&spec.name), "已建立排程工作");
    }
    Ok(specs)
}

/// 移除兩個工作；不存在的工作略過。回傳實際移除的工作。
pub fn uninstall(runner: &dyn TaskRunner) -> Result<Vec<String>, CoreError> {
    let installed: Vec<String> = status(runner)?
        .into_iter()
        .filter(|s| s.info.is_some())
        .map(|s| s.name)
        .collect();
    for name in &installed {
        let args = vec![
            "/Delete".to_string(),
            "/TN".to_string(),
            task_path(name),
            "/F".to_string(),
        ];
        let out = runner.run("schtasks", &args).map_err(CoreError::Invalid)?;
        if !out.ok() {
            return Err(CoreError::Invalid(format!(
                "移除排程工作 {} 失敗：{}",
                task_path(name),
                out.message()
            )));
        }
    }
    Ok(installed)
}

/// 工作的執行資訊。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct TaskInfo {
    #[serde(rename = "LastRunTime")]
    pub last_run: Option<String>,
    #[serde(rename = "LastTaskResult")]
    pub last_result: Option<i64>,
    #[serde(rename = "NextRunTime")]
    pub next_run: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStatus {
    pub name: String,
    /// None 表示尚未安裝
    pub info: Option<TaskInfo>,
}

/// 讀取一個工作的執行資訊；不存在時輸出空白。結尾 `exit 0`：`-Command` 的結束碼取自最後一個
/// 敘述的 `$?`，找不到工作（已以 SilentlyContinue 略過）會讓它成為 1。
fn status_script(name: &str) -> String {
    format!(
        "$ErrorActionPreference='Stop';[Console]::OutputEncoding=[Text.Encoding]::UTF8;\
         $t=Get-ScheduledTask -TaskPath '\\{TASK_FOLDER}\\' -TaskName '{name}' -ErrorAction SilentlyContinue;\
         if($t){{$i=$t|Get-ScheduledTaskInfo;\
         $f={{param($d) if($d -and $d.Year -gt 1999){{$d.ToString('yyyy-MM-dd HH:mm')}}else{{$null}}}};\
         [pscustomobject]@{{LastRunTime=(&$f $i.LastRunTime);LastTaskResult=[long]$i.LastTaskResult;\
         NextRunTime=(&$f $i.NextRunTime)}}|ConvertTo-Json -Compress}};exit 0"
    )
}

pub fn status(runner: &dyn TaskRunner) -> Result<Vec<TaskStatus>, CoreError> {
    TASKS
        .iter()
        .map(|name| {
            let args = vec![
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-Command".to_string(),
                status_script(name),
            ];
            let out = runner
                .run("powershell", &args)
                .map_err(CoreError::Invalid)?;
            if !out.ok() {
                return Err(CoreError::Invalid(format!(
                    "無法讀取排程狀態：{}",
                    out.message()
                )));
            }
            Ok(TaskStatus {
                name: (*name).to_string(),
                info: parse_info(&out.stdout)?,
            })
        })
        .collect()
}

fn parse_info(stdout: &str) -> Result<Option<TaskInfo>, CoreError> {
    let text = stdout.trim().trim_start_matches('\u{feff}');
    if text.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(text)
        .map(Some)
        .map_err(|e| CoreError::Invalid(format!("排程狀態格式無法解析（{e}）：{text}")))
}

/// 結果碼說明（Task Scheduler 常見值以十六進位顯示）。
pub fn result_text(code: i64) -> String {
    match code {
        0 => "0（成功）".into(),
        0x41300 => "0x41300（可執行）".into(),
        0x41301 => "0x41301（執行中）".into(),
        0x41303 => "0x41303（尚未執行）".into(),
        c if (0..=255).contains(&c) => format!("{c}"),
        c => format!("0x{:X}", c as u32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<(String, Vec<String>)>>,
        outputs: RefCell<Vec<RunOutput>>,
    }

    impl Fake {
        fn with(outputs: Vec<RunOutput>) -> Self {
            Fake {
                outputs: RefCell::new(outputs),
                ..Fake::default()
            }
        }
    }

    impl TaskRunner for Fake {
        fn run(&self, program: &str, args: &[String]) -> Result<RunOutput, String> {
            self.calls
                .borrow_mut()
                .push((program.to_string(), args.to_vec()));
            let mut o = self.outputs.borrow_mut();
            Ok(if o.is_empty() { ok("") } else { o.remove(0) })
        }
    }

    fn ok(stdout: &str) -> RunOutput {
        RunOutput {
            code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    fn ctx(dir: &Path) -> InstallContext {
        InstallContext {
            user_id: r"LAB\rz".into(),
            wrapper: dir.join(WRAPPER_EXE),
            config: None,
            today: chrono::NaiveDate::from_ymd_opt(2026, 11, 7).unwrap(),
        }
    }

    #[test]
    fn install_creates_both_tasks_with_force_and_removes_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        let specs = install(
            &fake,
            &ScheduleConfig::default(),
            &ctx(dir.path()),
            dir.path(),
        )
        .unwrap();
        assert_eq!(specs.len(), 2);
        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 2);
        for (call, name) in calls.iter().zip([r"cvewatch\sync", r"cvewatch\digest"]) {
            assert_eq!(call.0, "schtasks");
            assert_eq!(call.1[..3], ["/Create", "/TN", name]);
            assert_eq!(call.1[3], "/XML");
            assert_eq!(call.1[5], "/F", "重複安裝以 /F 取代既有工作");
        }
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "暫存 XML 已刪除"
        );
    }

    #[test]
    fn install_failure_reports_schtasks_message() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::with(vec![RunOutput {
            code: 1,
            stdout: String::new(),
            stderr: "ERROR: Access is denied.".into(),
        }]);
        let err = install(
            &fake,
            &ScheduleConfig::default(),
            &ctx(dir.path()),
            dir.path(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("Access is denied") && err.contains(r"cvewatch\sync"),
            "{err}"
        );
    }

    #[test]
    fn status_parses_info_and_missing_tasks() {
        let fake = Fake::with(vec![
            ok("{\"LastRunTime\":\"2026-11-07 08:00\",\"LastTaskResult\":4,\"NextRunTime\":\"2026-11-07 12:00\"}\r\n"),
            ok(""),
        ]);
        let s = status(&fake).unwrap();
        let sync = s[0].info.as_ref().unwrap();
        assert_eq!(sync.last_result, Some(4));
        assert_eq!(sync.next_run.as_deref(), Some("2026-11-07 12:00"));
        assert!(s[1].info.is_none(), "digest 尚未安裝");
        assert!(fake.calls.borrow()[0].1[3].contains("-TaskName 'sync'"));
    }

    #[test]
    fn uninstall_deletes_only_installed_tasks() {
        let fake = Fake::with(vec![
            ok("{\"LastRunTime\":null,\"LastTaskResult\":267011,\"NextRunTime\":null}"),
            ok(""),
            ok("SUCCESS"),
        ]);
        assert_eq!(uninstall(&fake).unwrap(), vec!["sync".to_string()]);
        let calls = fake.calls.borrow();
        assert_eq!(calls[2].1, ["/Delete", "/TN", r"cvewatch\sync", "/F"]);
    }

    #[test]
    fn result_codes() {
        assert_eq!(result_text(0), "0（成功）");
        assert_eq!(result_text(4), "4");
        assert_eq!(result_text(0x41303), "0x41303（尚未執行）");
        assert_eq!(result_text(-2147024894), "0x80070002");
    }
}
