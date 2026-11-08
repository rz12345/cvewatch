//! 啟動：解析 `--config`、啟動執行環境（與 CLI 相同的設定檔解析順序）、開啟資料庫、偵測
//! companion。失敗時不建立 `GuiApp`，改以同一個視窗顯示錯誤畫面（design D7）。

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cvewatch_core::paths::{default_config_path, resolve_config_location, CONFIG_ENV_VAR};
use cvewatch_core::runtime::{self, Runtime};
use cvewatch_core::CoreError;
use cvewatch_frontend::context::{self, ContextOptions};
use eframe::egui;

use crate::app::{GuiApp, Options};
use crate::companion::{self, Companion, ProcessCompanion};
use crate::files::NativeDialogs;

/// 命令列參數。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    pub config: Option<PathBuf>,
}

/// `--config <path>` 或 `--config=<path>`；其他參數為錯誤。
pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let text = a.to_string_lossy();
        if text == "--config" {
            let path = it.next().ok_or("`--config` 需要設定檔路徑")?;
            out.config = Some(PathBuf::from(path));
        } else if let Some(p) = text.strip_prefix("--config=") {
            out.config = Some(PathBuf::from(p));
        } else {
            return Err(format!(
                "不認得的參數 `{text}`；cvewatch-gui.exe 只接受 `--config <設定檔>`"
            ));
        }
        if out
            .config
            .as_ref()
            .is_some_and(|p| p.as_os_str().is_empty())
        {
            return Err("`--config` 需要設定檔路徑".into());
        }
    }
    Ok(out)
}

/// 啟動失敗的錯誤畫面。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorScreen {
    /// 錯誤原文（core 的訊息已含 `cvewatch init`、`cvewatch db import-key` 等提示）
    pub message: String,
    pub config_path: Option<PathBuf>,
}

impl ErrorScreen {
    pub fn new(message: impl Into<String>, config_path: Option<&Path>) -> ErrorScreen {
        ErrorScreen {
            message: message.into(),
            config_path: config_path.map(Path::to_path_buf),
        }
    }

    pub fn from_core(e: &CoreError, config_path: Option<&Path>) -> ErrorScreen {
        tracing::error!(error = %e, "視窗版啟動失敗");
        ErrorScreen::new(e.to_string(), config_path)
    }

    /// 繪製；按「關閉」時回傳 true。
    pub fn show(&self, ui: &mut egui::Ui) -> bool {
        let mut close = false;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("cvewatch 無法啟動");
            ui.add_space(8.0);
            ui.add(egui::Label::new(egui::RichText::new(&self.message).size(15.0)).wrap());
            ui.add_space(8.0);
            if let Some(p) = &self.config_path {
                ui.label(format!("設定檔：{}", p.display()));
            }
            ui.label("請在命令提示字元執行上面提到的 cvewatch 指令後，再重新開啟視窗版。");
            ui.add_space(12.0);
            close = ui.button("關閉").clicked();
        });
        close
    }
}

/// 視窗的根：主畫面或錯誤畫面。
pub enum Root {
    App(Box<GuiApp>),
    Error(ErrorScreen),
}

/// 啟動結果。`runtime` 持有日誌寫入器，必須保留到視窗關閉。
pub struct Launched {
    pub root: Root,
    pub runtime: Option<Runtime>,
}

impl Launched {
    fn error(screen: ErrorScreen, runtime: Option<Runtime>) -> Launched {
        Launched {
            root: Root::Error(screen),
            runtime,
        }
    }
}

/// 啟動執行環境並建立主畫面。`gui_exe`：本執行檔路徑（用來找同資料夾的 `cvewatch.exe`）。
pub fn launch(args: impl IntoIterator<Item = OsString>, gui_exe: &Path) -> Launched {
    let args = match parse_args(args) {
        Ok(a) => a,
        Err(e) => return Launched::error(ErrorScreen::new(e, None), None),
    };
    let location = match resolve_config_location(
        args.config,
        std::env::var_os(CONFIG_ENV_VAR),
        default_config_path(),
    ) {
        Ok(l) => l,
        Err(e) => return Launched::error(ErrorScreen::from_core(&CoreError::from(e), None), None),
    };
    let runtime = match runtime::start(&location, "gui") {
        Ok(r) => r,
        Err(e) => return Launched::error(ErrorScreen::from_core(&e, Some(&location.path)), None),
    };
    match build(&runtime, gui_exe) {
        Ok(app) => Launched {
            root: Root::App(Box::new(app)),
            runtime: Some(runtime),
        },
        Err(e) => {
            let screen = ErrorScreen::from_core(&e, Some(&runtime.location().path));
            Launched::error(screen, Some(runtime))
        }
    }
}

