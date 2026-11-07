//! 開啟連結（只允許 http(s)）與複製到剪貼簿。

use super::Workspace;
use crate::data;
use crate::links::{self, Link};
use crate::report::Report;

impl<V> Workspace<V> {
    /// 單一 CVE 的連結（CVE.org、NVD、原廠公告、參考連結）。
    pub fn cve_links(&self, cve: &str) -> Result<Vec<Link>, Report> {
        data::cve_links(&self.conn, cve).map_err(|e| Report::failure("讀取連結失敗", &e))
    }

    /// 以系統瀏覽器開啟；非 http(s) 連結一律拒絕（參考連結來自外部資料）。
    pub fn open_url(&self, url: &str) -> Report {
        if !links::is_safe_url(url) {
            return Report::warning(format!("拒絕開啟非 http(s) 連結：{url}"));
        }
        match self.services.opener.open(url) {
            Ok(()) => Report::info(format!("已開啟 {url}")),
            Err(e) => Report::error(format!("無法開啟瀏覽器：{e}")),
        }
    }

    pub fn copy(&self, text: &str) -> Report {
        match self.services.clipboard.copy(text) {
            Ok(()) => Report::info(format!("已複製 {text}")),
            Err(e) => Report::error(format!("無法複製到剪貼簿（{e}）")),
        }
    }
}
