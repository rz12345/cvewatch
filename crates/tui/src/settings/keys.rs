//! 設定畫面的按鍵與動作（儲存、測試、載入清單、重新安裝排程、未儲存確認）。

use std::sync::mpsc;

use cvewatch_core::secrets::SecretKey;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_input::Input;
use zeroize::Zeroizing;

use super::form::{self, FieldId, Kind, Row, Secret, Tab};
use super::jobs::{JobKind, JobResult, Running};
use super::{save, Confirm, Editing, Leave, Picker, CONFIRM_CHOICES};
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

    /// Ctrl+S：驗證 → 寫回設定檔 → 寫入秘密。成功時回傳 true。
    pub(crate) fn save_settings(&mut self) -> bool {
        self.page.editing = None;
        let errors = self.page.errors();
        if !errors.is_empty() {
            self.message = Some(format!(
                "有 {} 個欄位錯誤（標示紅色），設定未儲存",
                errors.len()
            ));
            return false;
        }
        if !self.page.dirty() {
            self.message = Some("設定沒有變更".into());
            return true;
        }
        // 先寫入憑證庫：失敗時什麼都沒寫；成功後秘密即成為比較基準，設定檔寫入失敗時只剩一般欄位未儲存
        if let Err(e) = save::apply_secrets(self.ws.services.secrets.as_ref(), &self.page.form) {
            tracing::warn!(error = %e, "寫入憑證庫失敗");
            self.message = Some(format!("無法寫入 Credential Manager，設定未儲存：{e}"));
            return false;
        }
        self.page.secrets_saved();
        let page = &self.page;
        if let Err(e) = save::write_config(&self.ws.config_path, &page.original, &page.form) {
            tracing::warn!(error = %e, "儲存設定失敗");
            self.message = Some(format!("密碼／API key 已存入 Credential Manager，但{e}"));
            return false;
        }
        let schedule_changed = page.form.schedule_changed(&page.original);
        self.ws.config = form::apply(&page.form, &self.ws.config);
        self.page.saved();
        tracing::info!("已儲存通知與排程設定");
        self.page.schedule_pending |= schedule_changed;
        self.message = Some(if self.page.schedule_pending {
            "已儲存設定；排程已變更，按 i 重新安裝排程".into()
        } else {
            "已儲存設定".into()
        });
        true
    }

    fn spawn_job(&mut self, kind: JobKind, job: impl FnOnce() -> JobResult + Send + 'static) {
        self.spawn_tagged(kind, None, job);
    }

    fn spawn_tagged(
        &mut self,
        kind: JobKind,
        project: Option<String>,
        job: impl FnOnce() -> JobResult + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        self.ws.services.spawner.spawn(Box::new(move || {
            let _ = tx.send(job());
        }));
        self.page.jobs.push(Running { kind, project, rx });
    }

    fn secret(&self, which: FieldId) -> Result<Option<Zeroizing<String>>, String> {
        let (secret, key) = match which {
            FieldId::Password => (&self.page.form.smtp_password, SecretKey::SmtpPassword),
            _ => (&self.page.form.api_key, SecretKey::RedmineApiKey),
        };
        secret.effective(self.ws.services.secrets.as_ref(), key)
    }

    /// `t`：以目前（含未儲存）的表單值測試。
    fn start_test(&mut self) {
        let (kind, secret_field, errors) = match self.page.tab {
            Tab::Email => (
                JobKind::TestEmail,
                FieldId::Password,
                form::email_errors(&self.page.form, &self.rules()),
            ),
            Tab::Redmine => (
                JobKind::TestRedmine,
                FieldId::ApiKey,
                form::redmine_errors(&self.page.form, &self.rules()),
            ),
            Tab::Schedule => {
                self.message =
                    Some("測試寄信請切到 Email 分頁，測試 Redmine 請切到 Redmine 分頁".into());
                return;
            }
        };
        if self.page.running(kind) {
            self.message = Some(format!("{}仍在執行", kind.label()));
            return;
        }
        if let Some((f, e)) = errors.iter().next() {
            self.message = Some(format!("{}：{e}（請先修正再測試）", f.label()));
            return;
        }
        let secret = match self.secret(secret_field) {
            Ok(s) => s,
            Err(e) => {
                self.message = Some(format!("無法讀取 Credential Manager：{e}"));
                return;
            }
        };
        let config = form::apply(&self.page.form, &self.ws.config);
        let backend = self.ws.services.backend.clone();
        self.message = Some(format!("{}中…（背景執行）", kind.label()));
        self.spawn_job(kind, move || {
            JobResult::Message(match kind {
                JobKind::TestEmail => backend.test_email(&config, secret),
                _ => backend.test_redmine(&config, secret),
            })
        });
    }

    fn rules(&self) -> form::Rules<'_> {
        form::Rules {
            allow_insecure: self.page.allow_insecure,
            original: &self.page.original,
        }
    }

    /// `l`：載入專案與優先權（`project` 有值時載入該專案的 tracker 與成員）。
    pub(crate) fn load_lists(&mut self, project: Option<String>) {
        if self.page.tab != Tab::Redmine {
            return;
        }
        let kind = if project.is_some() {
            JobKind::LoadProject
        } else {
            JobKind::LoadLists
        };
        // 專案清單可同時載入多個（使用者連續換專案）；結果回來時只採用目前專案的
        if kind == JobKind::LoadLists && self.page.running(kind) {
            return;
        }
        let f = &self.page.form;
        if f.base_url.trim().is_empty()
            || form::redmine_errors(f, &self.rules()).contains_key(&FieldId::BaseUrl)
        {
            self.message = Some("請先填寫有效的 Redmine 網址（https）".into());
            return;
        }
        let key = match self.secret(FieldId::ApiKey) {
            Ok(Some(k)) => Some(k),
            Ok(None) => {
                self.message = Some("請先輸入 API key".into());
                return;
            }
            Err(e) => {
                self.message = Some(format!("無法讀取 Credential Manager：{e}"));
                return;
            }
        };
        let config = form::apply(&self.page.form, &self.ws.config);
        let backend = self.ws.services.backend.clone();
        self.message = Some("正在載入 Redmine 清單…".into());
        let tag = project.clone();
        self.spawn_tagged(kind, tag, move || {
            JobResult::Lists(backend.redmine_lists(&config, key, project))
        });
    }

    /// `i`：以已儲存的設定重新安裝排程。
    fn reinstall_schedule(&mut self) {
        if self.page.form.schedule_changed(&self.page.original) {
            self.message = Some("排程設定尚未儲存，請先按 Ctrl+S".into());
            return;
        }
        if self.page.running(JobKind::InstallSchedule) {
            return;
        }
        let backend = self.ws.services.backend.clone();
        self.message = Some("正在重新安裝排程…".into());
        self.spawn_job(JobKind::InstallSchedule, move || {
            JobResult::Message(backend.install_schedule())
        });
    }

    /// 取回已完成的背景工作；有結果時回傳 true（需要重繪）。
    pub(crate) fn poll_settings_jobs(&mut self) -> bool {
        let jobs = std::mem::take(&mut self.page.jobs);
        let mut changed = false;
        for job in jobs {
            match job.poll() {
                None => self.page.jobs.push(job),
                Some(result) => {
                    changed = true;
                    let stale = job.kind == JobKind::LoadProject
                        && job.project.as_deref() != Some(self.page.form.project.trim());
                    if !stale {
                        self.finish_job(job.kind, result);
                    }
                }
            }
        }
        changed
    }

    fn finish_job(&mut self, kind: JobKind, result: JobResult) {
        match result {
            JobResult::Message(Ok(text)) => {
                if kind == JobKind::InstallSchedule {
                    self.page.schedule_pending = false;
                }
                self.message = Some(text.clone());
                self.page.last_result = Some((true, text));
            }
            JobResult::Message(Err(e)) => {
                let text = format!("{}失敗：{e}", kind.label());
                self.message = Some(text.clone());
                self.page.last_result = Some((false, text));
            }
            JobResult::Lists(Ok(lists)) => {
                self.page.lists = self.page.lists.merged(lists);
                self.page.lists_error = None;
                self.message = Some("已載入 Redmine 清單，按 Enter 挑選".into());
                let project = self.page.form.project.trim().to_string();
                if kind == JobKind::LoadLists && !project.is_empty() {
                    self.load_lists(Some(project));
                }
            }
            JobResult::Lists(Err(e)) => {
                self.message = Some(format!("無法載入 Redmine 清單：{e}；可直接輸入數字 ID"));
                self.page.lists_error = Some(e);
            }
        }
    }
}
