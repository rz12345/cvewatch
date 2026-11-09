//! `GuiApp`：持有 `Workspace` 與畫面專屬狀態。所有寫入與判斷都經 `cvewatch-frontend`，
//! 回傳的 `Report` 顯示在狀態列。

use std::sync::Arc;
use std::time::Instant;

use cvewatch_core::matching::triage::Status;
use cvewatch_core::CoreError;
use cvewatch_frontend::data::DetailData;
use cvewatch_frontend::display::{self, Marks, Section};
use cvewatch_frontend::services::DisplayTz;
use cvewatch_frontend::services::LinkPolicy;
use cvewatch_frontend::workspace::{DetailRefresh, Refreshed};
use cvewatch_frontend::{Context, ListView, Report, Workspace};
use eframe::egui;

use crate::companion::Companion;
use crate::dialogs::import::ImportFlow;
use crate::dialogs::{AssetDialog, Dialog, NotePurpose, TriageDraft};
use crate::files::FileDialogs;
use crate::matching::Matching;
use crate::settings::SettingsPage;

/// 六個畫面（側邊導覽與 `Ctrl+1`～`Ctrl+6`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Findings,
    Assets,
    Pending,
    SyncLog,
    Settings,
}

impl Screen {
    pub const ALL: [Screen; 6] = [
        Screen::Dashboard,
        Screen::Findings,
        Screen::Assets,
        Screen::Pending,
        Screen::SyncLog,
        Screen::Settings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Dashboard => "Dashboard",
            Screen::Findings => "命中",
            Screen::Assets => "資產",
            Screen::Pending => "等待區",
            Screen::SyncLog => "同步紀錄",
            Screen::Settings => "設定",
        }
    }

    /// `Ctrl+<數字>` 的數字鍵。
    pub fn key(self) -> egui::Key {
        match self {
            Screen::Dashboard => egui::Key::Num1,
            Screen::Findings => egui::Key::Num2,
            Screen::Assets => egui::Key::Num3,
            Screen::Pending => egui::Key::Num4,
            Screen::SyncLog => egui::Key::Num5,
            Screen::Settings => egui::Key::Num6,
        }
    }
}

/// 命中清單的畫面專屬狀態（游標列與 Shift 範圍選取的起點，以命中 id 記錄）。
#[derive(Debug, Clone, Default)]
pub struct FindingsView {
    pub current: Option<i64>,
    pub anchor: Option<i64>,
    pub filter_open: bool,
    /// 下一幀把焦點移到搜尋框（Ctrl+F）
    pub focus_search: bool,
    /// 下一幀捲動表格到第幾列（鍵盤移動游標）
    pub scroll_to: Option<usize>,
}

impl ListView for FindingsView {
    fn visible_changed(&mut self, _len: usize) {}
}

/// 開啟中的命中詳情。
pub struct OpenDetail {
    pub id: i64,
    pub data: Box<DetailData>,
    /// 詳情段落（開啟或重新載入時產生一次，不在每一幀重排文字）
    pub sections: Vec<Section>,
}

/// 詳情的符號（視窗版一律可顯示 Unicode）。
const MARKS: Marks = Marks {
    warn: "!",
    arrow: "→",
};

impl OpenDetail {
    pub fn new(id: i64, data: Box<DetailData>, tz: DisplayTz) -> OpenDetail {
        OpenDetail {
            sections: display::sections(&data, tz, MARKS),
            id,
            data,
        }
    }
}

/// 未儲存變更確認後要去的地方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leave {
    Screen(Screen),
    Quit,
}

/// 建立 `GuiApp` 時由啟動流程（或測試）提供的部分。
pub struct Options {
    /// 啟動警告（未知欄位、日誌無法寫入、找不到中文字型）
    pub warnings: Vec<String>,
    /// 同資料夾的 `cvewatch.exe`；None 時停用同步、匯入清冊與重新安裝排程
    pub companion: Option<Arc<dyn Companion>>,
    pub dialogs: Box<dyn FileDialogs>,
    /// 單調時鐘（新資料輪詢；測試以可推進的時鐘取代）
    pub instant: Box<dyn Fn() -> Instant>,
}

