//! CVE 資料同步：cvelistV5（主要）、NVD、CISA KEV、FIRST EPSS、OSV 與原廠 CSAF 公告。
//!
//! 每個來源各自的游標、交易與 `sync_run` 紀錄；任一來源失敗不影響其他來源。

pub mod assigner;
pub mod compress;
pub mod csaf;
pub mod cursor;
pub mod cve5;
pub mod cvelist;
pub mod cvss;
pub mod endpoints;
pub mod epss;
pub mod kev;
pub mod nvd;
pub mod osv;
pub mod relevance;
pub mod runlog;
pub mod show;
pub mod source;
pub mod store;
pub mod sync;
