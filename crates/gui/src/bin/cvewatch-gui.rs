//! `cvewatch-gui.exe`：Windows GUI subsystem（不開主控台視窗）。
//!
//! 繪製後端依序嘗試 wgpu → glow；兩者都無法建立視窗時以系統訊息框說明後結束（design D6）。

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::cell::RefCell;
use std::path::PathBuf;

use cvewatch_gui::startup::{self, Launched, Root, RootApp};
use eframe::egui;

const TITLE: &str = "cvewatch";

fn options(renderer: eframe::Renderer) -> eframe::NativeOptions {
    eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0])
            .with_drag_and_drop(true),
        ..Default::default()
    }
}

fn main() {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("cvewatch-gui.exe"));
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // 執行環境（日誌寫入器）保留到 main 結束；繪製後端退回時沿用，不重新啟動
    let Launched { root, runtime } = startup::launch(args, &exe);
    let error = match &root {
        Root::Error(e) => Some(e.clone()),
        Root::App(_) => None,
    };
    // 第一個後端在建立 App 前失敗時，視窗內容留給下一個後端使用；已建立過則重建
    let slot = RefCell::new(Some(root));
    let take = || -> Root {
        slot.borrow_mut()
            .take()
            .unwrap_or_else(|| startup::rebuild(runtime.as_ref(), error.as_ref(), &exe))
    };
    let mut errors = Vec::new();
    for renderer in [eframe::Renderer::Wgpu, eframe::Renderer::Glow] {
        let result = eframe::run_native(
            TITLE,
            options(renderer),
            Box::new(|cc| {
                tracing::info!(?renderer, "繪製後端");
                Ok(Box::new(RootApp::new(take(), &cc.egui_ctx)))
            }),
        );
        match result {
            Ok(()) => return,
            Err(e) => {
                tracing::warn!(?renderer, error = %e, "無法以此繪製後端開啟視窗");
                errors.push(format!("{renderer:?}：{e}"));
            }
        }
    }
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("cvewatch 無法開啟視窗")
        .set_description(format!(
            "無法建立繪圖環境（遠端桌面或虛擬機可能不支援）。\n請改用命令提示字元執行 `cvewatch tui`。\n\n{}",
            errors.join("\n")
        ))
        .show();
}
