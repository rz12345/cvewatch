//! TUI 狀態與按鍵分派（Elm 式：按鍵 → 更新狀態 → 重繪）。寫入與外部效果在 [`crate::actions`]。

use std::cell::Cell;
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use cvewatch_core::config::Config;

use cvewatch_core::matching::triage::Status;
use cvewatch_core::CoreError;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rusqlite::Connection;
use tui_input::Input;

use crate::data::{DetailData, Settings, Snapshot};
use crate::dialog::{self, moved, Dialog, NotePurpose, Outcome};
use crate::filter::{self, Filter, SortKey};
use crate::glyphs::Glyphs;
use crate::services::{RunningSync, Services};
use crate::settings::{Form, Leave, SettingsPage};

/// 檢查是否有新同步資料的間隔。
pub const NEW_DATA_POLL: Duration = Duration::from_secs(5);

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

#[derive(Debug, Clone, Default)]
pub struct FindingsState {
    pub filter: Filter,
    pub search: String,
    /// 正在輸入搜尋字串
    pub search_input: Option<Input>,
    pub sort: SortKey,
    pub cursor: usize,
    pub selected: BTreeSet<i64>,
    /// 符合條件的列（`Snapshot::findings` 的索引）
    pub visible: Vec<usize>,
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

/// 開啟 TUI 所需的一切。
pub struct Context {
    pub conn: Connection,
    pub settings: Settings,
    /// 目前的設定檔內容（設定畫面由此載入，儲存後更新）
    pub config: Config,
    /// 設定畫面寫回的設定檔
    pub config_path: PathBuf,
    pub services: Services,
    pub glyphs: Glyphs,
}

pub struct App {
    pub conn: Connection,
    pub settings: Settings,
    pub config: Config,
    pub config_path: PathBuf,
    /// 設定畫面
    pub page: SettingsPage,
    pub services: Services,
    pub glyphs: Glyphs,
    pub data: Snapshot,
    pub screen: Screen,
    pub stack: Vec<View>,
    pub findings: FindingsState,
    pub assets_cursor: usize,
    pub pending_cursor: usize,
    pub sync_cursor: usize,
    pub dialog: Option<Dialog>,
    pub help: bool,
    pub message: Option<String>,
    /// 載入後有 sync 寫入新資料（按 R 重新載入）
    pub new_data: bool,
    pub sync: Option<Box<dyn RunningSync>>,
    pub last_poll: Option<Instant>,
    pub quit: bool,
    pub offsets: ScrollOffsets,
}

impl App {
    pub fn new(ctx: Context) -> Result<App, CoreError> {
        let now = ctx.services.now();
        let data = Snapshot::load(&ctx.conn, ctx.settings.stale_days, now)?;
        let page = SettingsPage::new(&ctx.config);
        let mut app = App {
            conn: ctx.conn,
            settings: ctx.settings,
            config: ctx.config,
            config_path: ctx.config_path,
            page,
            services: ctx.services,
            glyphs: ctx.glyphs,
            data,
            screen: Screen::Dashboard,
            stack: Vec::new(),
            findings: FindingsState::default(),
            assets_cursor: 0,
            pending_cursor: 0,
            sync_cursor: 0,
            dialog: None,
            help: false,
            message: None,
            new_data: false,
            sync: None,
            last_poll: None,
            quit: false,
            offsets: ScrollOffsets::default(),
        };
        app.refresh_visible();
        Ok(app)
    }

    pub fn view(&self) -> Option<&View> {
        self.stack.last()
    }

    /// 重新計算 Findings 可見列並修正游標。多選保留（搜尋縮小再放寬後仍在），
    /// 但批次變更只套用到可見的所選命中（見 [`App::visible_selection`]）。
    pub fn refresh_visible(&mut self) {
        let f = &self.findings;
        let visible = filter::visible(
            &self.data.findings,
            &self.data.search_keys,
            &f.filter,
            &f.search,
            f.sort,
        );
        let cursor = f.cursor.min(visible.len().saturating_sub(1));
        self.findings.visible = visible;
        self.findings.cursor = cursor;
    }

