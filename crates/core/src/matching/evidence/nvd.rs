//! NVD configurations 判定：只看 `vulnerable = true` 且 vendor:product 屬於產品的 CPE 條件。
//!
//! - 有版本區間（start/end incl/excl）時以產品 scheme 判斷
//! - `criteria` 帶固定版本（非 `*`）時做等值比較；CPE 的 update 欄位依 scheme 併入版本
//!   （PAN-OS `11.1.4:h4` → `11.1.4-h4`、ESXi `7.0:update_3c` → `7.0 update 3c`）
//! - AND 設定中非 vulnerable（或 negate）的平台條件忽略，並在證據註記

use std::collections::BTreeSet;

use super::range::{lower_bound, upper_bound, within, Bound};
use super::{ComponentVersion, Evidence, ProductIds, Source, Verdict};
use crate::matching::version::{Version, VersionScheme};

pub const PLATFORM_IGNORED: &str = "NVD 設定含平台條件，已忽略";

/// `cve_nvd_match` 的一列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvdMatch {
    pub config_group: i64,
    pub config_operator: Option<String>,
    pub node_negate: bool,
    pub criteria: String,
    /// `criteria` 的 (vendor, product)，載入時解析一次
    pub vendor_product: Option<(String, String)>,
    /// `criteria` 的 version 與 update 欄位（已移除跳脫字元；缺欄位時為 `*`），載入時解析一次
    pub cpe_version: String,
    pub cpe_update: String,
    pub vulnerable: bool,
    pub start_incl: Option<String>,
    pub start_excl: Option<String>,
    pub end_incl: Option<String>,
    pub end_excl: Option<String>,
}

impl NvdMatch {
    /// 屬於此產品的 vulnerable 條件（產品是否被提到、NVD 證據是否成立共用此判斷）。
    pub fn is_vulnerable_for(&self, ids: &ProductIds) -> bool {
        self.vulnerable
            && !self.node_negate
            && self
                .vendor_product
                .as_ref()
                .is_some_and(|vp| ids.has_cpe(vp))
    }

    fn has_range(&self) -> bool {
        self.start_incl.is_some()
            || self.start_excl.is_some()
            || self.end_incl.is_some()
            || self.end_excl.is_some()
    }

    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(s) = &self.start_incl {
            parts.push(format!("≥ {s}"));
        }
        if let Some(s) = &self.start_excl {
            parts.push(format!("> {s}"));
        }
        if let Some(s) = &self.end_incl {
            parts.push(format!("≤ {s}"));
        }
        if let Some(s) = &self.end_excl {
            parts.push(format!("< {s}"));
        }
        if parts.is_empty() {
            self.criteria.clone()
        } else {
            format!("{}（{}）", self.criteria, parts.join("、"))
        }
    }
}

fn is_any(field: &str) -> bool {
    field.is_empty() || field == "*" || field == "-"
}

/// CPE 固定版本：合成後的版本字串，以及 update 欄位是否為 `*`（任何 update）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpeVersion {
    pub text: String,
    pub any_update: bool,
}

/// CPE 的 version 與 update 欄位依 scheme 合成產品版本字串；version 為 `*` / `-` 時為 None。
///
/// update 為 `*`（ANY）時涵蓋該版本線的任何 update／hotfix；為 `-`（NA）或空白時只有該版本本身。
/// DSM 以數字開頭的 update 是 build（`7.2.1:69057` → `7.2.1-69057`、`7.1.1:42962-1` → `7.1.1-42962-1`），
/// `update_3` 這類才以空白相接。
pub fn cpe_version(scheme: &str, version: &str, update: &str) -> Option<CpeVersion> {
    if is_any(version) {
        return None;
    }
    let plain = |any_update| {
        Some(CpeVersion {
            text: version.to_string(),
            any_update,
        })
    };
    match update {
        "*" => return plain(true),
        "" | "-" => return plain(false),
        _ => {}
    }
    let numeric = update.starts_with(|c: char| c.is_ascii_digit());
    let text = match scheme {
        "esxi" => format!("{version} {update}"),
        "dsm" if !numeric => format!("{version} {update}"),
        _ => format!("{version}-{update}"),
    };
    Some(CpeVersion {
        text,
        any_update: false,
    })
}

