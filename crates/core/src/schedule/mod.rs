//! Windows 工作排程：`[schedule]` 設定與 Task Scheduler XML 產生（呼叫 schtasks 在執行檔）。

pub mod xml;

use serde::Deserialize;

use crate::notify::validate;

pub const DEFAULT_SYNC_EVERY_HOURS: u32 = 4;
pub const DEFAULT_DIGEST_AT: &str = "08:30";

/// `[schedule]`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawSchedule")]
pub struct ScheduleConfig {
    pub sync_every_hours: u32,
    /// 每日摘要時間 `HH:MM`（已驗證）
    pub digest_at: String,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        ScheduleConfig {
            sync_every_hours: DEFAULT_SYNC_EVERY_HOURS,
            digest_at: DEFAULT_DIGEST_AT.to_string(),
        }
    }
}

impl ScheduleConfig {
    /// 摘要時間 (時, 分)。載入時已驗證，無法解析時退回預設值。
    pub fn digest_time(&self) -> (u32, u32) {
        validate::hhmm(&self.digest_at).unwrap_or((8, 30))
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct RawSchedule {
    sync_every_hours: i64,
    digest_at: String,
}

impl Default for RawSchedule {
    fn default() -> Self {
        let d = ScheduleConfig::default();
        RawSchedule {
            sync_every_hours: i64::from(d.sync_every_hours),
            digest_at: d.digest_at,
        }
    }
}

impl TryFrom<RawSchedule> for ScheduleConfig {
    type Error = String;

    fn try_from(raw: RawSchedule) -> Result<Self, Self::Error> {
        let sync_every_hours = validate::sync_hours(raw.sync_every_hours)?;
        validate::hhmm(&raw.digest_at)?;
        Ok(ScheduleConfig {
            sync_every_hours,
            digest_at: raw.digest_at.trim().to_string(),
        })
    }
}
