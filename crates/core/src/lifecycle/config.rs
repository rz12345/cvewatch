//! `[lifecycle]` 設定（22 design D10）。

use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

/// 提醒天數的預設值。
pub const DEFAULT_REMINDER_DAYS: u32 = 90;
/// 提醒天數的允許範圍。
pub const MIN_REMINDER_DAYS: u32 = 1;
pub const MAX_REMINDER_DAYS: u32 = 3650;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LifecycleConfig {
    /// EoL 與保固／維護合約／軟體授權到期的提前提醒天數（同一個值）
    #[serde(deserialize_with = "reminder_days")]
    pub reminder_days: u32,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        LifecycleConfig {
            reminder_days: DEFAULT_REMINDER_DAYS,
        }
    }
}

fn reminder_days<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let n = u32::deserialize(d)?;
    if !(MIN_REMINDER_DAYS..=MAX_REMINDER_DAYS).contains(&n) {
        return Err(D::Error::custom(format!(
            "reminder_days {n} 超出允許範圍 {MIN_REMINDER_DAYS}–{MAX_REMINDER_DAYS}"
        )));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Wrap {
        lifecycle: LifecycleConfig,
    }

    fn parse(text: &str) -> Result<LifecycleConfig, String> {
        toml::from_str::<Wrap>(text)
            .map(|w| w.lifecycle)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn reminder_days_out_of_range() {
        let err = parse("[lifecycle]\nreminder_days = 0\n").unwrap_err();
        assert!(err.contains("1–3650"), "{err}");
        assert!(parse("[lifecycle]\nreminder_days = 3651\n").is_err());
        assert_eq!(
            parse("[lifecycle]\nreminder_days = 30\n")
                .unwrap()
                .reminder_days,
            30
        );
        assert_eq!(parse("[lifecycle]\n").unwrap(), LifecycleConfig::default());
    }
}