enum RowResult {
    In(String),
    Out,
    Unparseable,
}

fn evaluate_row(scheme: &dyn VersionScheme, v: &Version, row: &NvdMatch) -> RowResult {
    if let Some(fixed) = cpe_version(scheme.name(), &row.cpe_version, &row.cpe_update) {
        let Some(target) = scheme.parse(&fixed.text) else {
            return RowResult::Unparseable;
        };
        let (hit, label) = if fixed.any_update {
            (scheme.same_line(&target, v), "版本線")
        } else {
            (scheme.covers(&target, v), "版本 =")
        };
        return match hit {
            None => RowResult::Unparseable,
            Some(true) => RowResult::In(format!("{}（{label} {}）", row.criteria, fixed.text)),
            Some(false) => RowResult::Out,
        };
    }
    if row.cpe_version == "-" && !row.has_range() {
        return RowResult::Unparseable;
    }
    let lower = match (&row.start_incl, &row.start_excl) {
        (Some(s), _) => lower_bound(scheme, Some(s), true),
        (None, Some(s)) => lower_bound(scheme, Some(s), false),
        (None, None) => Some(Bound::Unbounded),
    };
    let upper = match (&row.end_incl, &row.end_excl) {
        (Some(s), _) => upper_bound(scheme, Some(s), true),
        (None, Some(s)) => upper_bound(scheme, Some(s), false),
        (None, None) => Some(Bound::Unbounded),
    };
    let (Some(lower), Some(upper)) = (lower, upper) else {
        return RowResult::Unparseable;
    };
    match within(scheme, v, &lower, &upper) {
        None => RowResult::Unparseable,
        Some(true) => RowResult::In(row.describe()),
        Some(false) => RowResult::Out,
    }
}

/// 只在 NVD 有屬於此產品的 vulnerable 條件時產生證據。
pub fn evaluate(
    rows: &[NvdMatch],
    ids: &ProductIds,
    version: &ComponentVersion<'_>,
) -> Option<Evidence> {
    let relevant: Vec<&NvdMatch> = rows.iter().filter(|r| r.is_vulnerable_for(ids)).collect();
    if relevant.is_empty() {
        return None;
    }
    let groups: BTreeSet<i64> = relevant.iter().map(|r| r.config_group).collect();
    let platform = rows.iter().any(|r| {
        groups.contains(&r.config_group)
            && r.config_operator
                .as_deref()
                .is_some_and(|o| o.eq_ignore_ascii_case("AND"))
            && (!r.vulnerable || r.node_negate)
    });
    let mut evidence = match &version.parsed {
        None => version.unparseable(Source::Nvd),
        Some(v) => verdict_for(version.scheme, v, &relevant),
    };
    // 版本為 `-`（沒有版本）且沒有區間的條件不提供版本資料
    evidence.has_version_data = relevant
        .iter()
        .any(|r| r.has_range() || r.cpe_version != "-");
    if platform {
        evidence.notes.push(PLATFORM_IGNORED.to_string());
    }
    Some(evidence)
}

fn verdict_for(scheme: &dyn VersionScheme, v: &Version, rows: &[&NvdMatch]) -> Evidence {
    let mut unparseable = 0;
    for row in rows {
        match evaluate_row(scheme, v, row) {
            RowResult::In(desc) => return Evidence::new(Source::Nvd, Verdict::Affected, desc),
            RowResult::Unparseable => unparseable += 1,
            RowResult::Out => {}
        }
    }
    if unparseable > 0 {
        return Evidence::new(
            Source::Nvd,
            Verdict::Unknown,
            format!("{unparseable} 個 NVD 條件的版本無法以產品格式比較"),
        );
    }
    Evidence::new(
        Source::Nvd,
        Verdict::NotAffected,
        format!("不在 NVD 的 {} 個受影響條件內", rows.len()),
    )
}