fn build(runtime: &Runtime, gui_exe: &Path) -> Result<GuiApp, CoreError> {
    let found = gui_exe.parent().and_then(companion::locate);
    if found.is_none() {
        tracing::warn!(gui = %gui_exe.display(), "同資料夾找不到 cvewatch.exe，停用同步、匯入與排程");
    }
    let cli_exe = found.clone().unwrap_or_else(|| companion::sibling(gui_exe));
    let options = ContextOptions::for_runtime(runtime, cli_exe)?;
    let target = options.cli.clone();
    let ctx = context::build(runtime, options)?;
    let companion = found
        .is_some()
        .then(|| Arc::new(ProcessCompanion::new(target)) as Arc<dyn Companion>);
    GuiApp::new(
        ctx,
        Options {
            warnings: runtime.warnings.clone(),
            companion,
            dialogs: Box::new(NativeDialogs),
            instant: Box::new(std::time::Instant::now),
        },
    )
}

/// eframe 的 App：根畫面與執行環境。
///
/// 執行環境（日誌寫入器）不放在這裡：由 `main` 保留到程式結束，繪製後端退回時沿用同一個。
pub struct RootApp {
    pub root: Root,
}

impl RootApp {
    /// 建立視窗內容並載入中文字型（找不到時於主畫面顯示警告）。
    pub fn new(mut root: Root, ctx: &egui::Context) -> RootApp {
        tracing::info!("視窗已建立");
        let font_warning = crate::fonts::install(ctx);
        if let (Root::App(app), Some(w)) = (&mut root, font_warning) {
            app.warnings.push(w);
        }
        RootApp { root }
    }
}

/// 繪製後端退回時重建視窗內容：沿用已啟動的執行環境（不重新啟動日誌），
/// 啟動失敗時重複顯示同一個錯誤畫面。
pub fn rebuild(runtime: Option<&Runtime>, error: Option<&ErrorScreen>, gui_exe: &Path) -> Root {
    if let Some(e) = error {
        return Root::Error(e.clone());
    }
    match runtime {
        Some(rt) => match build(rt, gui_exe) {
            Ok(app) => Root::App(Box::new(app)),
            Err(e) => Root::Error(ErrorScreen::from_core(&e, Some(&rt.location().path))),
        },
        None => Root::Error(ErrorScreen::new(
            "cvewatch 無法啟動（執行環境未建立）",
            None,
        )),
    }
}

impl eframe::App for RootApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match &mut self.root {
            Root::App(app) => app.show(ui),
            Root::Error(screen) => {
                if screen.show(ui) {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn config_argument_forms() {
        assert_eq!(parse_args(os(&[])).unwrap(), Args::default());
        let want = Some(PathBuf::from(r"D:\cvewatch\cvewatch.toml"));
        assert_eq!(
            parse_args(os(&["--config", r"D:\cvewatch\cvewatch.toml"]))
                .unwrap()
                .config,
            want
        );
        assert_eq!(
            parse_args(os(&[r"--config=D:\cvewatch\cvewatch.toml"]))
                .unwrap()
                .config,
            want
        );
        assert!(parse_args(os(&["--config"])).is_err());
        assert!(parse_args(os(&["--config="])).is_err());
        assert!(parse_args(os(&["sync"])).unwrap_err().contains("sync"));
    }

    #[test]
    fn bad_arguments_give_error_screen() {
        let l = launch(os(&["--bogus"]), Path::new(r"C:\x\cvewatch-gui.exe"));
        assert!(l.runtime.is_none());
        match l.root {
            Root::Error(e) => assert!(e.message.contains("--bogus")),
            Root::App(_) => panic!("expected error screen"),
        }
    }
}
