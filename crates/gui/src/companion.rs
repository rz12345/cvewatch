//! 同資料夾的 `cvewatch.exe`（companion）：背景 sync、清冊匯入與重新安裝排程都以隱藏視窗的
//! 子程序執行它。視窗版本身不認得 `sync`、`asset import` 等子命令。
//!
//! 命令一律由 frontend 的 `CliTarget` 組成（`--config` 的傳遞方式與 sync、排程相同）。

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use cvewatch_frontend::services::CliTarget;

/// 命令列版執行檔的檔名。
pub const CLI_EXE_NAME: &str = if cfg!(windows) {
    "cvewatch.exe"
} else {
    "cvewatch"
};

/// 子程序的執行時間上限（超過即終止，避免匯入視窗永遠停在「匯入中」）。
pub const RUN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// 等待子程序結束的輪詢間隔。
const WAIT_POLL: Duration = Duration::from_millis(100);

/// 子程序結束後等待輸出讀完的上限（孫程序仍持有管線時不無限等待）。
const DRAIN_GRACE: Duration = Duration::from_secs(5);

/// `gui_exe` 同資料夾的命令列版執行檔路徑（不檢查是否存在）。
pub fn sibling(gui_exe: &Path) -> PathBuf {
    gui_exe.with_file_name(CLI_EXE_NAME)
}

/// `dir` 中有命令列版執行檔時回傳其路徑。
pub fn locate(dir: &Path) -> Option<PathBuf> {
    let path = dir.join(CLI_EXE_NAME);
    path.is_file().then_some(path)
}

/// 子程序結束後的輸出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// 結束碼；被終止而沒有結束碼時為 None
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// 執行 companion（在背景執行緒呼叫，會等待子程序結束）。
pub trait Companion: Send + Sync {
    /// `args` 為子命令與參數（不含 `--config`，由 `CliTarget` 加上）。無法啟動或逾時時回傳原因。
    fn run(&self, args: &[OsString]) -> Result<Output, String>;
}

/// 正式環境：以 `CliTarget` 隱藏視窗執行，不接標準輸入，擷取標準輸出與錯誤輸出。
pub struct ProcessCompanion {
    pub target: CliTarget,
    pub timeout: Duration,
}

impl ProcessCompanion {
    pub fn new(target: CliTarget) -> ProcessCompanion {
        ProcessCompanion {
            target,
            timeout: RUN_TIMEOUT,
        }
    }

    pub fn command(&self, args: &[OsString]) -> Command {
        let mut cmd = self.target.command(args);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd
    }
}

/// 在另一個執行緒讀完管線（避免子程序因管線滿而卡住），結果經 channel 取回。
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = pipe {
            if let Err(e) = p.read_to_end(&mut buf) {
                tracing::warn!(error = %e, "讀取子程序輸出失敗");
            }
        }
        let _ = tx.send(buf);
    });
    rx
}

/// 取回輸出；`deadline` 前沒有讀完（管線仍被其他程序持有）時標示未完整。
fn collect(rx: &Receiver<Vec<u8>>, deadline: Instant) -> String {
    let wait = deadline.saturating_duration_since(Instant::now());
    match rx.recv_timeout(wait) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).trim_end().to_string(),
        Err(_) => "（輸出未能完整讀取）".into(),
    }
}

impl Companion for ProcessCompanion {
    fn run(&self, args: &[OsString]) -> Result<Output, String> {
        let exe = self.target.exe.display().to_string();
        tracing::info!(exe = %exe, ?args, "執行 companion");
        let mut child = self
            .command(args)
            .spawn()
            .map_err(|e| format!("無法執行 {exe}：{e}"))?;
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() >= deadline => {
                    tracing::warn!(exe = %exe, "companion 逾時，終止子程序");
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(format!(
                        "{exe} 執行超過 {} 分鐘，已終止",
                        self.timeout.as_secs() / 60
                    ));
                }
                Ok(None) => std::thread::sleep(WAIT_POLL),
                Err(e) => break Err(format!("無法取得 {exe} 的狀態：{e}")),
            }
        };
        let grace = Instant::now() + DRAIN_GRACE;
        let (out, err) = (collect(&stdout, grace), collect(&stderr, grace));
        status.map(|s| Output {
            code: s.code(),
            stdout: out,
            stderr: err,
        })
    }
}

/// 匯入的種類（18 design D8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// 一般清冊：`asset import`
    Inventory,
    /// ISMS 資訊資產清冊：`asset import-isms`（沒有元件檔與退役選項）
    Isms,
}

