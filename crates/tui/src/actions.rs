//! 有副作用的操作：狀態變更、等待區處理、重新載入、背景 sync、開啟連結、複製。
//! 寫入一律呼叫 core 與 CLI 相同的函式，確保事件紀錄一致。

use cvewatch_core::matching::dashboard;
use cvewatch_core::matching::pending_ops::{self, Action, Resolution};
use cvewatch_core::matching::triage::{self, BatchOutcome, ChangeRequest};
use cvewatch_core::CoreError;

use crate::app::{App, View};
use crate::data::{self, Snapshot};
use crate::dialog::{change_detail, Command, Dialog, NotePurpose};
use crate::links::{self, Link};
use crate::services::sync_exit_message;

impl App {
    pub(crate) fn execute(&mut self, cmd: Command) {
        match cmd {
            Command::Triage { purpose, note } => self.apply_triage(purpose, note),
            Command::ResolvePending { cve, action, note } => {
                self.resolve_pending(&cve, &action, &note)
            }
            Command::Open(url) => self.open_url(&url),
            Command::ApplyFilter(filter) => {
                self.findings.filter = filter;
                self.findings.cursor = 0;
                self.refresh_visible();
            }
            Command::Quit => self.quit = true,
        }
    }

    fn apply_triage(&mut self, purpose: NotePurpose, note: String) {
        let NotePurpose::Triage {
            targets,
            status,
            reason,
            justification,
        } = purpose
        else {
            return;
        };
        let request = ChangeRequest {
            status: status.as_str().to_string(),
            reason: reason.map(|r| r.as_str().to_string()),
            justification: justification.map(|j| j.as_str().to_string()),
            note,
        };
        match self.write_triage(&targets, &request) {
            Ok(outcome) => {
                // 只移除這次處理的命中；從詳情變更單筆時保留清單上的多選
                self.findings.selected.retain(|id| !targets.contains(id));
                let label = format!("{}{}", status.label(), change_detail(reason, justification));
                self.report_after_write(triage_message(&outcome, &label));
            }
            Err(e) => self.fail("變更狀態失敗", &e),
        }
    }

    /// 與 `cvewatch finding set` 共用 core 的驗證與批次寫入（單一交易，全部成功才寫入）；
    /// 略過已是該狀態的命中，不寫入沒有變化的事件。
    fn write_triage(
        &mut self,
        targets: &[i64],
        request: &ChangeRequest,
    ) -> Result<BatchOutcome, CoreError> {
        let change = triage::validate(request)?;
        let now = self.services.now();
        triage::set_status_many(&mut self.conn, targets, &change, now, true)
    }

    fn resolve_pending(&mut self, cve: &str, action: &Action, note: &str) {
        let now = self.services.now();
        let result = pending_ops::resolve(
            &mut self.conn,
            &self.settings.catalog,
            &self.settings.scoring,
            cve,
            action,
            note,
            now,
        );
        match result {
            Ok(resolution) => self.report_after_write(pending_message(&resolution, action)),
            Err(e) => self.fail("等待區處理失敗", &e),
        }
    }

    fn fail(&mut self, what: &str, e: &CoreError) {
        tracing::warn!(error = %e, "{what}");
        self.message = Some(format!("{what}：{e}"));
    }

    /// 寫入成功後重新載入；重新載入失敗時一併提示，避免畫面顯示過時的狀態卻無警示。
    fn report_after_write(&mut self, done: String) {
        self.message = Some(match self.reload_data() {
            Ok(None) => done,
            Ok(Some(note)) => format!("{done}；{note}"),
            Err(e) => {
                tracing::warn!(error = %e, "寫入後重新載入失敗");
                format!("{done}；但重新載入失敗：{e}（畫面可能過時，按 R 重試）")
            }
        });
    }

    /// `R`：重新載入全部資料並清除新資料提示。
    pub(crate) fn reload(&mut self) {
        match self.reload_data() {
            Ok(None) => self.message = Some("已重新載入".into()),
            Ok(Some(note)) => self.message = Some(format!("已重新載入；{note}")),
            Err(e) => self.fail("重新載入失敗", &e),
        }
    }

    /// 重新載入資料、修正游標並更新開啟中的詳情畫面；清單載入失敗時保留舊資料。
    /// 回傳需要告知使用者的附註（例如詳情的命中已不存在而關閉）。
    pub(crate) fn reload_data(&mut self) -> Result<Option<String>, CoreError> {
        let now = self.services.now();
        let data = Snapshot::load(&self.conn, self.settings.stale_days, now)?;
        self.data = data;
        self.new_data = false;
        self.assets_cursor = self
            .assets_cursor
            .min(self.data.assets.len().saturating_sub(1));
        self.pending_cursor = self
            .pending_cursor
            .min(self.data.pending.len().saturating_sub(1));
        self.sync_cursor = self
            .sync_cursor
            .min(self.data.sync_runs.len().saturating_sub(1));
        self.prune_selection();
        self.refresh_visible();
        self.refresh_details()
    }

