//! 資產生命週期（22 design D3、D5、D9）：版本分支對應、硬體型號比對、軟體與硬體 EoL 的判定
//! 順序、生命週期狀態與 Dashboard／摘要的計數。判定是純函式，GUI、TUI、CLI 與摘要共用；
//! 「今天」由呼叫端傳入（使用者時區的日期，見 [`local_today`]）。

pub mod config;
pub mod cycle;
pub mod eol_data;
pub mod evaluate;
pub mod hardware;
pub mod model;
pub mod summary;

pub use config::{LifecycleConfig, DEFAULT_REMINDER_DAYS, MAX_REMINDER_DAYS, MIN_REMINDER_DAYS};
pub use eol_data::{load_eol, EolData};
pub use evaluate::evaluate;
pub use model::{
    AssetLifecycle, ItemKind, ItemState, LifecycleFilter, LifecycleFlags, LifecycleItem,
    MissingReason, Source, Status,
};
pub use summary::{evaluate_active, summary, LifecycleCounts};

/// 使用者時區的今天（EoL 與到期日是日曆日）。
pub fn local_today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}
