//! 設定表單：由 `Config` 載入的欄位值、欄位清單、即時驗證（與 CLI 共用 core 的驗證規則）
//! 與轉回設定值（測試按鍵使用尚未儲存的值）。

use std::collections::BTreeMap;
use std::fmt;

use cvewatch_core::config::Config;
use cvewatch_core::notify::settings::{EmailConfig, NotifyConfig, RedmineConfig, SmtpSecurity};
use cvewatch_core::notify::validate;
use cvewatch_core::schedule::ScheduleConfig;
use cvewatch_core::secrets::{SecretKey, SecretStore};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Email,
    Redmine,
    Schedule,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Email, Tab::Redmine, Tab::Schedule];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Email => "Email",
            Tab::Redmine => "Redmine",
            Tab::Schedule => "排程與門檻",
        }
    }

    pub fn next(self) -> Tab {
        match self {
            Tab::Email => Tab::Redmine,
            Tab::Redmine => Tab::Schedule,
            Tab::Schedule => Tab::Email,
        }
    }

    pub fn prev(self) -> Tab {
        self.next().next()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldId {
    EmailEnabled,
    SmtpHost,
    SmtpPort,
    Security,
    Username,
    Password,
    From,
    To,
    RedmineEnabled,
    BaseUrl,
    ApiKey,
    Project,
    Tracker,
    PriorityP1,
    PriorityP2,
    Assignee,
    NoteOnTriage,
    SyncHours,
    DigestAt,
    UrgentCvss,
    SendEmpty,
}

/// 欄位的編輯方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Toggle,
    Text,
    Radio,
    Secret,
    /// 可由 Redmine 清單挑選；無法載入清單時手動輸入
    Pick,
}

impl FieldId {
    pub fn label(self) -> &'static str {
        match self {
            FieldId::EmailEnabled | FieldId::RedmineEnabled => "啟用",
            FieldId::SmtpHost => "SMTP 主機",
            FieldId::SmtpPort => "連接埠",
            FieldId::Security => "加密",
            FieldId::Username => "帳號",
            FieldId::Password => "密碼",
            FieldId::From => "寄件者",
            FieldId::To => "收件者",
            FieldId::BaseUrl => "網址",
            FieldId::ApiKey => "API key",
            FieldId::Project => "專案",
            FieldId::Tracker => "Tracker",
            FieldId::PriorityP1 => "P1 優先權",
            FieldId::PriorityP2 => "P2 優先權",
            FieldId::Assignee => "指派對象",
            FieldId::NoteOnTriage => "狀態變更時加註記",
            FieldId::SyncHours => "同步間隔（小時）",
            FieldId::DigestAt => "每日摘要時間",
            FieldId::UrgentCvss => "緊急門檻（CVSS）",
            FieldId::SendEmpty => "摘要無新事項仍寄出",
        }
    }

    pub fn kind(self) -> Kind {
        match self {
            FieldId::EmailEnabled
            | FieldId::RedmineEnabled
            | FieldId::NoteOnTriage
            | FieldId::SendEmpty => Kind::Toggle,
            FieldId::Security => Kind::Radio,
            FieldId::Password | FieldId::ApiKey => Kind::Secret,
            FieldId::Project
            | FieldId::Tracker
            | FieldId::PriorityP1
            | FieldId::PriorityP2
            | FieldId::Assignee => Kind::Pick,
            _ => Kind::Text,
        }
    }
}

/// 畫面上的一列：一般欄位、第 i 個收件者、或「新增收件者」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Field(FieldId),
    Recipient(usize),
    AddRecipient,
}

/// 秘密欄位：只保存是否已設定，或尚未儲存的新值／清除；不顯示內容。
#[derive(Clone, PartialEq, Eq)]
pub enum Secret {
    Stored(bool),
    New(Zeroizing<String>),
    Clear,
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Secret::Stored(true) => "Stored(已設定)",
            Secret::Stored(false) => "Stored(未設定)",
            Secret::New(_) => "New(●●●)",
            Secret::Clear => "Clear",
        })
    }
}

