//! TUI 狀態與按鍵分派（Elm 式：按鍵 → 更新狀態 → 重繪）。資料、清單模型與有副作用的操作在
//! `cvewatch_frontend::Workspace`（[`App::ws`]）；結果文字的顯示與詳情畫面的更新在 [`crate::dispatch`]。

use std::cell::Cell;
use std::ops::{Deref, DerefMut};
use std::time::Instant;

use cvewatch_core::lifecycle::LifecycleFilter;
use cvewatch_core::matching::pending_ops::Action;
use cvewatch_core::matching::query::FindingRow;
use cvewatch_core::matching::triage::Status;
use cvewatch_core::CoreError;
use cvewatch_frontend::data::{AssetEntry, AssetListFilter};
use cvewatch_frontend::{ListView, Report, Workspace};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_input::Input;

use crate::data::DetailData;
use crate::dialog::{self, moved, Dialog, NotePurpose, Outcome};
use crate::glyphs::Glyphs;
use crate::settings::{Form, Leave, SettingsPage};

pub use cvewatch_frontend::workspace::NEW_DATA_POLL;

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
            Screen::Findings => "Findings",
            Screen::Assets => "Assets",
            Screen::Pending => "Pending",
            Screen::SyncLog => "Sync log",
            Screen::Settings => "Settings",
        }
    }

    /// 底部分頁標籤。
    pub fn tab(self) -> &'static str {
        match self {
            Screen::Dashboard => "Dash",
            Screen::Findings => "Findings",
            Screen::Assets => "Assets",
            Screen::Pending => "Pending",
            Screen::SyncLog => "Sync",
            Screen::Settings => "Settings",
        }
    }
}

/// 主畫面之上的詳情畫面（`Esc`/`h` 返回）。
#[derive(Debug, Clone)]
pub enum View {
    FindingDetail { data: Box<DetailData>, scroll: u16 },
    AssetDetail { asset_tag: String, cursor: usize },
}

/// Findings 清單的 TUI 專屬狀態（篩選、搜尋、排序、多選與可見列在 `Workspace::findings`），
/// 即 `FindingsModel::view`。tui 內部一律寫 `findings.view.<欄位>`；`FindingsModel` 的 `Deref`
/// 只為了讓既有測試的 `app.findings.cursor` 等路徑可用。
#[derive(Debug, Clone, Default)]
pub struct FindingsState {
    /// 正在輸入搜尋字串
    pub search_input: Option<Input>,
    pub cursor: usize,
}

impl ListView for FindingsState {
    /// 可見列改變後把游標修正到範圍內。
    fn visible_changed(&mut self, len: usize) {
        self.cursor = self.cursor.min(len.saturating_sub(1));
    }
}

/// 各清單的捲動位置（繪製時依游標調整；以 `Cell` 保存，繪製只需 `&App`）。
#[derive(Debug, Default)]
pub struct ScrollOffsets {
    pub findings: Cell<usize>,
    pub assets: Cell<usize>,
    pub pending: Cell<usize>,
    pub sync: Cell<usize>,
    pub asset_findings: Cell<usize>,
    /// 命中詳情可捲動的最大行數（依繪製時的寬高計算）
    pub detail_max: Cell<u16>,
}

/// 開啟 TUI 所需的一切：前端共用的 `Context` 加上框線字元。
pub struct Context {
    pub base: cvewatch_frontend::Context,
    pub glyphs: Glyphs,
}

pub struct App {
    /// 前端共用狀態；`App` 以 `Deref` 直接存取其欄位（`app.conn`、`app.data`…）
    pub ws: Workspace<FindingsState>,
    /// 設定畫面
    pub page: SettingsPage,
    pub glyphs: Glyphs,
    pub screen: Screen,
    pub stack: Vec<View>,
    pub assets_cursor: usize,
    /// 資產畫面的生命週期篩選（`l` 循環切換；None = 全部，22 design D11）
    pub assets_lifecycle_filter: Option<LifecycleFilter>,
    pub pending_cursor: usize,
    pub sync_cursor: usize,
    pub dialog: Option<Dialog>,
    pub help: bool,
    pub message: Option<String>,
    pub quit: bool,
    pub offsets: ScrollOffsets,
}

