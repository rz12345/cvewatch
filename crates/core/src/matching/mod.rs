//! 資產 × CVE 比對：版本比較、證據判定、命中、等待區、風險等級與處理狀態。
//!
//! 流程（[`run::run_match`]）：載入 → 各來源證據（[`evidence`]）→ 彙整（[`engine`]）
//! → 寫入命中與自動狀態轉換（[`findings`]）→ 等待區重評估（[`pending`]）。

pub mod dashboard;
pub mod engine;
pub mod evaluate;
pub mod evidence;
pub mod findings;
pub mod load;
pub mod pending;
pub mod pending_ops;
pub mod query;
pub mod run;
pub mod scoring;
pub mod triage;
pub mod version;
