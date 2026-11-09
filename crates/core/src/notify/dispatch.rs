//! 通知流程：sync 後排入緊急通知、每日摘要、清送出佇列。
//!
//! - 每次 `sync` 與 `notify digest` 開始時先送出到期的佇列項目（[`Notifier::flush`]）
//! - sync 比對後：Email 合併當次所有緊急事件為一封；Redmine 依 CVE 開一張議題，
//!   已有議題（或建立中）的 CVE 改加註記（[`Notifier::queue_urgent`]）
//! - 每日摘要：新摘要取代尚未送出的舊摘要，範圍從上次成功寄出的摘要算起（[`Notifier::queue_digest`]）
//!
//! 事件在排入佇列的同一交易中寫入 `notification_log`，下次 sync 不會重複排入；送出成功才標為已送出。

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, TransactionBehavior};

use super::content::{self, Zone};
use super::digest::{self, DigestData};
use super::email::{Mail, MailTransport, SmtpMailer, SmtpSettings};
use super::outbox::{self, Channel, Deliver, Delivery, FlushStats, OutboxItem};
use super::redmine::{RedmineApi, RedmineClient, RedmineError, RedmineSettings};
use super::redmine_content::{self, IssueDraft, NoteDraft};
use super::settings::NotifyConfig;
use super::urgent::{self, UrgentEvent};
use crate::config::Config;
use crate::error::CoreError;
use crate::secrets::{SecretKey, SecretStore};

pub const KIND_URGENT: &str = "urgent";
pub const KIND_DIGEST: &str = "digest";
pub const KIND_CREATE_ISSUE: &str = "create_issue";
pub const KIND_ADD_NOTE: &str = "add_note";

/// 摘要佇列項目的內容。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DigestPayload {
    pub mail: Mail,
    /// 涵蓋的結束時間（寄出成功後成為下次摘要的起點）
    pub until: String,
}

/// 管道的送出端：已建立、停用，或設定有誤（送出時回報此錯誤並依退避重送）。
pub enum Endpoint<T: ?Sized> {
    Ready(Box<T>),
    Disabled,
    Broken(String),
}

impl<T: ?Sized> Endpoint<T> {
    pub fn is_enabled(&self) -> bool {
        !matches!(self, Endpoint::Disabled)
    }
}

/// 每日摘要的生命週期提醒需要的目錄與提醒天數（None 時摘要不含生命週期段落）。
#[derive(Debug, Clone)]
pub struct NotifyLifecycle {
    pub catalog: crate::catalog::Catalog,
    pub window: u32,
}

pub struct Notifier {
    pub config: NotifyConfig,
    pub stale_days: u32,
    /// 生命週期提醒（22 design D12）
    pub lifecycle: Option<NotifyLifecycle>,
    pub zone: Zone,
    pub mail: Endpoint<dyn MailTransport>,
    pub redmine: Endpoint<dyn RedmineApi>,
}

/// 排入緊急通知的結果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UrgentQueued {
    /// 排入的緊急信事件數（合併為一封）
    pub email_events: usize,
    pub issues: usize,
    pub notes: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DigestOutcome {
    /// Email 未啟用，不產生摘要
    EmailDisabled,
    /// 沒有新事項且 `send_empty = false`
    SkippedEmpty,
    /// 另一個程序正在寄出先前的摘要（不另產生重疊的摘要）
    InFlight,
    Queued {
        empty: bool,
        replaced: usize,
    },
}

impl Notifier {
    /// 加上每日摘要的生命週期提醒（CLI `notify digest` 由 runtime 傳入目錄）。
    pub fn with_lifecycle(self, catalog: crate::catalog::Catalog, window: u32) -> Self {
        Notifier {
            lifecycle: Some(NotifyLifecycle { catalog, window }),
            ..self
        }
    }

    /// 依設定建立：秘密取自憑證庫；設定不完整或違反傳輸規則的管道在送出時回報原因。
    pub fn from_config(config: &Config, store: &dyn SecretStore) -> Self {
        let n = &config.notify;
        let timeout = Duration::from_secs(config.http.timeout_secs);
        let mail = if n.email.enabled {
            mail_endpoint(secret(store, SecretKey::SmtpPassword).and_then(|password| {
                SmtpMailer::new(&SmtpSettings {
                    email: &n.email,
                    password,
                    ca_file: n.ca_file(),
                    allow_insecure: n.allow_insecure_transport,
                    timeout,
                })
            }))
        } else {
            Endpoint::Disabled
        };
        let redmine = if n.redmine.enabled {
            redmine_endpoint(secret(store, SecretKey::RedmineApiKey).and_then(|api_key| {
                RedmineClient::new(&RedmineSettings {
                    redmine: &n.redmine,
                    api_key,
                    ca_file: n.ca_file(),
                    allow_insecure: n.allow_insecure_transport,
                    http: &config.http,
                })
            }))
        } else {
            Endpoint::Disabled
        };
        if n.allow_insecure_transport {
            tracing::warn!("明文傳輸例外已啟用（allow_insecure_transport = true）");
        }
        Notifier {
            config: n.clone(),
            stale_days: config.pending.stale_days,
            lifecycle: None,
            zone: Zone::Local,
            mail,
            redmine,
        }
    }