impl Secret {
    /// 「已設定」／「未設定」，尚未儲存的變更另外標示。
    pub fn status(&self) -> &'static str {
        match self {
            Secret::Stored(true) => "已設定",
            Secret::Stored(false) => "未設定",
            Secret::New(_) => "已設定（未儲存）",
            Secret::Clear => "未設定（未儲存）",
        }
    }

    /// 測試時使用的值：新輸入的值優先，其次為憑證庫中的值。
    pub fn effective(
        &self,
        store: &dyn SecretStore,
        key: SecretKey,
    ) -> Result<Option<Zeroizing<String>>, String> {
        match self {
            Secret::New(v) => Ok(Some(v.clone())),
            Secret::Clear => Ok(None),
            Secret::Stored(_) => store.get(key).map_err(|e| e.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub email_enabled: bool,
    pub smtp_host: String,
    pub smtp_port: String,
    pub security: SmtpSecurity,
    pub username: String,
    pub smtp_password: Secret,
    pub from: String,
    pub to: Vec<String>,
    pub redmine_enabled: bool,
    pub base_url: String,
    pub api_key: Secret,
    pub project: String,
    pub tracker: String,
    pub p1: String,
    pub p2: String,
    /// TUI 不編輯的其他等級對應（P3/P4），儲存時原樣保留
    pub other_priorities: BTreeMap<String, u32>,
    pub assignee: String,
    pub note_on_triage: bool,
    pub sync_hours: String,
    pub digest_at: String,
    pub urgent_cvss: String,
    pub send_empty: bool,
}

fn id_text(id: u32) -> String {
    if id == 0 {
        String::new()
    } else {
        id.to_string()
    }
}

impl Form {
    /// 由設定與憑證庫載入（只讀取秘密是否存在；憑證庫讀取失敗時視為未設定）。
    pub fn load(config: &Config, store: &dyn SecretStore) -> Form {
        Form::load_checked(config, store).0
    }

    /// 同 [`Form::load`]，並回傳憑證庫讀取失敗的說明（供畫面提示）。
    pub fn load_checked(config: &Config, store: &dyn SecretStore) -> (Form, Option<String>) {
        let mut error = None;
        let mut stored = |key: SecretKey| match store.is_set(key) {
            Ok(b) => Secret::Stored(b),
            Err(e) => {
                error.get_or_insert_with(|| format!("無法讀取 Credential Manager：{e}"));
                Secret::Stored(false)
            }
        };
        let form = Form {
            smtp_password: stored(SecretKey::SmtpPassword),
            api_key: stored(SecretKey::RedmineApiKey),
            ..Form::from_config(config)
        };
        (form, error)
    }

    /// 只由設定載入（秘密顯示為未設定，不讀取憑證庫）。
    pub fn from_config(config: &Config) -> Form {
        let n = &config.notify;
        let (e, r, s) = (&n.email, &n.redmine, &config.schedule);
        let stored = |_key: SecretKey| Secret::Stored(false);
        let priority = |p: &str| {
            r.priority_map
                .get(p)
                .copied()
                .map(id_text)
                .unwrap_or_default()
        };
        Form {
            email_enabled: e.enabled,
            smtp_host: e.smtp_host.clone(),
            smtp_port: e.smtp_port.to_string(),
            security: e.security,
            username: e.username.clone(),
            smtp_password: stored(SecretKey::SmtpPassword),
            from: e.from.clone(),
            to: e.to.clone(),
            redmine_enabled: r.enabled,
            base_url: r.base_url.clone(),
            api_key: stored(SecretKey::RedmineApiKey),
            project: r.project_id.clone(),
            tracker: id_text(r.tracker_id),
            p1: priority("P1"),
            p2: priority("P2"),
            other_priorities: r
                .priority_map
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "P1" | "P2"))
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            assignee: id_text(r.assigned_to_id),
            note_on_triage: r.add_note_on_triage,
            sync_hours: s.sync_every_hours.to_string(),
            digest_at: s.digest_at.clone(),
            urgent_cvss: format!("{:.1}", n.urgent_cvss),
            send_empty: e.send_empty,
        }
    }

    /// 各分頁的列。
    pub fn rows(&self, tab: Tab) -> Vec<Row> {
        use FieldId::*;
        let fields = |ids: &[FieldId]| ids.iter().map(|f| Row::Field(*f)).collect::<Vec<_>>();
        match tab {
            Tab::Email => {
                let mut rows = fields(&[
                    EmailEnabled,
                    SmtpHost,
                    SmtpPort,
                    Security,
                    Username,
                    Password,
                    From,
                ]);
                rows.extend((0..self.to.len()).map(Row::Recipient));
                rows.push(Row::AddRecipient);
                rows
            }
            Tab::Redmine => fields(&[
                RedmineEnabled,
                BaseUrl,
                ApiKey,
                Project,
                Tracker,
                PriorityP1,
                PriorityP2,
                Assignee,
                NoteOnTriage,
            ]),
            Tab::Schedule => fields(&[SyncHours, DigestAt, UrgentCvss, SendEmpty]),
        }
    }

    /// 文字類欄位的值（含挑選欄位的 ID）。
    pub fn text(&self, f: FieldId) -> &str {
        match f {
            FieldId::SmtpHost => &self.smtp_host,
            FieldId::SmtpPort => &self.smtp_port,
            FieldId::Username => &self.username,
            FieldId::From => &self.from,
            FieldId::BaseUrl => &self.base_url,
            FieldId::Project => &self.project,
            FieldId::Tracker => &self.tracker,
            FieldId::PriorityP1 => &self.p1,
            FieldId::PriorityP2 => &self.p2,
            FieldId::Assignee => &self.assignee,
            FieldId::SyncHours => &self.sync_hours,
            FieldId::DigestAt => &self.digest_at,
            FieldId::UrgentCvss => &self.urgent_cvss,
            _ => "",
        }
    }

    /// 回傳設定了文字欄位的新表單。
    pub fn with_text(&self, f: FieldId, value: &str) -> Form {
        let v = value.trim().to_string();
        let mut next = self.clone();
        match f {
            FieldId::SmtpHost => next.smtp_host = v,
            FieldId::SmtpPort => next.smtp_port = v,
            FieldId::Username => next.username = v,
            FieldId::From => next.from = v,
            FieldId::BaseUrl => next.base_url = v,
            FieldId::Project => next.project = v,
            FieldId::Tracker => next.tracker = v,
            FieldId::PriorityP1 => next.p1 = v,
            FieldId::PriorityP2 => next.p2 = v,
            FieldId::Assignee => next.assignee = v,
            FieldId::SyncHours => next.sync_hours = v,
            FieldId::DigestAt => next.digest_at = v,
            FieldId::UrgentCvss => next.urgent_cvss = v,
            _ => {}
        }
        next
    }

    pub fn toggled(&self, f: FieldId) -> Form {
        let mut next = self.clone();
        match f {
            FieldId::EmailEnabled => next.email_enabled = !next.email_enabled,
            FieldId::RedmineEnabled => next.redmine_enabled = !next.redmine_enabled,
            FieldId::NoteOnTriage => next.note_on_triage = !next.note_on_triage,
            FieldId::SendEmpty => next.send_empty = !next.send_empty,
            _ => {}
        }
        next
    }

    /// 加密方式只在 STARTTLS 與 TLS 間切換（TUI 不提供無加密選項）。
    pub fn cycled_security(&self) -> Form {
        Form {
            security: match self.security {
                SmtpSecurity::Starttls => SmtpSecurity::Tls,
                _ => SmtpSecurity::Starttls,
            },
            ..self.clone()
        }
    }

    pub fn with_secret(&self, f: FieldId, secret: Secret) -> Form {
        let mut next = self.clone();
        match f {
            FieldId::Password => next.smtp_password = secret,
            FieldId::ApiKey => next.api_key = secret,
            _ => {}
        }
        next
    }

    pub fn with_recipient(&self, idx: Option<usize>, value: &str) -> Form {
        let mut next = self.clone();
        let v = value.trim().to_string();
        match idx {
            Some(i) if i < next.to.len() => next.to[i] = v,
            _ => next.to.push(v),
        }
        next.to.retain(|t| !t.is_empty());
        next
    }

    pub fn without_recipient(&self, idx: usize) -> Form {
        let mut next = self.clone();
        if idx < next.to.len() {
            next.to.remove(idx);
        }
        next
    }

    /// 排程時間（間隔或摘要時間）與原本不同。
    pub fn schedule_changed(&self, original: &Form) -> bool {
        self.sync_hours.trim() != original.sync_hours.trim()
            || self.digest_at.trim() != original.digest_at.trim()
    }
}

