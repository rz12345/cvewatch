//! `cvewatch tui`：終端機介面。只透過 `cvewatch-core` 的查詢與寫入函式存取本機資料庫，
//! 不在背景抓資料；`S` 另啟一個 `cvewatch sync` 子程序。

mod actions;
pub mod app;
pub mod data;
pub mod dialog;
pub mod filter;
pub mod glyphs;
pub mod links;
pub mod services;
mod terminal;
pub mod ui;
pub mod width;

pub use app::{App, Context, Screen, View};
use cvewatch_core::CoreError;

/// 載入資料後進入事件迴圈，直到使用者離開。
pub fn run(ctx: Context) -> Result<(), CoreError> {
    let app = App::new(ctx)?;
    terminal::run(app)
}