    /// 目前可見的所選命中（依畫面順序）。
    pub fn visible_selection(&self) -> Vec<i64> {
        self.findings
            .visible
            .iter()
            .map(|i| self.data.findings[*i].id)
            .filter(|id| self.findings.selected.contains(id))
            .collect()
    }

    /// 重新載入後移除已不存在的命中。
    pub(crate) fn prune_selection(&mut self) {
        let ids: HashSet<i64> = self.data.findings.iter().map(|f| f.id).collect();
        self.findings.selected.retain(|id| ids.contains(id));
    }

    /// Findings 目前列的命中 id。
    pub fn current_finding(&self) -> Option<i64> {
        self.findings
            .visible
            .get(self.findings.cursor)
            .map(|i| self.data.findings[*i].id)
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
        if self.findings.search_input.is_some() {
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
        let Some(input) = self.findings.search_input.take() else {
            return;
        };
        match key.code {
            KeyCode::Enter => {}
            KeyCode::Esc => self.findings.search.clear(),
            _ => {
                let next = dialog::edit(&input, key);
                self.findings.search = next.value().to_string();
                self.findings.search_input = Some(next);
            }
        }
        self.findings.cursor = 0;
        self.refresh_visible();
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
            KeyCode::Char('R') => self.reload(),
            KeyCode::Char('S') => self.start_sync(),
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
                    self.sync_cursor = moved(self.sync_cursor, self.data.sync_runs.len(), code)
                }
                Screen::Settings => self.on_settings_key(KeyEvent::from(code)),
            },
        }
    }

    fn on_findings_key(&mut self, code: KeyCode) {
        let len = self.findings.visible.len();
        match code {
            KeyCode::Enter => {
                if let Some(id) = self.current_finding() {
                    self.open_detail(id);
                }
            }
            KeyCode::Char(' ') => self.toggle_selected(),
            KeyCode::Char('/') => {
                self.findings.search_input =
                    Some(Input::default().with_value(self.findings.search.clone()))
            }
            KeyCode::Char('f') => self.open_filter(),
            KeyCode::Char('s') => {
                self.findings.sort = self.findings.sort.next();
                self.refresh_visible();
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
                    self.copy(&cve);
                }
            }
            KeyCode::Esc => {
                self.findings.selected.clear();
                self.findings.search.clear();
                self.refresh_visible();
            }
            code => self.findings.cursor = moved(self.findings.cursor, len, code),
        }
    }

    pub fn current_row(&self) -> Option<&cvewatch_core::matching::query::FindingRow> {
        self.findings
            .visible
            .get(self.findings.cursor)
            .map(|i| &self.data.findings[*i])
    }

    fn toggle_selected(&mut self) {
        if let Some(id) = self.current_finding() {
            if !self.findings.selected.remove(&id) {
                self.findings.selected.insert(id);
            }
            let len = self.findings.visible.len();
            self.findings.cursor = moved(self.findings.cursor, len, KeyCode::Down);
        }
    }

    fn open_filter(&mut self) {
        let assets = self
            .data
            .assets
            .iter()
            .map(|a| a.asset.record.asset_tag.clone())
            .collect();
        self.dialog = Some(Dialog::Filter {
            draft: self.findings.filter.clone(),
            assets,
            cursor: 0,
        });
    }

    /// 變更狀態的對象：Findings 有多選時為所選命中，否則為目前列；詳情畫面為該命中。
    fn triage_targets(&self) -> Vec<i64> {
        match self.stack.last() {
            Some(View::FindingDetail { data, .. }) => vec![data.finding.row.id],
            Some(View::AssetDetail { .. }) => Vec::new(),
            None if !self.findings.selected.is_empty() => self.visible_selection(),
            None => self.current_finding().into_iter().collect(),
        }
    }

    fn start_triage(&mut self, status: Status) {
        let targets = self.triage_targets();
        if targets.is_empty() {
            if self.stack.is_empty() && !self.findings.selected.is_empty() {
                self.message = Some(format!(
                    "已選的 {} 筆命中目前都未顯示，未變更（放寬搜尋／篩選，或按 Esc 清除多選）",
                    self.findings.selected.len()
                ));
            }
            return;
        }
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
                    self.copy(&cve);
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
                if let Some(a) = self.data.assets.get(self.assets_cursor) {
                    self.stack.push(View::AssetDetail {
                        asset_tag: a.asset.record.asset_tag.clone(),
                        cursor: 0,
                    });
                }
            }
            code => self.assets_cursor = moved(self.assets_cursor, self.data.assets.len(), code),
        }
    }

    fn on_asset_detail_key(&mut self, asset_tag: String, cursor: usize, code: KeyCode) {
        let findings = self.data.asset_findings(&asset_tag);
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
            .data
            .pending
            .get(self.pending_cursor)
            .map(|p| p.cve_id.clone());
        match (code, cve) {
            (KeyCode::Char('a'), Some(cve)) => self.start_pending_apply(cve),
            (KeyCode::Char('d'), Some(cve)) => {
                self.dialog = Some(dialog::note_dialog(NotePurpose::Pending {
                    cve,
                    action: cvewatch_core::matching::pending_ops::Action::Dismiss,
                }))
            }
            (KeyCode::Char('o'), Some(cve)) => self.open_links_for(&cve),
            (KeyCode::Char('y'), Some(cve)) => self.copy(&cve),
            (code, _) => {
                self.pending_cursor = moved(self.pending_cursor, self.data.pending.len(), code)
            }
        }
    }

    /// 套用目標：有產品元件的使用中資產（等待區只含產品，套件由 OSV 判定）；
    /// 有多個產品元件時另列 `<資產>:<產品>`。
    fn start_pending_apply(&mut self, cve: String) {
        let options: Vec<String> = self
            .data
            .assets
            .iter()
            .flat_map(|a| {
                let tag = a.asset.record.asset_tag.clone();
                let products: Vec<String> = a
                    .components
                    .iter()
                    .filter_map(|c| match &c.kind {
                        cvewatch_core::assets::ComponentKind::Product { product_key, .. } => {
                            Some(format!("{tag}:{product_key}"))
                        }
                        _ => None,
                    })
                    .collect();
                let head = (!products.is_empty()).then_some(tag);
                let extra = if products.len() > 1 {
                    products
                } else {
                    Vec::new()
                };
                head.into_iter().chain(extra)
            })
            .collect();
        if options.is_empty() {
            self.message = Some("沒有含產品元件的使用中資產可套用".into());
            return;
        }
        self.dialog = Some(Dialog::PendingTarget {
            cve,
            options,
            cursor: 0,
        });
    }

    /// 背景 sync 結束檢查與新資料偵測（每 5 秒）。畫面需要重繪時回傳 true。
    pub fn on_tick(&mut self, now: Instant) -> bool {
        let before = (self.sync.is_some(), self.new_data, self.message.clone());
        let jobs_done = self.poll_settings_jobs();
        self.poll_sync();
        let due = self
            .last_poll
            .is_none_or(|t| now.duration_since(t) >= NEW_DATA_POLL);
        if due {
            self.last_poll = Some(now);
            self.check_new_data();
        }
        jobs_done || before != (self.sync.is_some(), self.new_data, self.message.clone())
    }

    /// 進入設定畫面：沒有未儲存的變更時重新讀取欄位與秘密狀態（Redmine 清單保留）。
    fn enter_settings(&mut self) {
        if !self.page.dirty() {
            let (form, error) = Form::load_checked(&self.config, self.services.secrets.as_ref());
            self.page.original = form.clone();
            self.page.form = form;
            if let Some(e) = error {
                tracing::warn!(error = %e, "設定畫面無法讀取憑證庫");
                self.message = Some(e);
            }
        }
    }
}
