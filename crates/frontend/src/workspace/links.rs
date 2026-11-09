//! 開啟連結（只允許 http(s)）與複製到剪貼簿。

use super::Workspace;
use crate::data;
use crate::links::Link;
use crate::report::Report;

impl<V> Workspace<V> {
    /// 單一 CVE 的連結（CVE.org、NVD、原廠公告、參考連結；只列出連結規則允許者）。
    pub fn cve_links(&self, cve: &str) -> Result<Vec<Link>, Report> {
        data::cve_links(&self.conn, cve, self.services.link_policy)
            .map_err(|e| Report::failure("讀取連結失敗", &e))
    }

    /// 此前端可開啟的連結（[`crate::services::LinkPolicy`]）。
    pub fn link_allowed(&self, url: &str) -> bool {
        self.services.link_policy.allows(url)
    }

    /// 以系統瀏覽器開啟；不符合連結規則（預設只允許 http(s)）者一律拒絕（參考連結來自外部資料）。
    pub fn open_url(&self, url: &str) -> Report {
        if !self.link_allowed(url) {
            return Report::warning(self.services.link_policy.refusal(url));
        }
        match self.services.opener.open(url) {
            Ok(()) => Report::info(format!("已開啟 {url}")),
            Err(e) => Report::error(format!("無法開啟瀏覽器：{e}")),
        }
    }

    /// 開啟資產的管理介面位址（20 design D10）：只檢查 http／https 與不含空白、控制字元，
    /// 不套用 [`crate::services::LinkPolicy`]——這是使用者自己輸入、已通過驗證的位址，
    /// 許多設備只提供 http。
    pub fn open_management_url(&self, url: &str) -> Report {
        if !crate::links::is_safe_url(url) {
            return Report::warning(format!("只能開啟 http:// 或 https:// 位址：{url}"));
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
