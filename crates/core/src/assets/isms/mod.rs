//! ISMS 資訊資產清冊（17 design D1）：讀取機構的 ISMS 清冊並轉成 cvewatch 清冊草稿。
//!
//! | 模組 | 責任 |
//! |---|---|
//! | [`workbook`] | 找出表頭列與欄位，產生 [`workbook::IsmsRow`] |
//! | [`row`] | 單列轉換：ISMS 編號、大類過濾、欄位驗證 |
//! | [`describe`] | 資產描述拆行、清理、目錄比對、重複合併 |
//! | [`draft`] | 整批組裝：沿用既有資產、`systems`、報表 |
//! | [`report`] | 報表結構與文字（CLI 摘要、`isms_report` 工作表、畫面摘要） |
//! | [`output`] | 寫出草稿 xlsx |

pub mod describe;
pub mod draft;
pub mod output;
pub mod report;
pub mod row;
pub mod workbook;
pub mod workbook_read;

pub use draft::{build_draft, Draft, DraftContext};
pub use output::{generate_draft, write_draft};
pub use report::DraftReport;
