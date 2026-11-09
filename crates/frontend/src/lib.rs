//! TUI 與 GUI 共用的前端層：畫面資料、命中清單模型、有副作用的操作（回傳 [`Report`]）、
//! 外部效果抽象、設定表單與啟動 [`Context`]。不相依任何 UI 框架（按鍵、輸入框、繪製）。

pub mod asset_form;
pub mod context;
pub mod data;
pub mod display;
pub mod filter;
pub mod findings;
pub mod links;
pub mod product_search;
pub mod report;
pub mod services;
pub mod settings;
pub mod workspace;

#[cfg(feature = "test-support")]
pub mod test_support;

pub use context::{Context, ContextOptions};
pub use findings::{FindingsModel, ListView};
pub use report::{Hint, Level, Report};
pub use workspace::Workspace;