    /// 送出所有到期的佇列項目。
    pub fn flush(&self, conn: &Connection, now: DateTime<Utc>) -> Result<FlushStats, CoreError> {
        outbox::flush(conn, self, now)
    }

    /// 偵測尚未通知的緊急事件並排入佇列（單一交易）。
    pub fn queue_urgent(
        &self,
        conn: &mut Connection,
        now: DateTime<Utc>,
    ) -> Result<UrgentQueued, CoreError> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let names = content::product_names(&tx)?;
        let mut queued = UrgentQueued::default();
        // 緊急命中只載入一次，兩個管道與議題內容共用
        let rows = if self.mail.is_enabled() || self.redmine.is_enabled() {
            urgent::urgent_findings(&tx, self.config.urgent_cvss)?
        } else {
            Vec::new()
        };
        if self.mail.is_enabled() {
            let events = urgent::detect_in(&tx, Channel::Email, &rows)?;
            if !events.is_empty() {
                let mail =
                    content::urgent_mail(&events, &names, self.config.urgent_cvss, now, self.zone);
                let id = outbox::enqueue(&tx, Channel::Email, KIND_URGENT, &to_json(&mail)?, now)?;
                urgent::record(&tx, Channel::Email, &events, Some(id), now)?;
                queued.email_events = events.len();
            }
        }
        if self.redmine.is_enabled() {
            let events = urgent::detect_in(&tx, Channel::Redmine, &rows)?;
            queue_redmine(&tx, &events, &rows, &names, now, &mut queued)?;
        }
        tx.commit()?;
        if queued != UrgentQueued::default() {
            tracing::info!(
                email_events = queued.email_events,
                issues = queued.issues,
                notes = queued.notes,
                "已排入緊急通知"
            );
        }
        Ok(queued)
    }

    /// 設定有誤而無法送出的管道與原因（`config check` 用）。
    pub fn problems(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Endpoint::Broken(e) = &self.mail {
            out.push(("Email", e.clone()));
        }
        if let Endpoint::Broken(e) = &self.redmine {
            out.push(("Redmine", e.clone()));
        }
        out
    }

    /// sync 比對後：排入緊急通知並立即送出。
    pub fn after_sync(
        &self,
        conn: &mut Connection,
        now: DateTime<Utc>,
    ) -> Result<(UrgentQueued, FlushStats), CoreError> {
        let queued = self.queue_urgent(conn, now)?;
        let stats = self.flush(conn, now)?;
        Ok((queued, stats))
    }

    /// 產生每日摘要並排入佇列（取代尚未送出的舊摘要）。
    pub fn queue_digest(
        &self,
        conn: &mut Connection,
        now: DateTime<Utc>,
    ) -> Result<DigestOutcome, CoreError> {
        if !self.mail.is_enabled() {
            return Ok(DigestOutcome::EmailDisabled);
        }
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if outbox::in_flight(&tx, Channel::Email, KIND_DIGEST, now)? {
            tx.commit()?;
            return Ok(DigestOutcome::InFlight);
        }
        let since = digest::period_start(&tx, now)?;
        let lifecycle = self.lifecycle.as_ref().map(|l| digest::LifecycleInput {
            catalog: &l.catalog,
            window: l.window,
            zone: self.zone,
        });
        let data = digest::collect(&tx, since, now, self.stale_days, lifecycle)?;
        let empty = data.is_empty();
        if empty && !self.config.email.send_empty {
            tx.commit()?;
            return Ok(DigestOutcome::SkippedEmpty);
        }
        let replaced = outbox::discard_pending(&tx, Channel::Email, KIND_DIGEST, now)?;
        let payload = DigestPayload {
            mail: self.digest_mail(&tx, &data)?,
            until: now.to_rfc3339(),
        };
        outbox::enqueue(&tx, Channel::Email, KIND_DIGEST, &to_json(&payload)?, now)?;
        tx.commit()?;
        Ok(DigestOutcome::Queued { empty, replaced })
    }

    pub fn digest_mail(&self, conn: &Connection, data: &DigestData) -> Result<Mail, CoreError> {
        let names = content::product_names(conn)?;
        Ok(content::digest_mail(data, &names, self.zone))
    }

    fn deliver_email(&self, conn: &Connection, item: &OutboxItem) -> Delivery {
        let mailer = match &self.mail {
            Endpoint::Ready(m) => m,
            Endpoint::Disabled => return Delivery::Skipped,
            Endpoint::Broken(e) => return Delivery::Blocked(e.clone()),
        };
        let (mail, until) = match item.kind.as_str() {
            KIND_DIGEST => match from_json::<DigestPayload>(&item.payload_json) {
                Ok(p) => (p.mail, Some(p.until)),
                Err(e) => return Delivery::Permanent(e),
            },
            _ => match from_json::<Mail>(&item.payload_json) {
                Ok(m) => (m, None),
                Err(e) => return Delivery::Permanent(e),
            },
        };
        if let Err(e) = mailer.send(&mail) {
            return Delivery::Retry(e);
        }
        if let Some(until) = until {
            if let Err(e) = digest::mark_sent(conn, &until) {
                tracing::error!(error = %e, "摘要已寄出，但無法記錄寄出時間");
            }
        }
        Delivery::Sent
    }

    fn deliver_redmine(
        &self,
        conn: &Connection,
        item: &OutboxItem,
        now: DateTime<Utc>,
    ) -> Delivery {
        let api = match &self.redmine {
            Endpoint::Ready(a) => a,
            Endpoint::Disabled => return Delivery::Skipped,
            Endpoint::Broken(e) => return Delivery::Blocked(e.clone()),
        };
        let result = match item.kind.as_str() {
            KIND_CREATE_ISSUE => match from_json::<IssueDraft>(&item.payload_json) {
                // 先前失敗，或先前的程序在送出途中中止（認領過期）：Redmine 可能已建立
                Ok(d) => create_issue(
                    conn,
                    api.as_ref(),
                    &d,
                    item.attempts > 0 || item.claimed_until.is_some(),
                    now,
                ),
                Err(e) => return Delivery::Permanent(e),
            },
            KIND_ADD_NOTE => match from_json::<NoteDraft>(&item.payload_json) {
                Ok(n) => add_note(conn, api.as_ref(), &n),
                Err(e) => return Delivery::Permanent(e),
            },
            other => return Delivery::Permanent(format!("未知的 Redmine 項目種類 `{other}`")),
        };
        match result {
            Ok(d) => d,
            Err(e) => Delivery::Retry(e.to_string()),
        }
    }
}