pub struct GuiApp {
    pub ws: Workspace<FindingsView>,
    pub screen: Screen,
    pub status: Option<Report>,
    pub warnings: Vec<String>,
    pub companion: Option<Arc<dyn Companion>>,
    pub dialogs: Box<dyn FileDialogs>,
    pub instant: Box<dyn Fn() -> Instant>,
    pub detail: Option<OpenDetail>,
    pub dialog: Option<Dialog>,
    pub settings: SettingsPage,
    pub assets_selected: Option<String>,
    /// 資產清單含已退役資產
    pub assets_show_retired: bool,
    /// 只顯示有版本待補元件的資產（Dashboard 的版本待補數字開啟；18 design D8）
    pub assets_pending_only: bool,
    /// 資產編輯、退役與永久刪除確認
    pub asset_dialog: Option<AssetDialog>,
    /// 資產編修後的背景比對
    pub matching: Matching,
    pub pending_selected: Option<String>,
    pub pending_links: Option<crate::screens::pending::PendingLinks>,
    pub import: Option<ImportFlow>,
    pub leave: Option<Leave>,
    /// 使用者已確認關閉（不再攔截關閉要求）
    pub allow_close: bool,
}

/// 停用按鈕的說明（spec：Companion command-line executable）。
pub const NO_COMPANION: &str =
    "需要與 cvewatch-gui.exe 放在同一資料夾的 cvewatch.exe，此功能已停用";

impl GuiApp {
    pub fn new(mut ctx: Context, options: Options) -> Result<GuiApp, CoreError> {
        // 視窗版只開啟 https 連結（spec：GUI references and clipboard）
        ctx.services.link_policy = LinkPolicy::HttpsOnly;
        let ws = Workspace::new(ctx)?;
        let (settings, store_error) = SettingsPage::load(&ws);
        let mut warnings = options.warnings;
        warnings.extend(store_error);
        Ok(GuiApp {
            ws,
            screen: Screen::Dashboard,
            status: None,
            warnings,
            companion: options.companion,
            dialogs: options.dialogs,
            instant: options.instant,
            detail: None,
            dialog: None,
            settings,
            assets_selected: None,
            assets_show_retired: false,
            assets_pending_only: false,
            asset_dialog: None,
            matching: Matching::default(),
            pending_selected: None,
            pending_links: None,
            import: None,
            leave: None,
            allow_close: false,
        })
    }

    pub fn report(&mut self, r: Report) {
        if r.level != cvewatch_frontend::Level::Info {
            tracing::info!(message = %r.message, level = ?r.level, "操作結果");
        }
        self.status = Some(r);
    }

    /// 開啟中的命中詳情（重新載入時一併更新）。
    pub fn open_ids(&self) -> Vec<i64> {
        self.detail.iter().map(|d| d.id).collect()
    }

    /// 顯示結果並更新或關閉詳情。
    pub fn apply_refreshed(&mut self, r: Refreshed) {
        if let Some(change) = r.details.into_iter().next() {
            match change {
                DetailRefresh::Updated(data) => {
                    if let Some(d) = &mut self.detail {
                        *d = OpenDetail::new(d.id, data, self.ws.services.tz);
                    }
                }
                DetailRefresh::Gone => self.detail = None,
                DetailRefresh::Kept => {}
            }
        }
        self.report(r.report);
    }

    pub fn reload(&mut self) {
        let open = self.open_ids();
        let r = self.ws.reload(&open);
        self.apply_refreshed(r);
    }

    /// 切換畫面；離開有未儲存變更的設定畫面時先確認。
    pub fn go(&mut self, screen: Screen) {
        if screen == self.screen {
            return;
        }
        if self.screen == Screen::Settings && self.settings.dirty() {
            self.leave = Some(Leave::Screen(screen));
            return;
        }
        self.enter(screen);
    }

    /// 進入畫面；進入設定畫面時重新讀取設定檔與秘密狀態（TUI、CLI 可能已修改）。
    pub fn enter(&mut self, screen: Screen) {
        self.screen = screen;
        if screen == Screen::Settings {
            self.refresh_settings();
        }
    }

