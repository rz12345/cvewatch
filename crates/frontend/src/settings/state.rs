//! 設定畫面的共用狀態與動作（TUI 與 GUI 共用）：表單與比較基準、Redmine 清單、背景工作、
//! 儲存、測試、載入清單、重新安裝排程與取回結果。
//!
//! 動作只回傳結果種類，不產生含操作方式（按鍵、按鈕名稱）的文字；各前端依結果自行說明。

use std::path::Path;
use std::sync::mpsc;

use cvewatch_core::config::Config;
use cvewatch_core::notify::redmine::Named;
use cvewatch_core::secrets::SecretKey;
use zeroize::Zeroizing;

use super::form::{self, Errors, FieldId, Form, Secret};
use super::jobs::{JobKind, JobResult, RedmineLists, Running};
use super::save;
use crate::services::Services;

pub struct SettingsState {
    pub form: Form,
    /// 上次載入或儲存時的值（判斷是否有未儲存的變更）
    pub original: Form,
    pub lists: RedmineLists,
    /// 無法載入清單的原因
    pub lists_error: Option<String>,
    pub jobs: Vec<Running>,
    /// 儲存後排程時間有變，尚未重新安裝
    pub schedule_pending: bool,
    /// 最近一次測試或安裝的結果（成功與否、說明）
    pub last_result: Option<(bool, String)>,
    /// 設定檔已手動啟用明文傳輸例外
    pub allow_insecure: bool,
}

/// 儲存的結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    /// 有 n 個欄位錯誤，未儲存
    Invalid(usize),
    /// 沒有變更
    Unchanged,
    /// 已儲存；`schedule_pending` 為排程時間有變、尚未重新安裝
    Done { schedule_pending: bool },
    /// 無法寫入憑證庫，什麼都沒寫
    SecretsFailed(String),
    /// 秘密已寫入，但設定檔寫入失敗
    ConfigFailed(String),
}

/// 背景工作沒有啟動的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// 同類工作仍在執行
    Running(JobKind),
    /// 欄位錯誤（第一個）
    Invalid(FieldId, String),
    /// 無法讀取憑證庫
    Store(String),
    /// 載入清單需要有效的 https 網址
    NeedUrl,
    /// 載入清單需要 API key
    NeedApiKey,
    /// 排程設定尚未儲存
    ScheduleUnsaved,
}

/// 背景工作完成後要告知使用者的事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finished {
    /// 測試或安裝的結果（成功與否、說明；失敗時已加上「<工作>失敗：」）
    Message { ok: bool, text: String },
    /// 已載入清單；`follow_up` 有值時接著載入該專案的 tracker 與成員
    Lists { follow_up: Option<String> },
    /// 無法載入清單
    ListsFailed(String),
}

/// 前端的操作方式（插入說明文字中），例如 TUI 的「按 Ctrl+S」、GUI 的「按「儲存」」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionWords {
    /// 儲存設定，例如「按 Ctrl+S」
    pub save: &'static str,
    /// 重新安裝排程（完整句），例如「按 i 重新安裝排程」
    pub reinstall: &'static str,
    /// 由清單挑選，例如「按 Enter 挑選」
    pub pick: &'static str,
}

impl Saved {
    /// 已儲存或沒有變更（可以離開設定畫面）。
    pub fn ok(&self) -> bool {
        matches!(self, Saved::Unchanged | Saved::Done { .. })
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Saved::SecretsFailed(_) | Saved::ConfigFailed(_))
    }

    pub fn message(&self, words: &ActionWords) -> String {
        match self {
            Saved::Invalid(n) => format!("有 {n} 個欄位錯誤（標示紅色），設定未儲存"),
            Saved::Unchanged => "設定沒有變更".into(),
            Saved::SecretsFailed(e) => format!("無法寫入 Credential Manager，設定未儲存：{e}"),
            Saved::ConfigFailed(e) => format!("密碼／API key 已存入 Credential Manager，但{e}"),
            Saved::Done {
                schedule_pending: true,
            } => format!("已儲存設定；排程已變更，{}", words.reinstall),
            Saved::Done { .. } => "已儲存設定".into(),
        }
    }
}