    /// 更新開啟中的命中詳情；命中已不存在的詳情關閉並回傳附註。全部嘗試後回報第一個錯誤。
    fn refresh_details(&mut self) -> Result<Option<String>, CoreError> {
        let stack = std::mem::take(&mut self.stack);
        let mut kept = Vec::with_capacity(stack.len());
        let mut gone = Vec::new();
        let mut first_error = None;
        for view in stack {
            let View::FindingDetail { mut data, scroll } = view else {
                kept.push(view);
                continue;
            };
            let id = data.finding.row.id;
            match data::load_detail(&self.conn, id) {
                Ok(Some(fresh)) => {
                    *data = fresh;
                    kept.push(View::FindingDetail { data, scroll });
                }
                Ok(None) => gone.push(format!("#{id}")),
                Err(e) => {
                    first_error.get_or_insert(e);
                    kept.push(View::FindingDetail { data, scroll });
                }
            }
        }
        self.stack = kept;
        if let Some(e) = first_error {
            return Err(e);
        }
        Ok((!gone.is_empty()).then(|| format!("命中 {} 已不存在，已關閉詳情", gone.join("、"))))
    }

    pub(crate) fn open_detail(&mut self, id: i64) {
        match data::load_detail(&self.conn, id) {
            Ok(Some(d)) => self.stack.push(View::FindingDetail {
                data: Box::new(d),
                scroll: 0,
            }),
            Ok(None) => self.message = Some(format!("找不到命中 {id}（請按 R 重新載入）")),
            Err(e) => self.fail("讀取命中失敗", &e),
        }
    }

    /// 有多個連結時先顯示選單。
    pub(crate) fn open_links(&mut self, links: Vec<Link>) {
        match links.as_slice() {
            [] => self.message = Some("沒有可開啟的連結".into()),
            [one] => self.open_url(&one.url.clone()),
            _ => self.dialog = Some(Dialog::Links { links, cursor: 0 }),
        }
    }

    pub(crate) fn open_links_for(&mut self, cve: &str) {
        match data::cve_links(&self.conn, cve) {
            Ok(links) => self.open_links(links),
            Err(e) => self.fail("讀取連結失敗", &e),
        }
    }

    fn open_url(&mut self, url: &str) {
        if !links::is_safe_url(url) {
            self.message = Some(format!("拒絕開啟非 http(s) 連結：{url}"));
            return;
        }
        self.message = Some(match self.services.opener.open(url) {
            Ok(()) => format!("已開啟 {url}"),
            Err(e) => format!("無法開啟瀏覽器：{e}"),
        });
    }

    pub(crate) fn copy(&mut self, text: &str) {
        self.message = Some(match self.services.clipboard.copy(text) {
            Ok(()) => format!("已複製 {text}"),
            Err(e) => format!("無法複製到剪貼簿（{e}）"),
        });
    }

    /// `S`：背景啟動一次 sync；已有本程序啟動的 sync 在執行時不重複啟動。
    pub(crate) fn start_sync(&mut self) {
        if self.sync.is_some() {
            self.message = Some("背景 sync 仍在執行".into());
            return;
        }
        match self.services.launcher.launch() {
            Ok(child) => {
                self.sync = Some(child);
                self.message = Some("已在背景啟動 sync".into());
            }
            Err(e) => self.message = Some(format!("無法啟動 sync：{e}")),
        }
    }

    pub(crate) fn poll_sync(&mut self) {
        let Some(child) = self.sync.as_mut() else {
            return;
        };
        match child.try_wait() {
            Ok(None) => {}
            Ok(Some(code)) => {
                self.sync = None;
                self.message = Some(sync_exit_message(code));
                // sync（含結尾的自動比對）已結束：立即檢查新資料，不等下一次輪詢
                self.check_new_data();
            }
            Err(e) => {
                self.sync = None;
                self.message = Some(format!("無法取得背景 sync 狀態：{e}"));
            }
        }
    }

    /// 只提示，不自動重新載入（避免使用者正在看的列跳動）。
    pub(crate) fn check_new_data(&mut self) {
        match dashboard::data_marker(&self.conn) {
            Ok(marker) => {
                if marker != self.data.marker {
                    self.new_data = true;
                }
            }
            Err(e) => tracing::warn!(error = %e, "檢查新同步資料失敗"),
        }
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

fn pending_message(r: &Resolution, action: &Action) -> String {
    match action {
        Action::Dismiss if r.ignored.is_empty() => {
            format!("{} 已移出等待區（人工判定不相關）", r.cve_id)
        }
        Action::Dismiss => format!(
            "{} 已移出等待區（人工判定不相關），一併忽略 {} 筆 probable 命中",
            r.cve_id,
            r.ignored.len()
        ),
        Action::Apply(target) => {
            let ids: Vec<String> = r
                .applied
                .iter()
                .map(|a| format!("#{}", a.finding_id))
                .collect();
            format!(
                "{} 已移出等待區並套用到 {target}（命中 {}）",
                r.cve_id,
                ids.join("、")
            )
        }
    }
}
