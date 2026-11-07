//! 各來源共用的執行環境、選項、統計與進度回報。

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};

use super::endpoints::Endpoints;
use super::relevance::RelevanceIndex;
use crate::catalog::Catalog;
use crate::http::HttpClient;
use crate::paths::DataPaths;

/// `cvewatch sync` 的選項。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncOptions {
    /// 以最新 baseline 依目前目錄重新篩選
    pub rescan: bool,
    /// 覆寫游標起點（UTC 日期）
    pub since: Option<NaiveDate>,
}

/// 一個來源一次執行的結果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncStats {
    pub added: u64,
    pub updated: u64,
    /// 給使用者看的提示（例如目錄已變更、略過的文件），也寫入 `sync_run.error`
    pub notes: Vec<String>,
}

impl SyncStats {
    pub fn note(&mut self, message: impl Into<String>) {
        self.notes.push(message.into());
    }
}

/// 長時間作業的進度回報；執行檔以 indicatif 實作，測試與排程用 [`NoProgress`]。
pub trait Progress {
    fn begin(&self, label: &str, total_bytes: Option<u64>);
    fn advance(&self, bytes: u64);
    fn end(&self);
}

pub struct NoProgress;

impl Progress for NoProgress {
    fn begin(&self, _label: &str, _total: Option<u64>) {}
    fn advance(&self, _bytes: u64) {}
    fn end(&self) {}
}

/// 同步期間不變的共用資料。
pub struct SyncCtx<'a> {
    pub http: &'a HttpClient,
    pub endpoints: &'a Endpoints,
    pub catalog: &'a Catalog,
    /// 目錄 key → `catalog_product.id`
    pub product_ids: &'a HashMap<String, i64>,
    pub index: &'a RelevanceIndex,
    pub paths: &'a DataPaths,
    pub options: &'a SyncOptions,
    pub now: DateTime<Utc>,
    pub progress: &'a dyn Progress,
    /// NVD API key（來自憑證庫；不寫入任何檔案）
    pub nvd_api_key: Option<&'a str>,
    /// NVD 請求間隔；None 時依有無 API key 決定
    pub nvd_interval: Option<Duration>,
}

impl SyncCtx<'_> {
    pub fn now_str(&self) -> String {
        self.now.to_rfc3339()
    }

    /// 以 UTC 計算的今天（cvelistV5 release 以 UTC 命名）。
    pub fn today(&self) -> NaiveDate {
        self.now.date_naive()
    }
}
