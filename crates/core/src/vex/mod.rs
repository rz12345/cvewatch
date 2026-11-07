//! CycloneDX VEX 交換：處理判定的匯出（[`export`]、[`file`]）與匯入（[`import`]）。
//!
//! 每筆命中（元件 × CVE）輸出一個 vulnerability；狀態對應見 [`mapping`]，元件識別見 [`bomref`]，
//! detail 與時間見 [`detail`]。

pub mod bomref;
pub mod detail;
pub mod document;
pub mod export;
pub mod file;
pub mod import;
pub mod mapping;
