//! 狀態變更（與 `cvewatch finding set` 相同的 core 驗證與批次寫入；已有 Redmine 議題時
//! 同一交易排入狀態註記）。

use cvewatch_core::matching::triage::{
    self, BatchOutcome, ChangeRequest, IgnoreReason, Justification, Status,
};
use cvewatch_core::notify::triage_notes;
use cvewatch_core::CoreError;

use super::reload::Refreshed;
use super::Workspace;
use crate::findings::ListView;
use crate::report::{Hint, Report};

/// 一次狀態變更：對象命中、新狀態、忽略原因與理由、備註。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageRequest {
    pub targets: Vec<i64>,
    pub status: Status,
    pub reason: Option<IgnoreReason>,
    pub justification: Option<Justification>,
    pub note: String,
}

/// 變更的原因與理由說明，例如「（判定不受影響，需特定設定）」；沒有原因時為空字串。
pub fn change_detail(reason: Option<IgnoreReason>, justification: Option<Justification>) -> String {
    match (reason, justification) {
        (Some(r), Some(j)) => format!("（{}，{}）", r.label(), j.label()),
        (Some(r), None) => format!("（{}）", r.label()),
        _ => String::new(),
    }
}

impl<V> Workspace<V> {
    /// 清單上的變更對象：有多選時為可見的所選命中，否則為 `current`（目前列）。
    /// 所選命中都被搜尋／篩選隱藏時回報原因，不變更任何命中。
    pub fn list_targets(&self, current: Option<i64>) -> Result<Vec<i64>, Report> {
        if self.findings.selected.is_empty() {
            return Ok(current.into_iter().collect());
        }
        let targets = self.visible_selection();
        if targets.is_empty() {
            return Err(Report::warning(format!(
                "已選的 {} 筆命中目前都未顯示，未變更",
                self.findings.selected.len()
            ))
            .with_hint(Hint::RevealSelection));
        }
        Ok(targets)
    }
}

impl<V: ListView> Workspace<V> {
    /// 變更命中狀態後重新載入（`open`：開啟中的命中詳情）。略過已是該狀態的命中，
    /// 不寫入沒有變化的事件；成功時只從多選移除這次處理的命中。
    pub fn set_status(&mut self, req: TriageRequest, open: &[i64]) -> Refreshed {
        let request = ChangeRequest {
            status: req.status.as_str().to_string(),
            reason: req.reason.map(|r| r.as_str().to_string()),
            justification: req.justification.map(|j| j.as_str().to_string()),
            note: req.note,
        };
        match self.write_triage(&req.targets, &request) {
            Ok(outcome) => {
                self.findings
                    .selected
                    .retain(|id| !req.targets.contains(id));
                let label = format!(
                    "{}{}",
                    req.status.label(),
                    change_detail(req.reason, req.justification)
                );
                self.after_write(triage_message(&outcome, &label), open)
            }
            Err(e) => Refreshed::unchanged(Report::failure("變更狀態失敗", &e), open),
        }
    }

    /// 單一交易，全部成功才寫入。
    fn write_triage(
        &mut self,
        targets: &[i64],
        request: &ChangeRequest,
    ) -> Result<BatchOutcome, CoreError> {
        let change = triage::validate(request)?;
        let now = self.services.now();
        let notify = &self.config.notify;
        triage_notes::set_status_many(&mut self.conn, notify, targets, &change, now, true)
    }
}

fn triage_message(outcome: &BatchOutcome, label: &str) -> String {
    let unchanged = outcome.unchanged.len();
    match (outcome.changed.len(), unchanged) {
        (0, n) => format!("{n} 筆命中原本就是{label}，未變更"),
        (n, 0) => format!("已將 {n} 筆命中設為{label}"),
        (n, m) => format!("已將 {n} 筆命中設為{label}（{m} 筆原本就是此狀態，未變更）"),
    }
}
