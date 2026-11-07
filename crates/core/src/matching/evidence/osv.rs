//! OSV 套件判定：OSV 以元件版本查詢，命中即受影響、未命中即不受影響。
//!
//! 元件版本變更（或恢復、資產重新啟用）後、下次 OSV 同步前，結果不是最新（[`osv_is_current`]），
//! 該套件不比對、命中維持原狀。

use chrono::{DateTime, FixedOffset};

use super::{Evidence, Source, Verdict};

/// `osv_hit` 的一列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsvHit {
    pub osv_id: String,
    pub cve_ids: Vec<String>,
    pub fixed_versions: Vec<String>,
    pub queried_at: String,
}

fn time(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(s).ok()
}

/// `a` 是否早於 `b`；任一無法解析時視為早於（結果當作過期，不判定為不受影響）。
fn before(a: &str, b: &str) -> bool {
    match (time(a), time(b)) {
        (Some(x), Some(y)) => x < y,
        _ => true,
    }
}

/// 元件版本的 OSV 結果是否為最新：最近一次成功的 OSV 同步晚於元件更新時間。
/// 不是最新時（版本變更或恢復後尚未重查）該套件不比對，命中維持原狀。
pub fn osv_is_current(component_updated_at: &str, last_osv_sync: Option<&str>) -> bool {
    last_osv_sync.is_some_and(|at| !before(at, component_updated_at))
}

/// 套件元件 × CVE：OSV 列出即受影響，未列出即不受影響。
/// 呼叫端須先以 [`osv_is_current`] 確認結果為最新（否則該套件不比對）。
pub fn evaluate(cve_id: &str, hits: &[OsvHit]) -> Evidence {
    match hits.iter().find(|h| h.cve_ids.iter().any(|c| c == cve_id)) {
        Some(h) => {
            let mut e = Evidence::new(
                Source::Osv,
                Verdict::Affected,
                format!("OSV {} 列出此套件版本", h.osv_id),
            );
            e.fixed_versions = h.fixed_versions.clone();
            e
        }
        None => Evidence::new(
            Source::Osv,
            Verdict::NotAffected,
            "OSV 查詢此套件版本已無此 CVE",
        ),
    }
}