impl Deliver for Notifier {
    fn deliver(&self, conn: &Connection, item: &OutboxItem, now: DateTime<Utc>) -> Delivery {
        match item.channel {
            Channel::Email => self.deliver_email(conn, item),
            Channel::Redmine => self.deliver_redmine(conn, item, now),
        }
    }
}

/// 記錄議題編號；Redmine 端已建立時即使無法記錄也視為已送出，避免重送 POST 建立第二張。
fn remember_issue(conn: &Connection, cve_id: &str, issue_id: u64, now: DateTime<Utc>) -> Delivery {
    match redmine_content::record_issue(conn, cve_id, issue_id, &now.to_rfc3339()) {
        Ok(()) => tracing::info!(cve = %cve_id, issue_id, "已記錄 Redmine 議題"),
        Err(e) => tracing::error!(cve = %cve_id, issue_id, error = %e,
            "Redmine 議題已建立，但無法記錄議題編號；之後的註記可能另開新議題，請手動確認"),
    }
    Delivery::Sent
}

/// `retrying`：先前的嘗試失敗（可能是 Redmine 已建立、但回應沒有收到），重送前先查既有議題。
fn create_issue(
    conn: &Connection,
    api: &dyn RedmineApi,
    draft: &IssueDraft,
    retrying: bool,
    now: DateTime<Utc>,
) -> Result<Delivery, CoreError> {
    // 同一 CVE 已由其他請求建立議題：改為註記，不建立第二張；議題已被刪除則清除對應後重新建立
    if let Some(id) = redmine_content::issue_id(conn, &draft.cve_id)? {
        match api.add_note(id, &draft.description) {
            Err(RedmineError::NotFound(_)) => {
                redmine_content::forget_issue(conn, &draft.cve_id)?;
                tracing::warn!(cve = %draft.cve_id, issue_id = id, "Redmine 議題已不存在，改為重新建立");
            }
            other => return Ok(map_redmine(other.map(|()| None)).into_delivery()),
        }
    }
    if retrying {
        match api.find_issue(&draft.cve_id) {
            Ok(Some(id)) => return Ok(remember_issue(conn, &draft.cve_id, id, now)),
            Ok(None) => {}
            Err(e) => return Ok(map_redmine(Err(e)).into_delivery()),
        }
    }
    Ok(match map_redmine(api.create_issue(draft).map(Some)) {
        Delivered::Done(Some(issue_id)) => remember_issue(conn, &draft.cve_id, issue_id, now),
        other => other.into_delivery(),
    })
}

