//! 設定畫面：Email、Redmine、排程與門檻三個分頁（[`email`]、[`redmine`]、[`schedule`]），
//! 儲存與背景工作在 [`actions`]。
//!
//! 表單模型、驗證、寫回與背景工作都沿用 `cvewatch_frontend::settings`；這裡只保存畫面的
//! 編輯緩衝：文字欄位輸入中保留原樣（模型會 trim），秘密以 `Zeroizing` 緩衝（design D11）。

pub mod actions;
pub mod email;
pub mod redmine;
pub mod schedule;

use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Deref, DerefMut};

use cvewatch_frontend::settings::form::Errors;
use cvewatch_frontend::settings::{FieldId, Form, Secret, SettingsState, Tab};
use cvewatch_frontend::Workspace;
use eframe::egui;
use zeroize::{Zeroize, Zeroizing};

use crate::app::{FindingsView, GuiApp};

/// 錯誤文字的顏色。
pub const ERROR_COLOR: egui::Color32 = egui::Color32::from_rgb(0xd0, 0x30, 0x30);

pub struct SettingsPage {
    pub tab: Tab,
    /// 文字欄位的輸入內容（未建立者顯示表單值）
    pub buffers: BTreeMap<FieldId, String>,
    /// 收件者的輸入內容（含尚未填寫的空白列）
    pub recipients: Vec<String>,
    pub password: Zeroizing<String>,
    pub api_key: Zeroizing<String>,
    /// 已按「清除」、要從憑證庫刪除的秘密（輸入後再清空仍維持刪除）
    pub cleared: BTreeSet<FieldId>,
    /// 表單、清單、背景工作等與 TUI 共用的狀態（經 `Deref` 以 `page.form` 等存取）
    pub state: SettingsState,
}

impl Deref for SettingsPage {
    type Target = SettingsState;

    fn deref(&self) -> &SettingsState {
        &self.state
    }
}

impl DerefMut for SettingsPage {
    fn deref_mut(&mut self) -> &mut SettingsState {
        &mut self.state
    }
}

impl SettingsPage {
    /// 由目前設定與憑證庫載入；回傳憑證庫讀取失敗的說明。
    pub fn load(ws: &Workspace<FindingsView>) -> (SettingsPage, Option<String>) {
        let (state, error) = SettingsState::load(&ws.config, &ws.services);
        let page = SettingsPage {
            tab: Tab::Email,
            buffers: BTreeMap::new(),
            recipients: state.form.to.clone(),
            password: Zeroizing::default(),
            api_key: Zeroizing::default(),
            cleared: BTreeSet::new(),
            state,
        };
        (page, error)
    }

    /// 以目前設定與憑證庫重新載入表單（保留已載入的 Redmine 清單與最近結果）；
    /// 回傳憑證庫讀取失敗的說明。
    pub fn reload(&mut self, ws: &Workspace<FindingsView>) -> Option<String> {
        let (fresh, error) = SettingsState::load(&ws.config, &ws.services);
        let old = std::mem::replace(&mut self.state, fresh);
        self.state.lists = old.lists;
        self.state.lists_error = old.lists_error;
        self.state.last_result = old.last_result;
        self.state.schedule_pending = old.schedule_pending;
        self.reset_buffers();
        error
    }

    /// 設定文字欄位（輸入框與表單一併更新）。
    pub fn set_text(&mut self, f: FieldId, value: &str) {
        self.buffers.insert(f, value.to_string());
        self.state.form = self.state.form.with_text(f, value);
    }

    /// 收件者輸入改變：空白列不寫入表單。
    pub fn sync_recipients(&mut self) {
        let to = self
            .recipients
            .iter()
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty())
            .collect();
        self.state.form = Form {
            to,
            ..self.state.form.clone()
        };
    }

    /// 秘密輸入改變：有內容即為待寫入的新值；清空則回到原本的狀態，或已按「清除」時維持刪除。
    pub fn sync_secret(&mut self, f: FieldId) {
        let (buffer, original) = match f {
            FieldId::Password => (&self.password, &self.state.original.smtp_password),
            _ => (&self.api_key, &self.state.original.api_key),
        };
        let secret = if !buffer.is_empty() {
            Secret::New(buffer.clone())
        } else if self.cleared.contains(&f) {
            Secret::Clear
        } else {
            original.clone()
        };
        self.state.form = self.state.form.with_secret(f, secret);
    }

    /// 「清除」：憑證庫中有值時於儲存時刪除；原本就未設定時只清空輸入（不算變更）。
    pub fn clear_secret(&mut self, f: FieldId) {
        let original = match f {
            FieldId::Password => {
                self.password.zeroize();
                &self.state.original.smtp_password
            }
            _ => {
                self.api_key.zeroize();
                &self.state.original.api_key
            }
        };
        let secret = match original {
            Secret::Stored(true) => {
                self.cleared.insert(f);
                Secret::Clear
            }
            other => other.clone(),
        };
        self.state.form = self.state.form.with_secret(f, secret);
    }

    pub fn clear_secret_buffers(&mut self) {
        self.password.zeroize();
        self.api_key.zeroize();
        self.cleared.clear();
    }

    /// 編輯緩衝回到表單值；秘密緩衝清空。
    pub fn reset_buffers(&mut self) {
        self.buffers.clear();
        self.recipients = self.state.form.to.clone();
        self.clear_secret_buffers();
    }

    /// 放棄未儲存的變更。
    pub fn discard(&mut self) {
        self.state.form = self.state.original.clone();
        self.reset_buffers();
    }
}

