//! 「設定」畫面（主畫面鍵 `6`）：Email、Redmine、排程與門檻三個分頁。
//!
//! - 欄位即時驗證，有錯誤時不能儲存（規則與 CLI 載入設定共用 core）
//! - Ctrl+S 以 `toml_edit` 寫回 `cvewatch.toml`（保留註解），秘密只寫入 Credential Manager
//! - 測試寄信／測試 Redmine 使用尚未儲存的表單值，在背景執行
//! - TUI 不提供無加密 SMTP、http Redmine 或明文傳輸例外開關

pub use cvewatch_frontend::settings::{form, jobs, save};
mod keys;

use cvewatch_core::config::Config;
use cvewatch_core::notify::redmine::Named;
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
    pub form: Form,
    /// 上次載入或儲存時的值（判斷是否有未儲存的變更）
    pub original: Form,
    pub editing: Option<Editing>,
    pub picker: Option<Picker>,
    pub confirm: Option<Confirm>,
    pub lists: RedmineLists,
    /// 無法載入清單的原因（此時挑選欄位改為手動輸入）
    pub lists_error: Option<String>,
    pub jobs: Vec<Running>,
    /// 儲存後排程時間有變，尚未重新安裝
    pub schedule_pending: bool,
    /// 最近一次測試或安裝的結果（成功與否、說明）
    pub last_result: Option<(bool, String)>,
    /// 設定檔已手動啟用明文傳輸例外
    pub allow_insecure: bool,
}

impl SettingsPage {
    /// 尚未讀取憑證庫（秘密顯示為未設定）；進入設定畫面時以 [`Form::load_checked`] 重新載入。
    pub fn new(config: &Config) -> Self {
        let form = Form::from_config(config);
        SettingsPage {
            tab: Tab::Email,
            cursor: 0,
            original: form.clone(),
            form,
            editing: None,
            picker: None,
            confirm: None,
            lists: RedmineLists::default(),
            lists_error: None,
            jobs: Vec::new(),
            schedule_pending: false,
            last_result: None,
            allow_insecure: config.notify.allow_insecure_transport,
        }
    }

    pub fn dirty(&self) -> bool {
        self.form != self.original
    }

    pub fn rows(&self) -> Vec<Row> {
        self.form.rows(self.tab)
    }

    pub fn current_row(&self) -> Option<Row> {
        self.rows().get(self.cursor).copied()
    }

    pub fn errors(&self) -> form::Errors {
        form::errors(
            &self.form,
            &form::Rules {
                allow_insecure: self.allow_insecure,
                original: &self.original,
            },
        )
    }

    /// 正在輸入、挑選或確認（按鍵先交給設定畫面）。
    pub fn captures_keys(&self) -> bool {
        self.editing.is_some() || self.picker.is_some() || self.confirm.is_some()
    }

    pub fn running(&self, kind: JobKind) -> bool {
        self.jobs.iter().any(|j| j.kind == kind)
    }

    /// 挑選欄位對應的清單；未載入時為 None（改為手動輸入）。
    pub fn pick_items(&self, f: FieldId) -> Option<Vec<Named>> {
        let l = &self.lists;
        match f {
            FieldId::Project => l.projects.clone(),
            FieldId::Tracker => l.trackers.clone(),
            FieldId::PriorityP1 | FieldId::PriorityP2 => l.priorities.clone(),
            FieldId::Assignee => l.members.clone().map(|m| {
                let mut items = vec![Named {
                    id: 0,
                    name: "不指派".into(),
                }];
                items.extend(m);
                items
            }),
            _ => None,
        }
    }

    /// 挑選欄位的顯示：「名稱 (#ID)」；清單中找不到時顯示原值。
    pub fn pick_label(&self, f: FieldId) -> String {
        let value = self.form.text(f).trim();
        if f == FieldId::Assignee && matches!(value, "" | "0") {
            return "不指派".into();
        }
        if value.is_empty() {
            return "（未設定）".into();
        }
        let found = value.parse::<u64>().ok().and_then(|id| {
            self.pick_items(f)
                .and_then(|items| items.into_iter().find(|n| n.id == id))
        });
        match found {
            Some(n) => n.label(),
            None if value.chars().all(|c| c.is_ascii_digit()) => format!("#{value}"),
            None => value.to_string(),
        }
    }

    /// 秘密已寫入憑證庫：表單與比較基準的秘密都改為「已設定／未設定」。
    pub fn secrets_saved(&mut self) {
        let settle = |s: &Secret| match s {
            Secret::New(_) => Secret::Stored(true),
            Secret::Clear => Secret::Stored(false),
            Secret::Stored(b) => Secret::Stored(*b),
        };
        let (password, api_key) = (settle(&self.form.smtp_password), settle(&self.form.api_key));
        self.form = Form {
            smtp_password: password.clone(),
            api_key: api_key.clone(),
            ..self.form.clone()
        };
        self.original = Form {
            smtp_password: password,
            api_key,
            ..self.original.clone()
        };
    }

    /// 儲存成功後：新值成為比較基準。
    pub fn saved(&mut self) {
        self.secrets_saved();
        self.original = self.form.clone();
    }
}
