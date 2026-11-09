//! ISMS 資訊資產清冊（17 design D1、18 design D4）：讀取機構的 ISMS 清冊並匯入 cvewatch。
//!
//! | 模組 | 責任 |
//! |---|---|
//! | [`workbook`] | 找出表頭列與欄位，產生 [`workbook::IsmsRow`] |
//! | [`row`] | 單列轉換：ISMS 編號、大類過濾、欄位驗證 |
//! | [`describe`] | 資產描述拆行、清理、目錄比對、重複合併 |
//! | [`conversion`] | 整批組裝：沿用既有資產、資產說明、報表 |
//! | [`report`] | 報表結構與文字（CLI 預覽與摘要，GUI 預覽顯示同一份輸出） |
//! | [`import`] | 轉換結果 → 清冊 → 一般清冊匯入的交易 |

pub mod conversion;
pub mod describe;
pub mod import;
pub mod report;
pub mod row;
pub mod workbook;
pub mod workbook_read;

pub use conversion::{build_conversion, Conversion, ConversionContext, ConvertedAsset};
pub use import::{import_isms, to_inventory, IsmsImport};
pub use report::IsmsReport;