impl Deref for App {
    type Target = Workspace<FindingsState>;

    fn deref(&self) -> &Self::Target {
        &self.ws
    }
}

impl DerefMut for App {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ws
    }
}

impl App {
    pub fn new(ctx: Context) -> Result<App, CoreError> {
        let ws = Workspace::new(ctx.base)?;
        let page = SettingsPage::new(&ws.config);
        Ok(App {
            ws,
            page,
            glyphs: ctx.glyphs,
            screen: Screen::Dashboard,
            stack: Vec::new(),
            assets_cursor: 0,
            assets_lifecycle_filter: None,
            pending_cursor: 0,
            sync_cursor: 0,
            dialog: None,
            help: false,
            message: None,
            quit: false,
            offsets: ScrollOffsets::default(),
        })
    }

    pub fn view(&self) -> Option<&View> {
        self.stack.last()
    }

    /// Findings 目前列的命中 id。
    pub fn current_finding(&self) -> Option<i64> {
        self.current_row().map(|r| r.id)
    }

    pub fn current_row(&self) -> Option<&FindingRow> {
        self.ws.row_at(self.ws.findings.view.cursor)
    }

    /// 資產畫面目前列出的使用中資產（套用生命週期篩選）；游標與 Enter 以此清單為準。
    pub fn visible_assets(&self) -> Vec<&AssetEntry> {
        self.ws.data.asset_list(&AssetListFilter {
            lifecycle: self.assets_lifecycle_filter,
            ..AssetListFilter::default()
        })
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        // 訊息只顯示到下一次按鍵，之後恢復該畫面的鍵位提示
        self.message = None;
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.request_quit();
            return;
        }
        if self.help {
            self.help = false;
            return;
        }
        if let Some(d) = self.dialog.take() {
            self.on_dialog_key(d, key);
            return;
        }
        if self.ws.findings.view.search_input.is_some() {
            self.on_search_key(key);
            return;
        }
        let ctrl_s =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s');
        if self.on_settings_screen() && (self.page.captures_keys() || ctrl_s) {
            self.on_settings_key(key);
            return;
        }
        if !self.on_global_key(key.code) {
            self.on_view_key(key.code);
        }
    }

    /// 目前在設定畫面（沒有開啟詳情）。
    pub fn on_settings_screen(&self) -> bool {
        self.screen == Screen::Settings && self.stack.is_empty()
    }

    /// 有未送出的備註時先確認；確認框開啟時再按一次即離開。設定畫面有未儲存的變更時
    /// 先詢問儲存／放棄／取消（確認框開啟時再按一次即離開，不儲存）。
    fn request_quit(&mut self) {
        if self.on_settings_screen() {
            if self.page.confirm.is_some_and(|c| c.then == Leave::Quit) {
                self.quit = true;
                return;
            }
            if !self.leave_settings(Leave::Quit) {
                return;
            }
        }
        match self.dialog.take() {
            Some(Dialog::ConfirmQuit { .. }) => self.quit = true,
            Some(d) if d.has_unsaved_note() => {
                self.dialog = Some(Dialog::ConfirmQuit { back: Box::new(d) })
            }
            _ => self.quit = true,
        }
    }

    fn on_dialog_key(&mut self, d: Dialog, key: KeyEvent) {
        match d.handle(key) {
            Outcome::Keep(next) => self.dialog = Some(next),
            Outcome::Close => {}
            Outcome::Submit(cmd) => self.execute(cmd),
        }
    }

    fn on_search_key(&mut self, key: KeyEvent) {
        let Some(input) = self.ws.findings.view.search_input.take() else {
            return;
        };
        match key.code {
            KeyCode::Enter => {}
            KeyCode::Esc => self.ws.findings.search.clear(),
            _ => {
                let next = dialog::edit(&input, key);
                self.ws.findings.search = next.value().to_string();
                self.ws.findings.view.search_input = Some(next);
            }
        }
        self.ws.findings.view.cursor = 0;
        self.ws.refresh_visible();
    }

    /// 全域鍵位；已處理時回傳 true。
    fn on_global_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char(c @ '1'..='6') => {
                let target = Screen::ALL[c as usize - '1' as usize];
                if target == Screen::Settings && self.screen != Screen::Settings {
                    self.enter_settings();
                }
                if self.screen == Screen::Settings && target != Screen::Settings {
                    self.leave_settings(Leave::Screen(target));
                } else {
                    self.screen = target;
                    self.stack.clear();
                }
            }
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('q') => self.request_quit(),
            KeyCode::Char('R') => self.reload_screen(),
            KeyCode::Char('S') => self.start_background_sync(),
            _ => return false,
        }
        true
    }

    fn on_view_key(&mut self, code: KeyCode) {
        match self.stack.last() {
            Some(View::FindingDetail { scroll, .. }) => self.on_detail_key(*scroll, code),
            Some(View::AssetDetail { asset_tag, cursor }) => {
                let (tag, cursor) = (asset_tag.clone(), *cursor);
                self.on_asset_detail_key(tag, cursor, code)
            }
            None => match self.screen {
                Screen::Dashboard => {}
                Screen::Findings => self.on_findings_key(code),
                Screen::Assets => self.on_assets_key(code),
                Screen::Pending => self.on_pending_key(code),
                Screen::SyncLog => {
                    self.sync_cursor = moved(self.sync_cursor, self.ws.data.sync_runs.len(), code)
                }
                Screen::Settings => self.on_settings_key(KeyEvent::from(code)),
            },
        }
    }

    fn on_findings_key(&mut self, code: KeyCode) {
        let len = self.ws.findings.visible.len();
        match code {
            KeyCode::Enter => {
                if let Some(id) = self.current_finding() {
                    self.open_detail(id);
                }
            }
            KeyCode::Char(' ') => self.toggle_selected(),
            KeyCode::Char('/') => {
                let search = self.ws.findings.search.clone();
                self.ws.findings.view.search_input = Some(Input::default().with_value(search))
            }
            KeyCode::Char('f') => self.open_filter(),
            KeyCode::Char('s') => {
                self.ws.findings.sort = self.ws.findings.sort.next();
                self.ws.refresh_visible();
            }
            KeyCode::Char('r') => self.start_triage(Status::Resolved),
            KeyCode::Char('i') => self.start_triage(Status::Ignored),
            KeyCode::Char('u') => self.start_triage(Status::Open),
            KeyCode::Char('o') => {
                if let Some(cve) = self.current_row().map(|r| r.cve_id.clone()) {
                    self.open_links_for(&cve);
                }
            }
            KeyCode::Char('y') => {
                if let Some(cve) = self.current_row().map(|r| r.cve_id.clone()) {
                    self.copy_text(&cve);
                }
            }
            KeyCode::Char('E') => self.start_vex_export(),
            KeyCode::Esc => {
                self.ws.findings.selected.clear();
                self.ws.findings.search.clear();
                self.ws.refresh_visible();
            }
            code => self.ws.findings.view.cursor = moved(self.ws.findings.view.cursor, len, code),
        }
    }

    fn toggle_selected(&mut self) {
        if let Some(id) = self.current_finding() {
            self.ws.findings.toggle(id);
            let len = self.ws.findings.visible.len();
            self.ws.findings.view.cursor = moved(self.ws.findings.view.cursor, len, KeyCode::Down);
        }
    }

    /// `E`：以目前篩選（含搜尋）匯出 VEX；預設存到「文件」資料夾，可在對話框修改路徑。
    fn start_vex_export(&mut self) {
        match self.ws.vex_export_default() {
            Ok((path, count)) => {
                self.dialog = Some(Dialog::ExportVex {
                    input: Input::default().with_value(path.display().to_string()),
                    count,
                })
            }
            Err(r) => self.show(r),
        }
    }

    fn open_filter(&mut self) {
        let assets = self
            .ws
            .data
            .assets
            .iter()
            .map(|a| a.asset.record.asset_tag.clone())
            .collect();
        self.dialog = Some(Dialog::Filter {
            draft: self.ws.findings.filter.clone(),
            assets,
            cursor: 0,
        });
    }

    /// 變更狀態的對象：Findings 有多選時為可見的所選命中，否則為目前列；詳情畫面為該命中。
    /// 所選命中都未顯示時回報原因。
    fn triage_targets(&self) -> Result<Vec<i64>, Report> {
        match self.stack.last() {
            Some(View::FindingDetail { data, .. }) => Ok(vec![data.finding.row.id]),
            Some(View::AssetDetail { .. }) => Ok(Vec::new()),
            None => self.ws.list_targets(self.current_finding()),
        }
    }

    fn start_triage(&mut self, status: Status) {
        let targets = match self.triage_targets() {
            Ok(t) if t.is_empty() => return,
            Ok(t) => t,
            Err(r) => return self.show(r),
        };
        self.dialog = Some(match status {
            Status::Ignored => Dialog::Reason { targets, cursor: 0 },
            _ => dialog::note_dialog(NotePurpose::Triage {
                targets,
                status,
                reason: None,
                justification: None,
            }),
        });
    }

    /// 目前詳情畫面的資料。
    fn detail(&self) -> Option<&DetailData> {
        match self.stack.last() {
            Some(View::FindingDetail { data, .. }) => Some(data),
            _ => None,
        }
    }

    fn on_detail_key(&mut self, scroll: u16, code: KeyCode) {
        let scroll = match code {
            KeyCode::Up | KeyCode::Char('k') => scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => scroll.saturating_add(1),
            KeyCode::PageUp => scroll.saturating_sub(10),
            KeyCode::PageDown => scroll.saturating_add(10),
            KeyCode::Home | KeyCode::Char('g') => 0,
            KeyCode::Esc | KeyCode::Char('h') => {
                self.stack.pop();
                return;
            }
            KeyCode::Char('r') => return self.start_triage(Status::Resolved),
            KeyCode::Char('i') => return self.start_triage(Status::Ignored),
            KeyCode::Char('u') => return self.start_triage(Status::Open),
            KeyCode::Char('o') => {
                let links = self.detail().map(|d| d.links.clone()).unwrap_or_default();
                return self.open_links(links);
            }
            KeyCode::Char('y') => {
                if let Some(cve) = self.detail().map(|d| d.finding.row.cve_id.clone()) {
                    self.copy_text(&cve);
                }
                return;
            }
            _ => return,
        };
        // 不捲過內容結尾（上限於繪製時依寬高計算）
        let scroll = scroll.min(self.offsets.detail_max.get());
        if let Some(View::FindingDetail { scroll: s, .. }) = self.stack.last_mut() {
            *s = scroll;
        }
    }

    fn on_assets_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter => {
                let tag = self
                    .visible_assets()
                    .get(self.assets_cursor)
                    .map(|a| a.asset.record.asset_tag.clone());
                if let Some(asset_tag) = tag {
                    self.stack.push(View::AssetDetail {
                        asset_tag,
                        cursor: 0,
                    });
                }
            }
            KeyCode::Char('l') => {
                self.assets_lifecycle_filter = next_lifecycle_filter(self.assets_lifecycle_filter);
                self.assets_cursor = 0;
            }
            code => {
                self.assets_cursor = moved(self.assets_cursor, self.visible_assets().len(), code)
            }
        }
    }

    fn on_asset_detail_key(&mut self, asset_tag: String, cursor: usize, code: KeyCode) {
        let findings = self.ws.data.asset_findings(&asset_tag);
        match code {
            KeyCode::Esc | KeyCode::Char('h') => {
                self.stack.pop();
            }
            KeyCode::Enter => {
                if let Some(id) = findings.get(cursor).map(|f| f.id) {
                    self.open_detail(id);
                }
            }
            code => {
                let next = moved(cursor, findings.len(), code);
                if let Some(View::AssetDetail { cursor: c, .. }) = self.stack.last_mut() {
                    *c = next;
                }
            }
        }
    }

    fn on_pending_key(&mut self, code: KeyCode) {
        let cve = self
            .ws
            .data
            .pending
            .get(self.pending_cursor)
            .map(|p| p.cve_id.clone());
        match (code, cve) {
            (KeyCode::Char('a'), Some(cve)) => self.start_pending_apply(cve),
            (KeyCode::Char('d'), Some(cve)) => {
                self.dialog = Some(dialog::note_dialog(NotePurpose::Pending {
                    cve,
                    action: Action::Dismiss,
                }))
            }
            (KeyCode::Char('o'), Some(cve)) => self.open_links_for(&cve),
            (KeyCode::Char('y'), Some(cve)) => self.copy_text(&cve),
            (code, _) => {
                self.pending_cursor = moved(self.pending_cursor, self.ws.data.pending.len(), code)
            }
        }
    }

    /// 套用目標由 `Workspace::pending_targets` 決定（有產品元件的使用中資產）。
    fn start_pending_apply(&mut self, cve: String) {
        match self.ws.pending_targets() {
            Ok(options) => {
                self.dialog = Some(Dialog::PendingTarget {
                    cve,
                    options,
                    cursor: 0,
                })
            }
            Err(r) => self.show(r),
        }
    }

    /// 背景 sync 結束檢查與新資料偵測（每 5 秒）。畫面需要重繪時回傳 true。
    pub fn on_tick(&mut self, now: Instant) -> bool {
        let before = self.background_state();
        let jobs_done = self.poll_settings_jobs();
        if let Some(r) = self.ws.tick(now) {
            self.show(r);
        }
        jobs_done || before != self.background_state()
    }

    /// 背景狀態中會反映在畫面上的部分（改變即需重繪）。
    fn background_state(&self) -> (bool, bool, Option<String>) {
        (
            self.ws.sync.is_some(),
            self.ws.new_data,
            self.message.clone(),
        )
    }

    /// 進入設定畫面：沒有未儲存的變更時重新讀取欄位與秘密狀態（Redmine 清單保留）。
    fn enter_settings(&mut self) {
        if !self.page.dirty() {
            let (form, error) =
                Form::load_checked(&self.ws.config, self.ws.services.secrets.as_ref());
            self.page.original = form.clone();
            self.page.form = form;
            if let Some(e) = error {
                tracing::warn!(error = %e, "設定畫面無法讀取憑證庫");
                self.message = Some(e);
            }
        }
    }
}