/// 欄位錯誤（收件者錯誤記在 `FieldId::To`）。
pub type Errors = BTreeMap<FieldId, String>;

/// 驗證時需要的設定檔狀態：例外開關與原本的值（例外只適用於檔案中原有、未修改的值）。
pub struct Rules<'a> {
    pub allow_insecure: bool,
    pub original: &'a Form,
}

fn check(errors: &mut Errors, f: FieldId, r: Result<(), String>) {
    if let Err(e) = r {
        errors.entry(f).or_insert(e);
    }
}

fn required(errors: &mut Errors, enabled: bool, f: FieldId, value: &str) {
    if enabled && value.trim().is_empty() {
        errors.entry(f).or_insert_with(|| "啟用時必填".into());
    }
}

/// Email 分頁的錯誤。
pub fn email_errors(form: &Form, rules: &Rules<'_>) -> Errors {
    let mut e = Errors::new();
    check(
        &mut e,
        FieldId::SmtpPort,
        validate::port_text(&form.smtp_port).map(|_| ()),
    );
    if form.security == SmtpSecurity::None
        && !(rules.allow_insecure && rules.original.security == SmtpSecurity::None)
    {
        e.insert(FieldId::Security, "請選擇 STARTTLS 或 TLS".into());
    }
    if !form.from.trim().is_empty() {
        check(
            &mut e,
            FieldId::From,
            validate::mailbox(&form.from).map(|_| ()),
        );
    }
    for (i, to) in form.to.iter().enumerate() {
        check(
            &mut e,
            FieldId::To,
            validate::mailbox(to)
                .map(|_| ())
                .map_err(|m| format!("第 {} 位：{m}", i + 1)),
        );
    }
    required(
        &mut e,
        form.email_enabled,
        FieldId::SmtpHost,
        &form.smtp_host,
    );
    required(&mut e, form.email_enabled, FieldId::From, &form.from);
    if form.email_enabled && form.to.is_empty() {
        e.entry(FieldId::To)
            .or_insert_with(|| "啟用時至少一位收件者".into());
    }
    e
}

