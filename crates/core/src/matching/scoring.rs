//! 風險等級：KEV > EPSS > CVSS，對外曝露資產上調一級。
//!
//! ```text
//! P1：KEV
//! P2：EPSS ≥ epss_high，或 CVSS ≥ cvss_high
//! P3：EPSS ≥ epss_medium，或 CVSS ≥ cvss_medium
//! P4：其他（含未評分）
//! 曝露度在 exposure_bump 內 → 上調一級（P1 不變）
//! ```
//!
//! 同一級內：KEV 加入日期新 → EPSS 高 → CVSS 高 → 資產重要性高。

use std::cmp::Ordering;

use serde::Deserialize;

use crate::assets::model::Exposure;

pub const DEFAULT_EPSS_HIGH: f64 = 0.10;
pub const DEFAULT_EPSS_MEDIUM: f64 = 0.01;
pub const DEFAULT_CVSS_HIGH: f64 = 9.0;
pub const DEFAULT_CVSS_MEDIUM: f64 = 7.0;
pub const DEFAULT_EXPOSURE_BUMP: [&str; 2] = ["internet", "dmz"];
const CVSS_MAX: f64 = 10.0;

/// `[scoring]` 設定。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "RawScoring")]
pub struct ScoringConfig {
    pub epss_high: f64,
    pub epss_medium: f64,
    pub cvss_high: f64,
    pub cvss_medium: f64,
    pub exposure_bump: Vec<Exposure>,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        ScoringConfig {
            epss_high: DEFAULT_EPSS_HIGH,
            epss_medium: DEFAULT_EPSS_MEDIUM,
            cvss_high: DEFAULT_CVSS_HIGH,
            cvss_medium: DEFAULT_CVSS_MEDIUM,
            exposure_bump: DEFAULT_EXPOSURE_BUMP
                .iter()
                .filter_map(|e| Exposure::parse(e))
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct RawScoring {
    epss_high: f64,
    epss_medium: f64,
    cvss_high: f64,
    cvss_medium: f64,
    exposure_bump: Vec<String>,
}

impl Default for RawScoring {
    /// 由 [`ScoringConfig::default`] 推導，預設值只定義一次。
    fn default() -> Self {
        let d = ScoringConfig::default();
        RawScoring {
            epss_high: d.epss_high,
            epss_medium: d.epss_medium,
            cvss_high: d.cvss_high,
            cvss_medium: d.cvss_medium,
            exposure_bump: d
                .exposure_bump
                .iter()
                .map(|e| e.as_str().to_string())
                .collect(),
        }
    }
}

fn in_range(name: &str, value: f64, max: f64) -> Result<(), String> {
    if (0.0..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} = {value} 超出允許範圍 0–{max}"))
    }
}

impl TryFrom<RawScoring> for ScoringConfig {
    type Error = String;

    fn try_from(raw: RawScoring) -> Result<Self, Self::Error> {
        in_range("epss_high", raw.epss_high, 1.0)?;
        in_range("epss_medium", raw.epss_medium, 1.0)?;
        in_range("cvss_high", raw.cvss_high, CVSS_MAX)?;
        in_range("cvss_medium", raw.cvss_medium, CVSS_MAX)?;
        if raw.epss_medium > raw.epss_high {
            return Err("epss_medium 不可大於 epss_high".into());
        }
        if raw.cvss_medium > raw.cvss_high {
            return Err("cvss_medium 不可大於 cvss_high".into());
        }
        let exposure_bump = raw
            .exposure_bump
            .iter()
            .map(|e| {
                Exposure::parse(e).ok_or_else(|| {
                    format!(
                        "exposure_bump 的 `{e}` 不是曝露度；可用：{}",
                        Exposure::allowed()
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ScoringConfig {
            epss_high: raw.epss_high,
            epss_medium: raw.epss_medium,
            cvss_high: raw.cvss_high,
            cvss_medium: raw.cvss_medium,
            exposure_bump,
        })
    }
}

string_enum!(
    #[derive(PartialOrd, Ord)]
    Priority {
        P1 => "P1",
        P2 => "P2",
        P3 => "P3",
        P4 => "P4",
    }
);

impl Priority {
    /// 上調一級（P1 不變）。
    pub fn bumped(self) -> Self {
        match self {
            Priority::P1 | Priority::P2 => Priority::P1,
            Priority::P3 => Priority::P2,
            Priority::P4 => Priority::P3,
        }
    }
}

/// CVE 層級的評分資料。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScoreInputs {
    /// KEV 加入日期（YYYY-MM-DD）；不在 KEV 為 None
    pub kev_date: Option<String>,
    pub epss: Option<f64>,
    /// 顯示用 CVSS（見 `ingest::cvss::preferred`）
    pub cvss: Option<f64>,
}

impl ScoreInputs {
    pub fn in_kev(&self) -> bool {
        self.kev_date.is_some()
    }
}

fn at_least(value: Option<f64>, threshold: f64) -> bool {
    value.is_some_and(|v| v >= threshold)
}

pub fn base_priority(inputs: &ScoreInputs, cfg: &ScoringConfig) -> Priority {
    if inputs.in_kev() {
        Priority::P1
    } else if at_least(inputs.epss, cfg.epss_high) || at_least(inputs.cvss, cfg.cvss_high) {
        Priority::P2
    } else if at_least(inputs.epss, cfg.epss_medium) || at_least(inputs.cvss, cfg.cvss_medium) {
        Priority::P3
    } else {
        Priority::P4
    }
}

pub fn priority(inputs: &ScoreInputs, exposure: Exposure, cfg: &ScoringConfig) -> Priority {
    let base = base_priority(inputs, cfg);
    if cfg.exposure_bump.contains(&exposure) {
        base.bumped()
    } else {
        base
    }
}

/// 排序用的欄位。
#[derive(Debug, Clone, PartialEq)]
pub struct RankKey {
    pub priority: Priority,
    pub inputs: ScoreInputs,
    /// 資產價值 MAX(C, I, A) 1–4，數字越大越重要
    pub asset_value: u8,
}

fn desc_f64(a: Option<f64>, b: Option<f64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => y.total_cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// 等級 → KEV 加入日期新到舊（不在 KEV 最後）→ EPSS → CVSS → 資產價值。
pub fn rank_cmp(a: &RankKey, b: &RankKey) -> Ordering {
    let kev = match (&a.inputs.kev_date, &b.inputs.kev_date) {
        (Some(x), Some(y)) => y.cmp(x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    };
    a.priority
        .cmp(&b.priority)
        .then(kev)
        .then_with(|| desc_f64(a.inputs.epss, b.inputs.epss))
        .then_with(|| desc_f64(a.inputs.cvss, b.inputs.cvss))
        .then_with(|| b.asset_value.cmp(&a.asset_value))
}