impl Refused {
    /// 說明；同類工作仍在執行的清單載入與排程安裝不提示（重複按下）。
    pub fn message(&self, words: &ActionWords) -> Option<String> {
        Some(match self {
            Refused::Running(
                JobKind::LoadLists | JobKind::LoadProject | JobKind::InstallSchedule,
            ) => return None,
            Refused::Running(kind) => format!("{}仍在執行", kind.label()),
            Refused::Invalid(f, e) => format!("{}：{e}（請先修正再測試）", f.label()),
            Refused::Store(e) => format!("無法讀取 Credential Manager：{e}"),
            Refused::NeedUrl => "請先填寫有效的 Redmine 網址（https）".into(),
            Refused::NeedApiKey => "請先輸入 API key".into(),
            Refused::ScheduleUnsaved => format!("排程設定尚未儲存，請先{}", words.save),
        })
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Refused::Store(_))
    }
}

impl Finished {
    pub fn message(&self, words: &ActionWords) -> String {
        match self {
            Finished::Message { text, .. } => text.clone(),
            Finished::Lists { .. } => format!("已載入 Redmine 清單，{}", words.pick),
            Finished::ListsFailed(e) => format!("無法載入 Redmine 清單：{e}；可直接輸入數字 ID"),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(
            self,
            Finished::Message { ok: false, .. } | Finished::ListsFailed(_)
        )
    }
}

impl SettingsState {
    /// 只由設定載入（秘密顯示為未設定，不讀取憑證庫）。
    pub fn new(config: &Config) -> SettingsState {
        SettingsState::with_form(Form::from_config(config), config)
    }

    /// 由設定與憑證庫載入；回傳憑證庫讀取失敗的說明。
    pub fn load(config: &Config, services: &Services) -> (SettingsState, Option<String>) {
        let (form, error) = Form::load_checked(config, services.secrets.as_ref());
        (SettingsState::with_form(form, config), error)
    }

