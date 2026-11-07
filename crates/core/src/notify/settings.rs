//! `[notify]`、`[notify.email]`、`[notify.redmine]` 設定。
//!
//! 格式與安全性錯誤（連接埠、Email 格式、門檻、未啟用例外卻使用明文傳輸）在載入時拒絕（結束碼 2）；
//! 啟用但尚未填妥的欄位（例如沒有 SMTP 主機）不阻擋載入，於送出、測試與 `config check` 時說明。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use super::validate;
use crate::matching::scoring::Priority;

pub const DEFAULT_URGENT_CVSS: f64 = 9.0;
pub const DEFAULT_SMTP_PORT: u16 = 587;

string_enum!(
    /// SMTP 加密方式。
    SmtpSecurity {
        Starttls => "starttls",
        Tls => "tls",
        None => "none",
    }
);

impl SmtpSecurity {
    pub fn label(self) -> &'static str {
        match self {
            SmtpSecurity::Starttls => "STARTTLS",
            SmtpSecurity::Tls => "TLS",
            SmtpSecurity::None => "無加密",
        }
    }

    pub fn is_encrypted(self) -> bool {
        self != SmtpSecurity::None
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "RawNotify")]
pub struct NotifyConfig {
    /// 與 KEV 一起定義「緊急」：顯示用 CVSS ≥ 此值
    pub urgent_cvss: f64,
    /// 明文傳輸例外（只能手動編輯設定檔）
    pub allow_insecure_transport: bool,
    /// 機構自簽 CA 的 PEM 檔（與系統根憑證並用）
    pub ca_file: Option<PathBuf>,
    pub email: EmailConfig,
    pub redmine: RedmineConfig,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        NotifyConfig {
            urgent_cvss: DEFAULT_URGENT_CVSS,
            allow_insecure_transport: false,
            ca_file: None,
            email: EmailConfig::default(),
            redmine: RedmineConfig::default(),
        }
    }
}

impl NotifyConfig {
    pub fn ca_file(&self) -> Option<&Path> {
        self.ca_file.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    pub enabled: bool,
    pub smtp_host: String,
    #[serde(deserialize_with = "port")]
    pub smtp_port: u16,
    #[serde(deserialize_with = "security")]
    pub security: SmtpSecurity,
    /// 空字串表示不驗證；密碼取自 Credential Manager（smtp_password）
    pub username: String,
    pub from: String,
    pub to: Vec<String>,
    /// 每日摘要沒有新事項時仍寄出「無新事項」
    pub send_empty: bool,
}

impl Default for EmailConfig {
    fn default() -> Self {
        EmailConfig {
            enabled: false,
            smtp_host: String::new(),
            smtp_port: DEFAULT_SMTP_PORT,
            security: SmtpSecurity::Starttls,
            username: String::new(),
            from: String::new(),
            to: Vec::new(),
            send_empty: true,
        }
    }
}

impl EmailConfig {
    /// 寄信必要欄位都已填寫；否則回傳缺少的欄位說明。
    pub fn ready(&self) -> Result<(), String> {
        let mut missing = Vec::new();
        if self.smtp_host.trim().is_empty() {
            missing.push("SMTP 主機（smtp_host）");
        }
        if self.from.trim().is_empty() {
            missing.push("寄件者（from）");
        }
        if self.to.is_empty() {
            missing.push("收件者（to）");
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!("Email 設定不完整：缺少{}", missing.join("、")))
        }
    }