/// 匯入的參數：`asset import <file> [--components C] [--ips I] [--prune] [--dry-run]`，或
/// `asset import-isms <file> [--dry-run]`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportArgs {
    pub kind: ImportKind,
    pub file: PathBuf,
    pub components: Option<PathBuf>,
    /// CSV 清冊的 IP 檔（20 design D11）
    pub ips: Option<PathBuf>,
    pub prune: bool,
}

impl ImportArgs {
    pub fn to_args(&self, dry_run: bool) -> Vec<OsString> {
        let command = match self.kind {
            ImportKind::Inventory => "import",
            ImportKind::Isms => "import-isms",
        };
        let mut args: Vec<OsString> = vec![
            "asset".into(),
            command.into(),
            self.file.clone().into_os_string(),
        ];
        if self.kind == ImportKind::Isms {
            if dry_run {
                args.push("--dry-run".into());
            }
            return args;
        }
        if let Some(c) = &self.components {
            args.extend(["--components".into(), c.clone().into_os_string()]);
        }
        if let Some(i) = &self.ips {
            args.extend(["--ips".into(), i.clone().into_os_string()]);
        }
        if self.prune {
            args.push("--prune".into());
        }
        if dry_run {
            args.push("--dry-run".into());
        }
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn locate_finds_sibling_only_when_present() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(locate(dir.path()), None);
        std::fs::write(dir.path().join(CLI_EXE_NAME), b"").unwrap();
        assert_eq!(locate(dir.path()), Some(dir.path().join(CLI_EXE_NAME)));
        let gui = dir.path().join("cvewatch-gui.exe");
        assert_eq!(sibling(&gui), dir.path().join(CLI_EXE_NAME));
    }

    #[test]
    fn directory_with_cli_name_is_not_a_companion() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(CLI_EXE_NAME)).unwrap();
        assert_eq!(locate(dir.path()), None);
    }

    #[test]
    fn import_command_uses_cli_target_config() {
        let a = ImportArgs {
            kind: ImportKind::Inventory,
            file: PathBuf::from(r"C:\in\assets.csv"),
            components: Some(PathBuf::from(r"C:\in\components.csv")),
            ips: Some(PathBuf::from(r"C:\in\ips.csv")),
            prune: true,
        };
        let c = ProcessCompanion::new(CliTarget {
            exe: PathBuf::from(r"C:\Tools\cvewatch.exe"),
            config: Some(PathBuf::from(r"D:\cfg\cvewatch.toml")),
        });
        let cmd = c.command(&a.to_args(true));
        assert_eq!(cmd.get_program(), r"C:\Tools\cvewatch.exe");
        assert_eq!(
            strings(&cmd),
            [
                "--config",
                r"D:\cfg\cvewatch.toml",
                "asset",
                "import",
                r"C:\in\assets.csv",
                "--components",
                r"C:\in\components.csv",
                "--ips",
                r"C:\in\ips.csv",
                "--prune",
                "--dry-run"
            ]
        );
        let plain = ImportArgs {
            components: None,
            ips: None,
            prune: false,
            ..a
        };
        let implicit = ProcessCompanion::new(CliTarget {
            config: None,
            ..c.target.clone()
        });
        assert_eq!(
            strings(&implicit.command(&plain.to_args(false))),
            ["asset", "import", r"C:\in\assets.csv"]
        );
    }

    #[test]
    fn isms_import_has_no_components_or_prune() {
        let a = ImportArgs {
            kind: ImportKind::Isms,
            file: PathBuf::from(r"C:\in\isms.xlsx"),
            components: Some(PathBuf::from(r"C:\in\components.csv")),
            ips: Some(PathBuf::from(r"C:\in\ips.csv")),
            prune: true,
        };
        let strings = |dry_run| -> Vec<String> {
            a.to_args(dry_run)
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(
            strings(true),
            ["asset", "import-isms", r"C:\in\isms.xlsx", "--dry-run"]
        );
        assert_eq!(strings(false), ["asset", "import-isms", r"C:\in\isms.xlsx"]);
    }

    #[test]
    fn missing_executable_is_reported() {
        let c = ProcessCompanion::new(CliTarget {
            exe: PathBuf::from(r"C:\no\such\cvewatch.exe"),
            config: None,
        });
        let err = c.run(&["--version".into()]).unwrap_err();
        assert!(err.contains("無法執行"), "{err}");
    }
}
