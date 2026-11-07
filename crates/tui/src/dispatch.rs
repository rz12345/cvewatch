//! 執行對話框送出的命令與全域操作：呼叫 `Workspace` 的方法，把回傳的 [`Report`] 顯示在
//! 狀態列，並依 [`Refreshed`] 更新 `View` 堆疊中開啟的命中詳情（TUI 專屬的畫面狀態）。

use cvewatch_frontend::workspace::{DetailRefresh, Refreshed, TriageRequest};
use cvewatch_frontend::{Hint, Report};

use crate::app::{App, View};
use crate::dialog::{Command, Dialog, NotePurpose};
use crate::links::Link;

/// 下一步提示的 TUI 按鍵說明（接在結果文字之後）。
fn hint_text(hint: Hint) -> &'static str {
    match hint {
        Hint::StaleAfterWrite => "（畫面可能過時，按 R 重試）",
        Hint::Reload => "（請按 R 重新載入）",
        Hint::RevealSelection => "（放寬搜尋／篩選，或按 Esc 清除多選）",
    }
}

impl App {
    pub(crate) fn execute(&mut self, cmd: Command) {
        match cmd {
            Command::Triage { purpose, note } => self.apply_triage(purpose, note),
            Command::ResolvePending { cve, action, note } => {
                let open = self.open_detail_ids();
                let r = self.ws.resolve_pending(&cve, &action, &note, &open);
                self.apply_refresh(r);
            }
            Command::Open(url) => {
                let r = self.ws.open_url(&url);
                self.show(r);
            }
            Command::ApplyFilter(filter) => {
                self.ws.findings.filter = filter;
                self.ws.findings.view.cursor = 0;
                self.ws.refresh_visible();
            }
            Command::ExportVex(path) => {
                let r = self.ws.export_vex(&path);
                self.show(r);
            }
            Command::Quit => self.quit = true,
        }
    }

    /// 在狀態列顯示操作結果（到下一次按鍵為止）。
    pub(crate) fn show(&mut self, report: Report) {
        let hint = report.hint.map_or("", hint_text);
        self.message = Some(format!("{}{hint}", report.message));
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
        let request = TriageRequest {
            targets,
            status,
            reason,
            justification,
            note,
        };
        let open = self.open_detail_ids();
        let r = self.ws.set_status(request, &open);
        self.apply_refresh(r);
    }

    /// 堆疊中開啟的命中詳情（由下而上）。
    fn open_detail_ids(&self) -> Vec<i64> {
        self.stack
            .iter()
            .filter_map(|v| match v {
                View::FindingDetail { data, .. } => Some(data.finding.row.id),
                View::AssetDetail { .. } => None,
            })
            .collect()
    }

    /// 重新載入後：修正各清單游標、更新或關閉開啟中的詳情，並顯示結果。
    fn apply_refresh(&mut self, r: Refreshed) {
        let data = &self.ws.data;
        self.assets_cursor = self.assets_cursor.min(data.assets.len().saturating_sub(1));
        self.pending_cursor = self
            .pending_cursor
            .min(data.pending.len().saturating_sub(1));
        self.sync_cursor = self.sync_cursor.min(data.sync_runs.len().saturating_sub(1));
        let mut details = r.details.into_iter();
        let stack = std::mem::take(&mut self.stack);
        self.stack = stack
            .into_iter()
            .filter_map(|view| match view {
                View::FindingDetail { data, scroll } => match details.next() {
                    Some(DetailRefresh::Updated(fresh)) => Some(View::FindingDetail {
                        data: fresh,
                        scroll,
                    }),
                    Some(DetailRefresh::Gone) => None,
                    Some(DetailRefresh::Kept) | None => Some(View::FindingDetail { data, scroll }),
                },
                other => Some(other),
            })
            .collect();
        self.show(r.report);
    }

    /// `R`：重新載入全部資料並清除新資料提示。
    pub(crate) fn reload_screen(&mut self) {
        let open = self.open_detail_ids();
        let r = self.ws.reload(&open);
        self.apply_refresh(r);
    }

    pub(crate) fn open_detail(&mut self, id: i64) {
        match self.ws.detail(id) {
            Ok(d) => self.stack.push(View::FindingDetail {
                data: Box::new(d),
                scroll: 0,
            }),
            Err(r) => self.show(r),
        }
    }

    /// 有多個連結時先顯示選單。
    pub(crate) fn open_links(&mut self, links: Vec<Link>) {
        match links.as_slice() {
            [] => self.message = Some("沒有可開啟的連結".into()),
            [one] => {
                let r = self.ws.open_url(&one.url);
                self.show(r);
            }
            _ => self.dialog = Some(Dialog::Links { links, cursor: 0 }),
        }
    }

    pub(crate) fn open_links_for(&mut self, cve: &str) {
        match self.ws.cve_links(cve) {
            Ok(links) => self.open_links(links),
            Err(r) => self.show(r),
        }
    }

    pub(crate) fn copy_text(&mut self, text: &str) {
        let r = self.ws.copy(text);
        self.show(r);
    }

    /// `S`：背景啟動一次 sync；已有本程序啟動的 sync 在執行時不重複啟動。
    pub(crate) fn start_background_sync(&mut self) {
        let r = self.ws.start_sync();
        self.show(r);
    }
}