/// `l` 的循環順序：全部 → 已 EoL → 即將 EoL → 保固或合約到期 → 全部。
pub fn next_lifecycle_filter(current: Option<LifecycleFilter>) -> Option<LifecycleFilter> {
    let all = LifecycleFilter::ALL;
    match current {
        None => all.first().copied(),
        Some(f) => all
            .iter()
            .position(|x| *x == f)
            .and_then(|i| all.get(i + 1))
            .copied(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cvewatch_frontend::FindingsModel;

    #[test]
    fn lifecycle_filter_cycles_through_all_and_back() {
        let mut f = None;
        let mut seen = Vec::new();
        for _ in 0..4 {
            f = next_lifecycle_filter(f);
            seen.push(f);
        }
        assert_eq!(
            seen,
            [
                Some(LifecycleFilter::Eol),
                Some(LifecycleFilter::EolSoon),
                Some(LifecycleFilter::Expiry),
                None
            ]
        );
    }

    /// 既有測試的 `app.findings.cursor` 等路徑經 `FindingsModel` 的 `Deref` 指向 `FindingsState`。
    /// 兩邊新增欄位時這裡無法編譯（完整解構），提醒確認沒有同名欄位：同名時 `Deref` 的路徑會
    /// 靜默改指向模型的欄位。
    #[test]
    fn findings_model_fields_do_not_shadow_view_state() {
        let FindingsModel {
            filter: _,
            search: _,
            sort: _,
            selected: _,
            visible: _,
            view,
        } = FindingsModel::<FindingsState>::default();
        let FindingsState {
            search_input: _,
            cursor: _,
        } = view;
    }
}