fn add_note(
    conn: &Connection,
    api: &dyn RedmineApi,
    note: &NoteDraft,
) -> Result<Delivery, CoreError> {
    match redmine_content::issue_id(conn, &note.cve_id)? {
        Some(id) => match api.add_note(id, &note.notes) {
            Err(RedmineError::NotFound(e)) => {
                // 議題已在 Redmine 刪除：清除對應，之後的緊急事件重新開案
                redmine_content::forget_issue(conn, &note.cve_id)?;
                tracing::warn!(cve = %note.cve_id, issue_id = id, "Redmine 議題已不存在，已清除對應");
                Ok(Delivery::Permanent(format!("議題 #{id} 已不存在：{e}")))
            }
            other => Ok(map_redmine(other.map(|()| None)).into_delivery()),
        },
        None if redmine_content::pending_create(conn, &note.cve_id)? => Ok(Delivery::Defer(
            format!("{} 的議題仍在佇列中等待建立", note.cve_id),
        )),
        None => Ok(Delivery::Permanent(format!(
            "{} 沒有 Redmine 議題（建立議題的請求已無法送出）",
            note.cve_id
        ))),
    }
}

enum Delivered {
    Done(Option<u64>),
    Failed(Delivery),
}

impl Delivered {
    fn into_delivery(self) -> Delivery {
        match self {
            Delivered::Done(_) => Delivery::Sent,
            Delivered::Failed(d) => d,
        }
    }
}

fn map_redmine(result: Result<Option<u64>, RedmineError>) -> Delivered {
    match result {
        Ok(id) => Delivered::Done(id),
        Err(RedmineError::Transient(e)) => Delivered::Failed(Delivery::Retry(e)),
        Err(RedmineError::Permanent(e) | RedmineError::NotFound(e)) => {
            Delivered::Failed(Delivery::Permanent(e))
        }
    }
}

fn queue_redmine(
    conn: &Connection,
    events: &[UrgentEvent],
    urgent_rows: &[crate::matching::query::FindingRow],
    names: &HashMap<String, String>,
    now: DateTime<Utc>,
    queued: &mut UrgentQueued,
) -> Result<(), CoreError> {
    let known = redmine_content::known_issue_cves(conn)?;
    for (cve_id, group) in redmine_content::by_cve(events) {
        let owned: Vec<UrgentEvent> = group.iter().map(|e| (*e).clone()).collect();
        let id = if known.contains(&cve_id) {
            let note = redmine_content::event_note(&cve_id, &group, names);
            queued.notes += 1;
            outbox::enqueue(conn, Channel::Redmine, KIND_ADD_NOTE, &to_json(&note)?, now)?
        } else {
            let affected = urgent_rows
                .iter()
                .filter(|r| r.cve_id == cve_id)
                .cloned()
                .collect();
            let draft = redmine_content::issue_draft(conn, &cve_id, &group, affected, names)?;
            queued.issues += 1;
            outbox::enqueue(
                conn,
                Channel::Redmine,
                KIND_CREATE_ISSUE,
                &to_json(&draft)?,
                now,
            )?
        };
        urgent::record(conn, Channel::Redmine, &owned, Some(id), now)?;
    }
    Ok(())
}

fn secret(
    store: &dyn SecretStore,
    key: SecretKey,
) -> Result<Option<zeroize::Zeroizing<String>>, String> {
    store.get(key).map_err(|e| e.to_string())
}

fn mail_endpoint(r: Result<SmtpMailer, String>) -> Endpoint<dyn MailTransport> {
    match r {
        Ok(m) => Endpoint::Ready(Box::new(m)),
        Err(e) => Endpoint::Broken(e),
    }
}

fn redmine_endpoint(r: Result<RedmineClient, String>) -> Endpoint<dyn RedmineApi> {
    match r {
        Ok(c) => Endpoint::Ready(Box::new(c)),
        Err(e) => Endpoint::Broken(e),
    }
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<String, CoreError> {
    serde_json::to_string(v).map_err(|e| CoreError::Invalid(format!("無法序列化通知內容：{e}")))
}

fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, String> {
    serde_json::from_str(s).map_err(|e| format!("佇列內容無法解析：{e}"))
}
