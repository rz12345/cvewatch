//! 通知：緊急事件判定、Email（緊急信與每日摘要）、Redmine 開案與送出佇列。

pub mod check;
pub mod content;
pub mod digest;
pub mod dispatch;
pub mod email;
pub mod outbox;
pub mod redmine;
pub mod redmine_content;
pub mod settings;
pub mod tls;
pub mod triage_notes;
pub mod urgent;
pub mod validate;