/// Redmine 分頁的錯誤。http 網址只在設定檔已啟用例外、且為原本的值時接受。
pub fn redmine_errors(form: &Form, rules: &Rules<'_>) -> Errors {
    let mut e = Errors::new();
    if !form.base_url.trim().is_empty() {
        let unchanged = form.base_url.trim() == rules.original.base_url.trim();
        let allow = rules.allow_insecure && unchanged;
        check(
            &mut e,
            FieldId::BaseUrl,
            validate::redmine_url(&form.base_url, allow).map(|_| ()),
        );
    }
    if !form.project.trim().is_empty() {
        check(
            &mut e,
            FieldId::Project,
            validate::project_ref(&form.project).map(|_| ()),
        );
    }
    for (f, v) in [
        (FieldId::Tracker, &form.tracker),
        (FieldId::PriorityP1, &form.p1),
        (FieldId::PriorityP2, &form.p2),
        (FieldId::Assignee, &form.assignee),
    ] {
        if !v.trim().is_empty() {
            check(&mut e, f, validate::id_text(v).map(|_| ()));
        }
    }
    required(
        &mut e,
        form.redmine_enabled,
        FieldId::BaseUrl,
        &form.base_url,
    );
    required(
        &mut e,
        form.redmine_enabled,
        FieldId::Project,
        &form.project,
    );
    if form.redmine_enabled && validate::id_text(&form.tracker).unwrap_or(0) == 0 {
        e.entry(FieldId::Tracker)
            .or_insert_with(|| "啟用時必填".into());
    }
    e
}

pub fn schedule_errors(form: &Form) -> Errors {
    let mut e = Errors::new();
    check(
        &mut e,
        FieldId::SyncHours,
        validate::sync_hours_text(&form.sync_hours).map(|_| ()),
    );
    check(
        &mut e,
        FieldId::DigestAt,
        validate::hhmm(&form.digest_at).map(|_| ()),
    );
    check(
        &mut e,
        FieldId::UrgentCvss,
        validate::threshold_text(&form.urgent_cvss).map(|_| ()),
    );
    e
}

/// 全部欄位的錯誤；有錯誤時不能儲存。
pub fn errors(form: &Form, rules: &Rules<'_>) -> Errors {
    let mut all = email_errors(form, rules);
    all.extend(redmine_errors(form, rules));
    all.extend(schedule_errors(form));
    all
}

fn id(text: &str) -> u32 {
    validate::id_text(text).unwrap_or(0)
}

/// 表單的 Email 設定（測試寄信用；呼叫前已驗證 Email 分頁）。
pub fn email_config(form: &Form) -> EmailConfig {
    EmailConfig {
        enabled: form.email_enabled,
        smtp_host: form.smtp_host.trim().to_string(),
        smtp_port: validate::port_text(&form.smtp_port).unwrap_or(0),
        security: form.security,
        username: form.username.trim().to_string(),
        from: form.from.trim().to_string(),
        to: form.to.clone(),
        send_empty: form.send_empty,
    }
}

pub fn redmine_config(form: &Form) -> RedmineConfig {
    let mut priority_map = form.other_priorities.clone();
    for (p, v) in [("P1", &form.p1), ("P2", &form.p2)] {
        if id(v) != 0 {
            priority_map.insert(p.to_string(), id(v));
        }
    }
    RedmineConfig {
        enabled: form.redmine_enabled,
        base_url: form.base_url.trim().to_string(),
        project_id: form.project.trim().to_string(),
        tracker_id: id(&form.tracker),
        priority_map,
        assigned_to_id: id(&form.assignee),
        add_note_on_triage: form.note_on_triage,
    }
}

