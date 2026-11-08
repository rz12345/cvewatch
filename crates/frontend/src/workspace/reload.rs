//! 重新載入與讀取命中詳情。
//!
//! 開啟中的命中詳情屬於前端的畫面狀態，因此由前端傳入開啟中的命中 id，
//! 依同樣順序取回各自的新詳情或「已不存在」（附註文字由此產生）。

use cvewatch_core::CoreError;

use super::Workspace;
use crate::data::{self, DetailData, Snapshot};
use crate::findings::ListView;
use crate::report::{Hint, Report};

/// 開啟中的命中詳情在重新載入後的結果。
#[derive(Debug, Clone)]
pub enum DetailRefresh {
    /// 換成新載入的詳情
    Updated(Box<DetailData>),
    /// 命中已不存在：關閉詳情
    Gone,
    /// 沒有重新載入（清單載入失敗或讀取詳情失敗）：保留原本的詳情
    Kept,
}

/// 會重新載入資料的操作結果：提示文字與每個開啟中詳情的更新（與傳入的 id 同順序）。
#[derive(Debug, Clone)]
#[must_use = "需要顯示結果並更新開啟中的詳情"]
pub struct Refreshed {
    pub report: Report,
    pub details: Vec<DetailRefresh>,
}

impl Refreshed {
    /// 沒有重新載入：詳情全部保留。
    pub(crate) fn unchanged(report: Report, open: &[i64]) -> Refreshed {
        Refreshed {
            report,
            details: kept(open),
        }
    }
}

impl<V: ListView> Workspace<V> {
    /// 重新載入全部資料並清除新資料提示（`open`：開啟中的命中詳情）。
    pub fn reload(&mut self, open: &[i64]) -> Refreshed {
        let (details, result) = self.reload_data(open);
        let report = match result {
            Ok(None) => Report::info("已重新載入"),
            Ok(Some(note)) => Report::info(format!("已重新載入；{note}")),
            Err(e) => Report::failure("重新載入失敗", &e),
        };
        Refreshed { report, details }
    }

    /// 寫入成功後重新載入；重新載入失敗時一併提示，避免畫面顯示過時的狀態卻無警示。
    pub(crate) fn after_write(&mut self, done: String, open: &[i64]) -> Refreshed {
        let (details, result) = self.reload_data(open);
        let report = match result {
            Ok(None) => Report::info(done),
            Ok(Some(note)) => Report::info(format!("{done}；{note}")),
            Err(e) => {
                tracing::warn!(error = %e, "寫入後重新載入失敗");
                Report::warning(format!("{done}；但重新載入失敗：{e}"))
                    .with_hint(Hint::StaleAfterWrite)
            }
        };
        Refreshed { report, details }
    }

    /// 重新載入資料、修正多選與可見列，再重新讀取開啟中的詳情；清單載入失敗時保留舊資料。
    /// 回傳需要告知使用者的附註（例如詳情的命中已不存在）。全部詳情嘗試後回報第一個錯誤。
    fn reload_data(
        &mut self,
        open: &[i64],
    ) -> (Vec<DetailRefresh>, Result<Option<String>, CoreError>) {
        let now = self.services.now();
        let data = match Snapshot::load(&self.conn, self.settings.stale_days, now) {
            Ok(d) => d,
            Err(e) => return (kept(open), Err(e)),
        };
        self.data = data;
        self.new_data = false;
        self.findings.prune_selection(&self.data);
        self.refresh_visible();
        let mut gone = Vec::new();
        let mut first_error = None;
        let details = open
            .iter()
            .map(
                |id| match data::load_detail(&self.conn, *id, self.services.link_policy) {
                    Ok(Some(fresh)) => DetailRefresh::Updated(Box::new(fresh)),
                    Ok(None) => {
                        gone.push(format!("#{id}"));
                        DetailRefresh::Gone
                    }
                    Err(e) => {
                        first_error.get_or_insert(e);
                        DetailRefresh::Kept
                    }
                },
            )
            .collect();
        let result = match first_error {
            Some(e) => Err(e),
            None => Ok((!gone.is_empty())
                .then(|| format!("命中 {} 已不存在，已關閉詳情", gone.join("、")))),
        };
        (details, result)
    }
}

impl<V> Workspace<V> {
    /// 讀取命中詳情（證據、時間軸、CVE 描述與連結）。
    pub fn detail(&self, id: i64) -> Result<DetailData, Report> {
        match data::load_detail(&self.conn, id, self.services.link_policy) {
            Ok(Some(d)) => Ok(d),
            Ok(None) => Err(Report::warning(format!("找不到命中 {id}")).with_hint(Hint::Reload)),
            Err(e) => Err(Report::failure("讀取命中失敗", &e)),
        }
    }
}

/// 每個開啟中的詳情都保留。
fn kept(open: &[i64]) -> Vec<DetailRefresh> {
    open.iter().map(|_| DetailRefresh::Kept).collect()
}
