//! 設定畫面的按鍵與動作（儲存、測試、載入清單、重新安裝排程、未儲存確認）。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_input::Input;
use zeroize::Zeroizing;

use super::form::{FieldId, Kind, Row, Secret, Tab};
use super::jobs::JobKind;
use super::{ActionWords, Confirm, Editing, Finished, Leave, Picker, Refused, CONFIRM_CHOICES};
use crate::app::{App, Screen};
use crate::dialog::{edit, moved};

impl App {
    /// 設定畫面的按鍵（輸入、挑選與確認中的按鍵也由此處理）。
    pub(crate) fn on_settings_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            // 輸入中按 Ctrl+S：先套用輸入的值再儲存
            if let Some(e) = self.page.editing.take() {
                self.commit_edit(&e);
            }
            self.save_settings();
            return;
        }
        if let Some(c) = self.page.confirm.take() {
            return self.on_confirm_key(c, key.code);
        }
        if let Some(p) = self.page.picker.take() {
            return self.on_picker_key(p, key.code);
        }
        if let Some(e) = self.page.editing.take() {
            return self.on_edit_key(e, key);
        }
        self.on_settings_nav(key.code);
    }

    fn on_settings_nav(&mut self, code: KeyCode) {
        match code {
            KeyCode::Tab | KeyCode::Right => {
                self.page.tab = self.page.tab.next();
                self.page.cursor = 0;
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.page.tab = self.page.tab.prev();
                self.page.cursor = 0;
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_row(),
            KeyCode::Delete | KeyCode::Backspace => self.clear_row(),
            KeyCode::Char('t') => self.start_test(),
            KeyCode::Char('l') => self.load_lists(None),
            KeyCode::Char('i') => self.reinstall_schedule(),
            KeyCode::Esc => {
                self.leave_settings(Leave::Screen(Screen::Dashboard));
            }
            code => self.page.cursor = moved(self.page.cursor, self.page.rows().len(), code),
        }
    }

    /// 切換畫面或離開前：有未儲存的變更時先確認。回傳是否可以直接離開。
    pub(crate) fn leave_settings(&mut self, then: Leave) -> bool {
        if self.screen == Screen::Settings && self.page.dirty() {
            self.page.editing = None;
            self.page.picker = None;
            self.page.confirm = Some(Confirm { then, cursor: 0 });
            return false;
        }
        self.proceed(then);
        true
    }

    fn proceed(&mut self, then: Leave) {
        match then {
            Leave::Screen(s) => {
                self.screen = s;
                self.stack.clear();
            }
            Leave::Quit => self.quit = true,
        }
    }

    fn on_confirm_key(&mut self, c: Confirm, code: KeyCode) {
        let choice = match code {
            KeyCode::Char('s') => 0,
            KeyCode::Char('d') => 1,
            KeyCode::Char('c') | KeyCode::Esc => 2,
            KeyCode::Enter => c.cursor,
            KeyCode::Left | KeyCode::Up | KeyCode::BackTab => {
                self.page.confirm = Some(Confirm {
                    cursor: c.cursor.saturating_sub(1),
                    ..c
                });
                return;
            }
            KeyCode::Right | KeyCode::Down | KeyCode::Tab => {
                self.page.confirm = Some(Confirm {
                    cursor: (c.cursor + 1).min(CONFIRM_CHOICES.len() - 1),
                    ..c
                });
                return;
            }
            _ => {
                self.page.confirm = Some(c);
                return;
            }
        };
        match choice {
            0 => {
                if self.save_settings() {
                    self.proceed(c.then);
                }
            }
            1 => {
                self.page.form = self.page.original.clone();
                self.message = Some("已放棄未儲存的設定變更".into());
                self.proceed(c.then);
            }
            _ => {}
        }
    }

    fn on_picker_key(&mut self, p: Picker, code: KeyCode) {
        match code {
            KeyCode::Esc => {}
            KeyCode::Enter => {
                if let Some(item) = p.items.get(p.cursor) {
                    self.page.form = self.page.form.with_text(p.field, &item.id.to_string());
                    if p.field == FieldId::Project {
                        // 換專案：tracker 與成員清單需重新載入
                        self.page.lists.trackers = None;
                        self.page.lists.members = None;
                        self.load_lists(Some(item.id.to_string()));
                    }
                }
            }
            code => {
                let cursor = moved(p.cursor, p.items.len(), code);
                self.page.picker = Some(Picker { cursor, ..p });
            }
        }
    }

    fn on_edit_key(&mut self, e: Editing, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {}
            KeyCode::Enter => self.commit_edit(&e),
            _ => {
                self.page.editing = Some(Editing {
                    input: edit(&e.input, key),
                    ..e
                })
            }
        }
    }

    fn commit_edit(&mut self, e: &Editing) {
        let value = e.input.value();
        let form = &self.page.form;
        self.page.form = match e.row {
            Row::Field(f) if f.kind() == Kind::Secret => {
                if value.is_empty() {
                    return;
                }
                form.with_secret(f, Secret::New(Zeroizing::new(value.to_string())))
            }
            Row::Field(f) => form.with_text(f, value),
            Row::Recipient(i) if value.trim().is_empty() => form.without_recipient(i),
            Row::Recipient(i) => form.with_recipient(Some(i), value),
            Row::AddRecipient => form.with_recipient(None, value),
        };
    }

    fn start_edit(&mut self, row: Row, value: &str, masked: bool) {
        self.page.editing = Some(Editing {
            row,
            input: Input::default().with_value(value.to_string()),
            masked,
        });
    }

    fn activate_row(&mut self) {
        let Some(row) = self.page.current_row() else {
            return;
        };
        let form = &self.page.form;
        match row {
            Row::Recipient(i) => {
                let v = form.to.get(i).cloned().unwrap_or_default();
                self.start_edit(row, &v, false);
            }
            Row::AddRecipient => self.start_edit(row, "", false),
            Row::Field(f) => match f.kind() {
                Kind::Toggle => self.page.form = form.toggled(f),
                Kind::Radio => self.page.form = form.cycled_security(),
                Kind::Secret => self.start_edit(row, "", true),
                Kind::Text => {
                    let v = form.text(f).to_string();
                    self.start_edit(row, &v, false);
                }
                Kind::Pick => match self.page.pick_items(f) {
                    Some(items) if !items.is_empty() => {
                        let current = form.text(f).trim().parse::<u64>().unwrap_or(0);
                        let cursor = items.iter().position(|n| n.id == current).unwrap_or(0);
                        self.page.picker = Some(Picker {
                            field: f,
                            items,
                            cursor,
                        });
                    }
                    _ => {
                        let v = form.text(f).to_string();
                        self.start_edit(row, &v, false);
                    }
                },
            },
        }
    }

    fn clear_row(&mut self) {
        let form = &self.page.form;
        self.page.form = match self.page.current_row() {
            Some(Row::Field(f)) if f.kind() == Kind::Secret => form.with_secret(f, Secret::Clear),
            Some(Row::Field(f)) if matches!(f.kind(), Kind::Text | Kind::Pick) => {
                form.with_text(f, "")
            }
            Some(Row::Recipient(i)) => form.without_recipient(i),
            _ => return,
        };
    }

    /// Ctrl+S：驗證 → 寫入秘密 → 寫回設定檔（`SettingsState::save`）。成功時回傳 true。
    pub(crate) fn save_settings(&mut self) -> bool {
        self.page.editing = None;
        let ws = &mut self.ws;
        let outcome = self
            .page
            .state
            .save(&ws.services, &mut ws.config, &ws.config_path);
        self.message = Some(outcome.message(&WORDS));
        outcome.ok()
    }

    /// 背景工作沒有啟動的原因（重複按下清單載入或排程安裝時不提示）。
    fn refused(&mut self, r: Refused) {
        if let Some(text) = r.message(&WORDS) {
            self.message = Some(text);
        }
    }

    /// `t`：以目前（含未儲存）的表單值測試。
    fn start_test(&mut self) {
        let kind = match self.page.tab {
            Tab::Email => JobKind::TestEmail,
            Tab::Redmine => JobKind::TestRedmine,
            Tab::Schedule => {
                self.message =
                    Some("測試寄信請切到 Email 分頁，測試 Redmine 請切到 Redmine 分頁".into());
                return;
            }
        };
        let ws = &self.ws;
        match self.page.state.start_test(&ws.services, &ws.config, kind) {
            Ok(()) => self.message = Some(kind.started()),
            Err(r) => self.refused(r),
        }
    }

    /// `l`：載入專案與優先權（`project` 有值時載入該專案的 tracker 與成員）。
    pub(crate) fn load_lists(&mut self, project: Option<String>) {
        if self.page.tab != Tab::Redmine {
            return;
        }
        let ws = &self.ws;
        match self
            .page
            .state
            .load_lists(&ws.services, &ws.config, project)
        {
            Ok(()) => self.message = Some(JobKind::LoadLists.started()),
            Err(r) => self.refused(r),
        }
    }

    /// `i`：以已儲存的設定重新安裝排程。
    fn reinstall_schedule(&mut self) {
        match self.page.state.reinstall_schedule(&self.ws.services) {
            Ok(()) => self.message = Some(JobKind::InstallSchedule.started()),
            Err(r) => self.refused(r),
        }
    }

    /// 取回已完成的背景工作；有工作結束時回傳 true（需要重繪，含被捨棄的過時清單結果）。
    pub(crate) fn poll_settings_jobs(&mut self) -> bool {
        let before = self.page.jobs.len();
        let finished = self.page.state.poll();
        let changed = self.page.jobs.len() != before || !finished.is_empty();
        for f in finished {
            self.message = Some(f.message(&WORDS));
            if let Finished::Lists { follow_up } = f {
                if follow_up.is_some() {
                    self.load_lists(follow_up);
                }
            }
        }
        changed
    }
}

/// TUI 的操作方式（插入共用的說明文字）。
const WORDS: ActionWords = ActionWords {
    save: "按 Ctrl+S",
    reinstall: "按 i 重新安裝排程",
    pick: "按 Enter 挑選",
};