/// 以表單值取代設定中的通知與排程（例外開關、CA 檔沿用設定檔）。
pub fn apply(form: &Form, base: &Config) -> Config {
    let mut config = base.clone();
    config.notify = NotifyConfig {
        urgent_cvss: validate::threshold_text(&form.urgent_cvss).unwrap_or(base.notify.urgent_cvss),
        email: email_config(form),
        redmine: redmine_config(form),
        ..base.notify.clone()
    };
    config.schedule = ScheduleConfig {
        sync_every_hours: validate::sync_hours_text(&form.sync_hours)
            .unwrap_or(base.schedule.sync_every_hours),
        digest_at: form.digest_at.trim().to_string(),
    };
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use cvewatch_core::secrets::MemoryStore;

    fn base() -> Form {
        let mut config = Config::default();
        config.notify.email.smtp_host = "smtp.example.edu.tw".into();
        config.notify.email.from = "a@example.edu.tw".into();
        config.notify.email.to = vec!["b@example.edu.tw".into()];
        Form::load(&config, &MemoryStore::new())
    }

    fn errs(form: &Form, original: &Form, insecure: bool) -> Errors {
        errors(
            form,
            &Rules {
                allow_insecure: insecure,
                original,
            },
        )
    }

    #[test]
    fn defaults_are_valid() {
        let f = base();
        assert!(errs(&f, &f, false).is_empty(), "{:?}", errs(&f, &f, false));
    }

    #[test]
    fn each_rule_reports_its_field() {
        let o = base();
        for (field, value) in [
            (FieldId::SmtpPort, "70000"),
            (FieldId::SmtpPort, "abc"),
            (FieldId::From, "not-an-email"),
            (FieldId::BaseUrl, "http://redmine.local"),
            (FieldId::UrgentCvss, "11"),
            (FieldId::DigestAt, "8:30"),
            (FieldId::SyncHours, "0"),
            (FieldId::Tracker, "-1"),
            (FieldId::Project, "Infra Sec"),
        ] {
            let f = o.with_text(field, value);
            assert!(
                errs(&f, &o, false).contains_key(&field),
                "{field:?} = {value}"
            );
        }
        let bad_to = o.with_recipient(None, "oops");
        assert!(errs(&bad_to, &o, false)[&FieldId::To].contains("第 2 位"));
    }

    #[test]
    fn enabled_channels_require_fields() {
        let o = base();
        let f = Form {
            email_enabled: true,
            redmine_enabled: true,
            to: Vec::new(),
            ..o.clone()
        };
        let e = errs(&f, &o, false);
        for field in [
            FieldId::To,
            FieldId::BaseUrl,
            FieldId::Project,
            FieldId::Tracker,
        ] {
            assert!(e.contains_key(&field), "{field:?}");
        }
    }

    #[test]
    fn http_and_plaintext_only_when_unchanged_with_exception() {
        let o = Form {
            base_url: "http://redmine.local".into(),
            security: SmtpSecurity::None,
            ..base()
        };
        assert!(errs(&o, &o, true).is_empty(), "設定檔例外：原值可保留");
        assert!(errs(&o, &o, false).contains_key(&FieldId::BaseUrl));
        let changed = o.with_text(FieldId::BaseUrl, "http://other.local");
        assert!(
            errs(&changed, &o, true).contains_key(&FieldId::BaseUrl),
            "TUI 不接受新的 http 網址"
        );
        assert_ne!(o.cycled_security().security, SmtpSecurity::None);
        assert_ne!(
            o.cycled_security().cycled_security().security,
            SmtpSecurity::None
        );
    }

    #[test]
    fn secrets_are_never_printed() {
        let f = base().with_secret(FieldId::Password, Secret::New(Zeroizing::new("pw!".into())));
        assert!(!format!("{f:?}").contains("pw!"));
        assert_eq!(f.smtp_password.status(), "已設定（未儲存）");
    }

    #[test]
    fn applied_config_carries_form_values() {
        let f = base()
            .with_text(FieldId::SmtpPort, "465")
            .with_text(FieldId::PriorityP1, "5")
            .with_text(FieldId::DigestAt, "07:45");
        let c = apply(&f, &Config::default());
        assert_eq!(c.notify.email.smtp_port, 465);
        assert_eq!(c.notify.redmine.priority_map.get("P1"), Some(&5));
        assert_eq!(c.schedule.digest_at, "07:45");
        assert!(f.schedule_changed(&base()));
    }
}
