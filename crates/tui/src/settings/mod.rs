//! 「設定」畫面（主畫面鍵 `6`）：Email、Redmine、排程與門檻三個分頁。
//!
//! - 欄位即時驗證，有錯誤時不能儲存（規則與 CLI 載入設定共用 core）
//! - Ctrl+S 以 `toml_edit` 寫回 `cvewatch.toml`（保留註解），秘密只寫入 Credential Manager
//! - 測試寄信／測試 Redmine 使用尚未儲存的表單值，在背景執行
//! - TUI 不提供無加密 SMTP、http Redmine 或明文傳輸例外開關

pub use cvewatch_frontend::settings::{form, jobs, save};
mod keys;

use std::ops::{Deref, DerefMut};

use cvewatch_core::config::Config;
use cvewatch_core::notify::redmine::Named;
pub use cvewatch_frontend::settings::{ActionWords, Finished, Refused, Saved, SettingsState};
use tui_input::Input;

pub use form::{FieldId, Form, Kind, Row, Secret, Tab};
pub use jobs::{JobKind, JobResult, NotifyBackend, RedmineLists, Running, Spawner, ThreadSpawner};

use crate::app::Screen;

/// 文字輸入中的欄位。
#[derive(Debug, Clone)]
pub struct Editing {
    pub row: Row,
    pub input: Input,
    /// 秘密欄位：畫面以遮罩顯示
    pub masked: bool,
}

/// 由 Redmine 清單挑選。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub field: FieldId,
    pub items: Vec<Named>,
    pub cursor: usize,
}

/// 未儲存變更確認後要去的地方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leave {
    Screen(Screen),
    Quit,
}

/// 儲存／放棄／取消。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Confirm {
    pub then: Leave,
    pub cursor: usize,
}

pub const CONFIRM_CHOICES: [&str; 3] = ["儲存", "放棄", "取消"];

pub struct SettingsPage {
    pub tab: Tab,
    pub cursor: usize,
    pub editing: Option<Editing>,
    pub picker: Option<Picker>,
    pub confirm: Option<Confirm>,
    /// 表單、清單、背景工作等與 GUI 共用的狀態（經 `Deref` 以 `page.form` 等存取）
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
    /// 尚未讀取憑證庫（秘密顯示為未設定）；進入設定畫面時以 [`Form::load_checked`] 重新載入。
    pub fn new(config: &Config) -> Self {
        SettingsPage {
            tab: Tab::Email,
            cursor: 0,
            editing: None,
            picker: None,
            confirm: None,
            state: SettingsState::new(config),
        }
    }

    pub fn rows(&self) -> Vec<Row> {
        self.form.rows(self.tab)
    }

    pub fn current_row(&self) -> Option<Row> {
        self.rows().get(self.cursor).copied()
    }

    /// 正在輸入、挑選或確認（按鍵先交給設定畫面）。
    pub fn captures_keys(&self) -> bool {
        self.editing.is_some() || self.picker.is_some() || self.confirm.is_some()
    }
}