    fn with_form(form: Form, config: &Config) -> SettingsState {
        SettingsState {
            original: form.clone(),
            form,
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

    pub fn rules(&self) -> form::Rules<'_> {
        form::Rules {
            allow_insecure: self.allow_insecure,
            original: &self.original,
        }
    }

    pub fn errors(&self) -> Errors {
        form::errors(&self.form, &self.rules())
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

    /// 驗證 → 寫入憑證庫 → 寫回設定檔（`config` 更新為儲存後的值）。
    pub fn save(&mut self, services: &Services, config: &mut Config, path: &Path) -> Saved {
        let errors = self.errors();
        if !errors.is_empty() {
            return Saved::Invalid(errors.len());
        }
        if !self.dirty() {
            return Saved::Unchanged;
        }
        // 先寫入憑證庫：失敗時什麼都沒寫；成功後秘密即成為比較基準，設定檔寫入失敗時只剩一般欄位未儲存
        if let Err(e) = save::apply_secrets(services.secrets.as_ref(), &self.form) {
            tracing::warn!(error = %e, "寫入憑證庫失敗");
            return Saved::SecretsFailed(e);
        }
        self.secrets_saved();
        if let Err(e) = save::write_config(path, &self.original, &self.form) {
            tracing::warn!(error = %e, "儲存設定失敗");
            return Saved::ConfigFailed(e);
        }
        let schedule_changed = self.form.schedule_changed(&self.original);
        *config = form::apply(&self.form, config);
        self.saved();
        tracing::info!("已儲存通知與排程設定");
        self.schedule_pending |= schedule_changed;
        Saved::Done {
            schedule_pending: self.schedule_pending,
        }
    }

    fn spawn(
        &mut self,
        services: &Services,
        kind: JobKind,
        project: Option<String>,
        job: impl FnOnce() -> JobResult + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        services.spawner.spawn(Box::new(move || {
            let _ = tx.send(job());
        }));
        self.jobs.push(Running { kind, project, rx });
    }

    /// 測試時使用的秘密：新輸入的值優先，其次為憑證庫中的值。
    fn secret(
        &self,
        services: &Services,
        which: FieldId,
    ) -> Result<Option<Zeroizing<String>>, Refused> {
        let (secret, key) = match which {
            FieldId::Password => (&self.form.smtp_password, SecretKey::SmtpPassword),
            _ => (&self.form.api_key, SecretKey::RedmineApiKey),
        };
        secret
            .effective(services.secrets.as_ref(), key)
            .map_err(Refused::Store)
    }

    /// 以目前（含未儲存）的表單值測試寄信或測試 Redmine（`kind`）；不建立議題。
    pub fn start_test(
        &mut self,
        services: &Services,
        config: &Config,
        kind: JobKind,
    ) -> Result<(), Refused> {
        let (secret_field, errors) = match kind {
            JobKind::TestEmail => (
                FieldId::Password,
                form::email_errors(&self.form, &self.rules()),
            ),
            _ => (
                FieldId::ApiKey,
                form::redmine_errors(&self.form, &self.rules()),
            ),
        };
        if self.running(kind) {
            return Err(Refused::Running(kind));
        }
        if let Some((f, e)) = errors.into_iter().next() {
            return Err(Refused::Invalid(f, e));
        }
        let secret = self.secret(services, secret_field)?;
        let config = form::apply(&self.form, config);
        let backend = services.backend.clone();
        self.spawn(services, kind, None, move || {
            JobResult::Message(match kind {
                JobKind::TestEmail => backend.test_email(&config, secret),
                _ => backend.test_redmine(&config, secret),
            })
        });
        Ok(())
    }

    /// 載入專案與優先權（`project` 有值時載入該專案的 tracker 與成員）。
    pub fn load_lists(
        &mut self,
        services: &Services,
        config: &Config,
        project: Option<String>,
    ) -> Result<(), Refused> {
        let kind = if project.is_some() {
            JobKind::LoadProject
        } else {
            JobKind::LoadLists
        };
        // 專案清單可同時載入多個（使用者連續換專案）；結果回來時只採用目前專案的
        if kind == JobKind::LoadLists && self.running(kind) {
            return Err(Refused::Running(kind));
        }
        if self.form.base_url.trim().is_empty()
            || form::redmine_errors(&self.form, &self.rules()).contains_key(&FieldId::BaseUrl)
        {
            return Err(Refused::NeedUrl);
        }
        let key = self
            .secret(services, FieldId::ApiKey)?
            .ok_or(Refused::NeedApiKey)?;
        let config = form::apply(&self.form, config);
        let backend = services.backend.clone();
        let tag = project.clone();
        self.spawn(services, kind, tag, move || {
            JobResult::Lists(backend.redmine_lists(&config, Some(key), project))
        });
        Ok(())
    }

    /// 以已儲存的設定重新安裝排程。
    pub fn reinstall_schedule(&mut self, services: &Services) -> Result<(), Refused> {
        if self.form.schedule_changed(&self.original) {
            return Err(Refused::ScheduleUnsaved);
        }
        if self.running(JobKind::InstallSchedule) {
            return Err(Refused::Running(JobKind::InstallSchedule));
        }
        let backend = services.backend.clone();
        self.spawn(services, JobKind::InstallSchedule, None, move || {
            JobResult::Message(backend.install_schedule())
        });
        Ok(())
    }

    /// 取回已完成的背景工作並更新狀態（專案已改變的清單結果捨棄）。
    pub fn poll(&mut self) -> Vec<Finished> {
        let jobs = std::mem::take(&mut self.jobs);
        let mut out = Vec::new();
        for job in jobs {
            match job.poll() {
                None => self.jobs.push(job),
                Some(result) => {
                    let stale = job.kind == JobKind::LoadProject
                        && job.project.as_deref() != Some(self.form.project.trim());
                    if !stale {
                        out.push(self.finish(job.kind, result));
                    }
                }
            }
        }
        out
    }

    fn finish(&mut self, kind: JobKind, result: JobResult) -> Finished {
        match result {
            JobResult::Message(Ok(text)) => {
                if kind == JobKind::InstallSchedule {
                    self.schedule_pending = false;
                }
                self.last_result = Some((true, text.clone()));
                Finished::Message { ok: true, text }
            }
            JobResult::Message(Err(e)) => {
                let text = format!("{}失敗：{e}", kind.label());
                self.last_result = Some((false, text.clone()));
                Finished::Message { ok: false, text }
            }
            JobResult::Lists(Ok(lists)) => {
                self.lists = self.lists.merged(lists);
                self.lists_error = None;
                let project = self.form.project.trim().to_string();
                Finished::Lists {
                    follow_up: (kind == JobKind::LoadLists && !project.is_empty())
                        .then_some(project),
                }
            }
            JobResult::Lists(Err(e)) => {
                self.lists_error = Some(e.clone());
                Finished::ListsFailed(e)
            }
        }
    }
}