    /// 是否需要 SMTP 驗證。
    pub fn authenticates(&self) -> bool {
        !self.username.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RedmineConfig {
    pub enabled: bool,
    pub base_url: String,
    /// 數字 ID 或識別碼（設定檔可寫數字或字串）
    #[serde(deserialize_with = "project")]
    pub project_id: String,
    pub tracker_id: u32,
    /// 等級 → Redmine priority id（未對應的等級使用 Redmine 預設優先權）
    #[serde(deserialize_with = "priority_map")]
    pub priority_map: BTreeMap<String, u32>,
    /// 0 = 不指派
    pub assigned_to_id: u32,
    pub add_note_on_triage: bool,
}

impl Default for RedmineConfig {
    fn default() -> Self {
        RedmineConfig {
            enabled: false,
            base_url: String::new(),
            project_id: String::new(),
            tracker_id: 0,
            priority_map: BTreeMap::new(),
            assigned_to_id: 0,
            add_note_on_triage: true,
        }
    }
}

impl RedmineConfig {
    pub fn ready(&self) -> Result<(), String> {
        let mut missing = Vec::new();
        if self.base_url.trim().is_empty() {
            missing.push("網址（base_url）");
        }
        if self.project_id.trim().is_empty() {
            missing.push("專案（project_id）");
        }
        if self.tracker_id == 0 {
            missing.push("tracker（tracker_id）");
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!("Redmine 設定不完整：缺少{}", missing.join("、")))
        }
    }

    pub fn priority_id(&self, priority: Priority) -> Option<u32> {
        self.priority_map.get(priority.as_str()).copied()
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct RawNotify {
    urgent_cvss: f64,
    allow_insecure_transport: bool,
    ca_file: String,
    email: EmailConfig,
    redmine: RedmineConfig,
}

impl Default for RawNotify {
    fn default() -> Self {
        let d = NotifyConfig::default();
        RawNotify {
            urgent_cvss: d.urgent_cvss,
            allow_insecure_transport: d.allow_insecure_transport,
            ca_file: String::new(),
            email: d.email,
            redmine: d.redmine,
        }
    }
}

impl TryFrom<RawNotify> for NotifyConfig {
    type Error = String;

    fn try_from(raw: RawNotify) -> Result<Self, Self::Error> {
        let urgent_cvss =
            validate::threshold(raw.urgent_cvss).map_err(|e| format!("urgent_cvss：{e}"))?;
        check_transport(&raw.email, &raw.redmine, raw.allow_insecure_transport)?;
        check_addresses(&raw.email)?;
        let ca = raw.ca_file.trim();
        Ok(NotifyConfig {
            urgent_cvss,
            allow_insecure_transport: raw.allow_insecure_transport,
            ca_file: (!ca.is_empty()).then(|| PathBuf::from(ca)),
            email: raw.email,
            redmine: raw.redmine,
        })
    }
}

/// 明文傳輸只在例外開關啟用時接受。
fn check_transport(
    email: &EmailConfig,
    redmine: &RedmineConfig,
    allow_insecure: bool,
) -> Result<(), String> {
    if email.security == SmtpSecurity::None && !allow_insecure {
        return Err(
            "email.security = \"none\"（無加密）需在設定檔手動設定 allow_insecure_transport = true"
                .into(),
        );
    }
    if !redmine.base_url.trim().is_empty() {
        validate::redmine_url(&redmine.base_url, allow_insecure)
            .map_err(|e| format!("redmine.base_url：{e}"))?;
    }
    Ok(())
}

fn check_addresses(email: &EmailConfig) -> Result<(), String> {
    if !email.from.trim().is_empty() {
        validate::mailbox(&email.from).map_err(|e| format!("email.from：{e}"))?;
    }
    for to in &email.to {
        validate::mailbox(to).map_err(|e| format!("email.to：{e}"))?;
    }
    Ok(())
}

fn port<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    validate::port(i64::deserialize(d)?).map_err(D::Error::custom)
}

fn security<'de, D: Deserializer<'de>>(d: D) -> Result<SmtpSecurity, D::Error> {
    let s = String::deserialize(d)?;
    SmtpSecurity::parse(&s).ok_or_else(|| {
        D::Error::custom(format!(
            "未知的加密方式 `{s}`；可用：{}",
            SmtpSecurity::allowed()
        ))
    })
}

fn project<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IdOrName {
        Id(u64),
        Name(String),
    }
    let text = match IdOrName::deserialize(d)? {
        IdOrName::Id(n) => n.to_string(),
        IdOrName::Name(s) => s,
    };
    if text.trim().is_empty() {
        return Ok(String::new());
    }
    validate::project_ref(&text).map_err(D::Error::custom)
}

fn priority_map<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, u32>, D::Error> {
    let raw = BTreeMap::<String, u32>::deserialize(d)?;
    raw.into_iter()
        .map(|(k, v)| {
            let p = Priority::parse(&k).ok_or_else(|| {
                D::Error::custom(format!(
                    "priority_map 的 `{k}` 不是等級；可用：{}",
                    Priority::allowed()
                ))
            })?;
            Ok((p.as_str().to_string(), v))
        })
        .collect()
}
