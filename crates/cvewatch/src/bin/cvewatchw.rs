//! `cvewatchw.exe`：工作排程器使用的無主控台包裝程式。
//!
//! 以 Windows GUI subsystem 建置（不開主控台視窗），用 `CREATE_NO_WINDOW` 啟動同目錄的
//! `cvewatch.exe`，原樣傳入參數並以其結束碼結束，讓工作排程器記錄實際結果。

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// 主程式檔名。
#[cfg(windows)]
const MAIN_EXE: &str = "cvewatch.exe";
#[cfg(not(windows))]
const MAIN_EXE: &str = "cvewatch";

/// 無法啟動主程式時的結束碼（與 cvewatch 的「一般錯誤」相同）。
const LAUNCH_FAILED: u8 = 1;

/// 與包裝程式同目錄的主程式。
fn sibling(wrapper: &Path) -> PathBuf {
    wrapper.with_file_name(MAIN_EXE)
}

/// 子程序結束碼轉為本程序結束碼；沒有結束碼（被終止）時為一般錯誤。
fn exit_code(code: Option<i32>) -> ExitCode {
    match code.and_then(|c| u8::try_from(c).ok()) {
        Some(c) => ExitCode::from(c),
        None => ExitCode::from(LAUNCH_FAILED),
    }
}

fn run(args: Vec<OsString>) -> Result<Option<i32>, String> {
    let wrapper = std::env::current_exe().map_err(|e| format!("無法取得執行檔路徑：{e}"))?;
    let main = sibling(&wrapper);
    let mut cmd = Command::new(&main);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cvewatch_core::process::hide_window(&mut cmd);
    let status = cmd
        .status()
        .map_err(|e| format!("無法啟動 {}：{e}", main.display()))?;
    Ok(status.code())
}

fn main() -> ExitCode {
    // 沒有主控台可以顯示錯誤；cvewatch 本身的錯誤寫入其日誌，這裡只回報無法啟動的情況
    match run(std::env::args_os().skip(1).collect()) {
        Ok(code) => exit_code(code),
        Err(_) => ExitCode::from(LAUNCH_FAILED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_is_in_same_directory() {
        let w = Path::new(r"C:\Tools\cvewatch\cvewatchw.exe");
        assert_eq!(sibling(w), Path::new(r"C:\Tools\cvewatch").join(MAIN_EXE));
    }

    #[test]
    fn exit_codes_pass_through() {
        assert_eq!(exit_code(Some(4)), ExitCode::from(4));
        assert_eq!(exit_code(Some(0)), ExitCode::SUCCESS);
        assert_eq!(exit_code(None), ExitCode::from(LAUNCH_FAILED));
        assert_eq!(exit_code(Some(-1)), ExitCode::from(LAUNCH_FAILED));
    }
}