    /// 沒有未儲存的變更、也沒有背景工作時，以設定檔目前的內容重新載入設定畫面。
    fn refresh_settings(&mut self) {
        if self.settings.dirty() || !self.settings.jobs.is_empty() {
            return;
        }
        let path = &self.ws.config_path;
        match std::fs::read_to_string(path) {
            Ok(text) => match cvewatch_core::config::parse(&text, path) {
                Ok((config, _unknown)) => self.ws.config = config,
                Err(e) => self.report(Report::warning(format!(
                    "設定檔目前無法解析，設定畫面顯示的是啟動時的內容：{e}"
                ))),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => self.report(Report::warning(format!(
                "無法讀取設定檔 {}：{e}",
                path.display()
            ))),
        }
        let store_error = self.settings.reload(&self.ws);
        if let Some(e) = store_error {
            self.report(Report::warning(e));
        }
    }

    /// 有未儲存的設定、未送出的備註或未儲存的資產編輯（關閉視窗前需確認）。
    pub fn has_unsaved(&self) -> bool {
        self.settings.dirty()
            || self.dialog.as_ref().is_some_and(Dialog::has_unsaved_note)
            || matches!(&self.asset_dialog, Some(AssetDialog::Edit(e)) if e.dirty())
    }

    pub fn start_sync(&mut self) {
        if self.companion.is_none() {
            self.report(Report::warning(NO_COMPANION));
            return;
        }
        let r = self.ws.start_sync();
        self.report(r);
    }

    /// 有背景工作（sync、設定連線、清冊匯入、編修後比對）時較頻繁地喚醒。
    pub fn busy(&self) -> bool {
        self.ws.sync.is_some()
            || !self.settings.jobs.is_empty()
            || self.import.as_ref().is_some_and(ImportFlow::running)
            || self.matching.running()
    }

    /// 每一幀：背景 sync 結束與新資料（`Workspace::tick` 自行節流）、設定的背景工作、匯入子程序、
    /// 編修後的比對。
    pub fn poll(&mut self) {
        let now = (self.instant)();
        if let Some(r) = self.ws.tick(now) {
            self.report(r);
        }
        self.poll_settings_jobs();
        self.poll_import();
        self.poll_match();
    }

    // ---- 命中處理 ----

    /// 游標列（仍在目前篩選與搜尋中時）。被隱藏或已不存在的游標列不作為變更對象。
    pub fn current_visible(&self) -> Option<i64> {
        let current = self.ws.findings.view.current?;
        self.ws.visible_ids().contains(&current).then_some(current)
    }

    /// 開始變更狀態：忽略先選原因，其他直接輸入備註。
    pub fn start_triage(&mut self, status: Status) {
        let current = self.current_visible();
        let targets = match self.ws.list_targets(current) {
            Ok(t) => t,
            Err(r) => return self.report(r),
        };
        if targets.is_empty() {
            return self.report(Report::info("請先選取命中"));
        }
        self.dialog = Some(if status == Status::Ignored {
            Dialog::Reason { targets }
        } else {
            Dialog::note(NotePurpose::Triage(TriageDraft {
                targets,
                status,
                reason: None,
                justification: None,
            }))
        });
    }

    pub fn open_detail(&mut self, id: i64) {
        match self.ws.detail(id) {
            Ok(data) => {
                self.detail = Some(OpenDetail::new(id, Box::new(data), self.ws.services.tz))
            }
            Err(r) => self.report(r),
        }
    }

    pub fn clear_selection(&mut self) {
        self.ws.findings.selected.clear();
        self.ws.findings.view.anchor = None;
    }

    /// 等待區的 CVE：開啟套用目標選擇。
    pub fn start_pending_apply(&mut self, cve: String) {
        match self.ws.pending_targets() {
            Ok(options) => self.dialog = Some(Dialog::PendingTarget { cve, options }),
            Err(r) => self.report(r),
        }
    }

    pub fn start_pending_dismiss(&mut self, cve: String) {
        self.dialog = Some(Dialog::note(NotePurpose::Pending {
            cve,
            action: cvewatch_core::matching::pending_ops::Action::Dismiss,
        }));
    }

    /// 開啟連結（`Workspace` 依連結規則只允許 https）。
    pub fn open_link(&mut self, url: &str) {
        let r = self.ws.open_url(url);
        self.report(r);
    }

    pub fn copy(&mut self, text: &str) {
        let r = self.ws.copy(text);
        self.report(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screens_have_distinct_keys() {
        let keys: std::collections::HashSet<_> = Screen::ALL.iter().map(|s| s.key()).collect();
        assert_eq!(keys.len(), Screen::ALL.len());
    }
}