/// 一個文字欄位：標籤、輸入框與錯誤訊息。
pub fn text_field(ui: &mut egui::Ui, page: &mut SettingsPage, f: FieldId, errors: &Errors) {
    let label = ui.label(f.label());
    let mut buffer = page
        .buffers
        .get(&f)
        .cloned()
        .unwrap_or_else(|| page.form.text(f).to_string());
    let edit = ui
        .add(egui::TextEdit::singleline(&mut buffer).desired_width(280.0))
        .labelled_by(label.id);
    if edit.changed() {
        page.set_text(f, &buffer);
    }
    ui.end_row();
    error_row(ui, f, errors);
}

/// 欄位有錯誤時在下一列以紅字顯示「欄位：原因」。
pub fn error_row(ui: &mut egui::Ui, f: FieldId, errors: &Errors) {
    if let Some(e) = errors.get(&f) {
        ui.label("");
        ui.colored_label(ERROR_COLOR, format!("{}：{e}", f.label()));
        ui.end_row();
    }
}

/// 開關欄位。
pub fn toggle_field(ui: &mut egui::Ui, page: &mut SettingsPage, f: FieldId) {
    ui.label("");
    let mut on = match f {
        FieldId::EmailEnabled => page.form.email_enabled,
        FieldId::RedmineEnabled => page.form.redmine_enabled,
        FieldId::NoteOnTriage => page.form.note_on_triage,
        _ => page.form.send_empty,
    };
    if ui.checkbox(&mut on, f.label()).changed() {
        page.form = page.form.toggled(f);
    }
    ui.end_row();
}

/// 秘密欄位：遮罩輸入，只顯示「已設定／未設定」。
pub fn secret_field(ui: &mut egui::Ui, page: &mut SettingsPage, f: FieldId) {
    let label = ui.label(f.label());
    let status = match f {
        FieldId::Password => page.form.smtp_password.status(),
        _ => page.form.api_key.status(),
    };
    let mut changed = false;
    let mut clear = false;
    ui.horizontal(|ui| {
        let buffer = match f {
            FieldId::Password => &mut page.password,
            _ => &mut page.api_key,
        };
        let edit = ui
            .add(
                egui::TextEdit::singleline(&mut **buffer)
                    .password(true)
                    .hint_text("輸入新值")
                    .desired_width(200.0),
            )
            .labelled_by(label.id);
        changed = edit.changed();
        // egui 的復原紀錄會保留輸入過的明文：每一幀清掉（也讓 Ctrl+Z 無法叫回秘密）
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), edit.id) {
            state.clear_undoer();
            state.store(ui.ctx(), edit.id);
        }
        ui.label(status);
        clear = ui.button(format!("清除{}", f.label())).clicked();
    });
    if changed {
        page.sync_secret(f);
    }
    if clear {
        page.clear_secret(f);
    }
    ui.end_row();
}

impl GuiApp {
    pub fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for tab in Tab::ALL {
                if ui
                    .selectable_label(self.settings.tab == tab, tab.title())
                    .clicked()
                {
                    self.settings.tab = tab;
                }
            }
        });
        ui.separator();
        let errors = self.settings.errors();
        egui::ScrollArea::vertical()
            .id_salt("settings-scroll")
            .auto_shrink([false, true])
            .max_height(ui.available_height() - 72.0)
            .show(ui, |ui| match self.settings.tab {
                Tab::Email => email::show(ui, &mut self.settings, &errors),
                Tab::Redmine => self.show_redmine(ui, &errors),
                Tab::Schedule => self.show_schedule(ui, &errors),
            });
        ui.separator();
        self.settings_buttons(ui, &errors);
    }

    fn settings_buttons(&mut self, ui: &mut egui::Ui, errors: &Errors) {
        ui.horizontal(|ui| {
            let can_save = self.settings.dirty() && errors.is_empty();
            if ui
                .add_enabled(can_save, egui::Button::new("儲存"))
                .on_disabled_hover_text(if errors.is_empty() {
                    "設定沒有變更"
                } else {
                    "有欄位錯誤，無法儲存"
                })
                .clicked()
            {
                self.save_settings();
            }
            if ui
                .add_enabled(self.settings.dirty(), egui::Button::new("放棄變更"))
                .clicked()
            {
                self.settings.discard();
            }
            match self.settings.tab {
                Tab::Email => self.test_button(ui, cvewatch_frontend::settings::JobKind::TestEmail),
                Tab::Redmine => {
                    self.test_button(ui, cvewatch_frontend::settings::JobKind::TestRedmine)
                }
                Tab::Schedule => {}
            }
            if !errors.is_empty() {
                ui.colored_label(ERROR_COLOR, format!("有 {} 個欄位錯誤", errors.len()));
            }
        });
        if let Some((ok, text)) = &self.settings.last_result {
            let color = if *ok {
                ui.visuals().text_color()
            } else {
                ERROR_COLOR
            };
            ui.colored_label(color, text);
        }
    }

    fn test_button(&mut self, ui: &mut egui::Ui, kind: cvewatch_frontend::settings::JobKind) {
        let running = self.settings.running(kind);
        if ui
            .add_enabled(!running, egui::Button::new(kind.label()))
            .clicked()
        {
            self.start_test(kind);
        }
    }
}
