//! 連線測試：`cvewatch notify test-email` / `test-redmine` 與 TUI 設定畫面共用。
//!
//! 使用呼叫端提供的設定（TUI 可傳入尚未儲存的表單值），不看 `enabled` 開關；
//! 測試 Redmine 不建立議題。

use std::time::Duration;

use zeroize::Zeroizing;

use super::email::{Mail, MailTransport, SmtpMailer, SmtpSettings};
use super::redmine::{RedmineClient, RedmineSettings, TestReport};
use super::settings::NotifyConfig;
use crate::config::HttpConfig;

pub const TEST_SUBJECT: &str = "[cvewatch] 測試信";

/// 寄出測試信，成功時回傳說明（收件者與連線方式）。
pub fn send_test_email(
    notify: &NotifyConfig,
    password: Option<Zeroizing<String>>,
    http: &HttpConfig,
) -> Result<String, String> {
    let email = &notify.email;
    let mailer = SmtpMailer::new(&SmtpSettings {
        email,
        password,
        ca_file: notify.ca_file(),
        allow_insecure: notify.allow_insecure_transport,
        timeout: Duration::from_secs(http.timeout_secs),
    })?;
    let mail = Mail {
        subject: TEST_SUBJECT.into(),
        body: format!(
            "這是 cvewatch 的測試信。\n\nSMTP：{}:{}（{}）\n收到此信表示通知設定正確。\n",
            email.smtp_host.trim(),
            email.smtp_port,
            email.security.label()
        ),
    };
    mailer.send(&mail)?;
    Ok(format!(
        "已寄出測試信至 {}（{}:{}，{}）",
        email.to.join("、"),
        email.smtp_host.trim(),
        email.smtp_port,
        email.security.label()
    ))
}

/// 驗證 API key、專案與 tracker，回傳可存取的專案與 tracker。
pub fn test_redmine(
    notify: &NotifyConfig,
    api_key: Option<Zeroizing<String>>,
    http: &HttpConfig,
) -> Result<TestReport, String> {
    let client = RedmineClient::new(&RedmineSettings {
        redmine: &notify.redmine,
        api_key,
        ca_file: notify.ca_file(),
        allow_insecure: notify.allow_insecure_transport,
        http,
    })?;
    client.test().map_err(|e| e.to_string())
}

/// 測試結果的說明文字。
pub fn describe_report(r: &TestReport) -> String {
    format!(
        "Redmine 連線正常：專案「{}」、tracker「{}」",
        r.project.label(),
        r.tracker.label()
    )
}
