//! cvewatch 視窗版（`cvewatch-gui.exe`）：egui / eframe 介面。
//!
//! 只寫「畫面」：所有寫入與判斷都經 `cvewatch-frontend`（`Workspace`、`FindingsModel`、
//! `settings`），與 TUI、CLI 共用同一個加密資料庫與設定檔。清冊匯入、背景 sync 與重新安裝
//! 排程以隱藏視窗的子程序執行同資料夾的 `cvewatch.exe`（[`companion`]）。

pub mod app;
pub mod companion;
pub mod dialogs;
pub mod files;
pub mod fonts;
pub mod matching;
pub mod screens;
pub mod settings;
pub mod shell;
pub mod startup;
pub mod text;

pub use app::{GuiApp, Options, Screen};
pub use startup::{ErrorScreen, Launched, Root, RootApp};
