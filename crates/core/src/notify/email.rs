//! SMTP 寄信（lettre + rustls）。
//!
//! - `starttls`：`Tls::Required`，伺服器不支援 STARTTLS 就失敗，不降級為明文
//! - `tls`：SMTPS（隱含 TLS，通常為 465）
//! - `none`：只有 `allow_insecure_transport` 時可用，且拒絕經明文送出密碼
//!
//! 伺服器憑證一律驗證（系統根憑證 + `ca_file`）。錯誤訊息只含伺服器回應，不含密碼。

use std::path::Path;
use std::time::Duration;

use lettre::message::header::ContentType;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Certificate, Tls, TlsParameters};
use lettre::{Message, SmtpTransport, Transport};
use zeroize::Zeroizing;

use super::settings::{EmailConfig, SmtpSecurity};
use super::tls::{load_ca_file, PLAINTEXT_SECRET_REFUSED_SMTP};
use super::validate;
use crate::process::error_chain;

/// 要寄出的信件內容（收件者取自寄送當下的設定）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Mail {
    pub subject: String,
    pub body: String,
}

/// 寄信介面；正式為 [`SmtpMailer`]，測試可替換。
pub trait MailTransport {
    fn send(&self, mail: &Mail) -> Result<(), String>;
}

/// 建立連線需要的設定（可使用尚未儲存的表單值）。
pub struct SmtpSettings<'a> {
    pub email: &'a EmailConfig,
    /// Credential Manager 中的 smtp_password
    pub password: Option<Zeroizing<String>>,
    pub ca_file: Option<&'a Path>,
    pub allow_insecure: bool,
    pub timeout: Duration,
}

pub struct SmtpMailer {
    transport: SmtpTransport,
    from: Mailbox,
    to: Vec<Mailbox>,
}

impl SmtpMailer {
    /// 驗證設定並建立連線設定（尚未連線）。設定不完整、違反傳輸規則時回傳說明。
    pub fn new(s: &SmtpSettings<'_>) -> Result<Self, String> {
        let email = s.email;
        email.ready()?;
        let from = validate::mailbox(&email.from)?;
        let to = email
            .to
            .iter()
            .map(|t| validate::mailbox(t))
            .collect::<Result<Vec<_>, _>>()?;
        let host = email.smtp_host.trim();
        let tls = tls_mode(s, host)?;
        let mut builder = SmtpTransport::builder_dangerous(host)
            .port(email.smtp_port)
            .tls(tls)
            .timeout(Some(s.timeout));
        if let Some(creds) = credentials(s)? {
            builder = builder.credentials(creds);
        }
        Ok(SmtpMailer {
            transport: builder.build(),
            from,
            to,
        })
    }
}

fn tls_mode(s: &SmtpSettings<'_>, host: &str) -> Result<Tls, String> {
    let security = s.email.security;
    if security == SmtpSecurity::None {
        if !s.allow_insecure {
            return Err("無加密 SMTP 需在設定檔手動設定 allow_insecure_transport = true".into());
        }
        if s.email.authenticates() || s.password.is_some() {
            return Err(PLAINTEXT_SECRET_REFUSED_SMTP.into());
        }
        return Ok(Tls::None);
    }
    let mut params = TlsParameters::builder(host.to_string());
    if let Some(path) = s.ca_file {
        for pem in load_ca_file(path)? {
            let cert = Certificate::from_pem(pem.as_bytes())
                .map_err(|e| format!("CA 檔 {} 的憑證無法讀取：{e}", path.display()))?;
            params = params.add_root_certificate(cert);
        }
    }
    let params = params
        .build_rustls()
        .map_err(|e| format!("無法建立 TLS 設定：{e}"))?;
    Ok(match security {
        SmtpSecurity::Tls => Tls::Wrapper(params),
        _ => Tls::Required(params),
    })
}

fn credentials(s: &SmtpSettings<'_>) -> Result<Option<Credentials>, String> {
    if !s.email.authenticates() {
        return Ok(None);
    }
    let password = s.password.as_ref().ok_or_else(|| {
        "已設定 SMTP 帳號，但 Credential Manager 中沒有 smtp_password；請在設定畫面輸入密碼，\
         或執行 `cvewatch config set-secret smtp_password`"
            .to_string()
    })?;
    Ok(Some(Credentials::new(
        s.email.username.trim().to_string(),
        password.to_string(),
    )))
}

impl MailTransport for SmtpMailer {
    fn send(&self, mail: &Mail) -> Result<(), String> {
        let mut builder = Message::builder()
            .from(self.from.clone())
            .subject(mail.subject.clone())
            .header(ContentType::TEXT_PLAIN);
        for to in &self.to {
            builder = builder.to(to.clone());
        }
        let message = builder
            .body(mail.body.clone())
            .map_err(|e| format!("無法組成信件：{e}"))?;
        self.transport
            .send(&message)
            .map(|_| ())
            .map_err(|e| describe(&e))
    }
}

/// 中文說明 + lettre 的錯誤（含伺服器回應碼與訊息）。
fn describe(err: &lettre::transport::smtp::Error) -> String {
    let detail = error_chain(err);
    if detail.contains("STARTTLS is not supported") {
        return "伺服器不支援 STARTTLS，未寄出（不會降級為明文）".into();
    }
    if err
        .status()
        .is_some_and(|c| c.to_string().starts_with("535"))
        || detail.contains("authentication")
    {
        return format!("SMTP 驗證失敗：{detail}");
    }
    if err.is_tls() || detail.contains("certificate") {
        return format!("TLS 連線失敗（憑證無法驗證時請確認 ca_file）：{detail}");
    }
    format!("SMTP 寄送失敗：{detail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn email(security: SmtpSecurity, username: &str) -> EmailConfig {
        EmailConfig {
            enabled: true,
            smtp_host: "mail.example.edu.tw".into(),
            smtp_port: 25,
            security,
            username: username.into(),
            from: "cvewatch <a@example.edu.tw>".into(),
            to: vec!["b@example.edu.tw".into()],
            send_empty: true,
        }
    }

    fn settings<'a>(
        email: &'a EmailConfig,
        password: Option<&str>,
        insecure: bool,
    ) -> SmtpSettings<'a> {
        SmtpSettings {
            email,
            password: password.map(|p| Zeroizing::new(p.to_string())),
            ca_file: None,
            allow_insecure: insecure,
            timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn plaintext_with_password_is_refused() {
        let e = email(SmtpSecurity::None, "");
        let err = SmtpMailer::new(&settings(&e, Some("pw"), true))
            .err()
            .unwrap();
        assert!(err.contains("不得傳送 SMTP 密碼"), "{err}");
        let e = email(SmtpSecurity::None, "rz");
        assert!(SmtpMailer::new(&settings(&e, None, true)).is_err());
        let anon = email(SmtpSecurity::None, "");
        assert!(SmtpMailer::new(&settings(&anon, None, true)).is_ok());
        assert!(SmtpMailer::new(&settings(&anon, None, false)).is_err());
    }

    #[test]
    fn username_without_password_is_explained() {
        let e = email(SmtpSecurity::Starttls, "rz");
        let err = SmtpMailer::new(&settings(&e, None, false)).err().unwrap();
        assert!(err.contains("smtp_password"), "{err}");
        assert!(SmtpMailer::new(&settings(&e, Some("pw"), false)).is_ok());
    }

    #[test]
    fn incomplete_settings_are_explained() {
        let mut e = email(SmtpSecurity::Starttls, "");
        e.to.clear();
        let err = SmtpMailer::new(&settings(&e, None, false)).err().unwrap();
        assert!(err.contains("收件者"), "{err}");
    }
}
