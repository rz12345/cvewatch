//! CNA / ADP `affected[]` 判定（CVE Record Format 5.x 語意）。
//!
//! 1. 依序檢查 `versions[]`：有 `lessThan` / `lessThanOrEqual` 時判斷 `version ≤ v < lessThan`，
//!    命中後若有 `changes[]`，取 `at ≤ v` 中最後一個 change 的 status；只有 `version` 時做等值比較
//! 2. 皆不符合 → `defaultStatus`（未提供視為 unknown）
//! 3. 任何會影響結論的版本無法解析 → unknown（不猜）

use std::cmp::Ordering;

use serde_json::Value;

use super::range::{is_placeholder, is_wildcard, lower_bound, upper_bound, within};
use super::{ComponentVersion, Evidence, ProductIds, Source, Verdict};
use crate::matching::version::{Version, VersionScheme};

/// `cve_affected` 的一列。
#[derive(Debug, Clone, PartialEq)]
pub struct AffectedEntry {
    pub source: Source,
    pub provider: Option<String>,
    pub vendor_raw: Option<String>,
    /// `vendor_raw` 以產品名稱規則正規化（載入時計算，比對時不再配置字串）
    pub vendor_raw_norm: Option<String>,
    pub product_raw: Option<String>,
    pub vendor_norm: String,
    pub product_norm: String,
    pub default_status: Option<String>,
    pub versions: Vec<Value>,
    pub cpes: Vec<String>,
    /// `cpes` 的 (vendor, product)，載入時解析一次
    pub cpe_pairs: Vec<(String, String)>,
}

impl AffectedEntry {
    /// vendor/product（正規化）或任一 CPE 與目錄產品相符。
    pub fn matches(&self, ids: &ProductIds) -> bool {
        let by_name = std::iter::once(self.vendor_norm.as_str())
            .chain(self.vendor_raw_norm.as_deref())
            .any(|v| {
                ids.cna
                    .get(v)
                    .is_some_and(|ps| ps.contains(&self.product_norm))
            });
        by_name || self.cpe_pairs.iter().any(|vp| ids.has_cpe(vp))
    }

    fn label(&self) -> String {
        let name = format!(
            "{} {}",
            self.vendor_raw.as_deref().unwrap_or("?"),
            self.product_raw.as_deref().unwrap_or("?")
        );
        match &self.provider {
            Some(p) => format!("{name}（{p}）"),
            None => name,
        }
    }

    /// 是否提供任何版本資訊（非佔位字的 versions 項目，或明確的 defaultStatus）。
    /// `defaultStatus: unknown` 不算：它等於沒有說明其他版本的狀態；有「標示受影響但版本為佔位字」
    /// 的項目時 defaultStatus 也不算（判定不會採用它，見 [`evaluate_entry`]）。
    fn has_version_data(&self) -> bool {
        let explicit_default = self
            .default_status
            .as_deref()
            .is_some_and(|d| Verdict::from_cve5(d) != Verdict::Unknown)
            && !self.versions.iter().any(placeholder_affected);
        explicit_default
            || self.versions.iter().any(|item| {
                let has_range =
                    text(item, "lessThan").is_some() || text(item, "lessThanOrEqual").is_some();
                has_range || text(item, "version").is_some_and(|v| !is_placeholder(v))
            })
    }
}

fn text<'a>(item: &'a Value, key: &str) -> Option<&'a str> {
    item.get(key).and_then(Value::as_str)
}

struct Outcome {
    verdict: Verdict,
    summary: String,
}

fn outcome(verdict: Verdict, summary: String) -> Outcome {
    Outcome { verdict, summary }
}

/// 區間內的 `changes[]`：取適用於 `v` 的變更點（見 [`VersionScheme::change_applies`]）中最大者的 status。
fn apply_changes(
    scheme: &dyn VersionScheme,
    v: &Version,
    changes: &[Value],
    base: Verdict,
) -> Option<(Verdict, Option<String>)> {
    let mut best: Option<(Version, &str, &str)> = None;
    for change in changes {
        let at = text(change, "at")?;
        let parsed = scheme.parse(at)?;
        if !scheme.change_applies(&parsed, v)? {
            continue;
        }
        let newer = match &best {
            None => true,
            Some((cur, _, _)) => scheme.cmp(&parsed, cur)? == Ordering::Greater,
        };
        if newer {
            best = Some((parsed, at, text(change, "status").unwrap_or("unknown")));
        }
    }
    Some(match best {
        None => (base, None),
        Some((_, at, status)) => (Verdict::from_cve5(status), Some(at.to_string())),
    })
}

/// 一個 `versions[]` 項目：None 表示不在此項目內；Err 表示無法判斷（版本無法解析）。
fn check_item(
    scheme: &dyn VersionScheme,
    v: &Version,
    item: &Value,
) -> Result<Option<Outcome>, ()> {
    let status = Verdict::from_cve5(text(item, "status").unwrap_or("unknown"));
    let version = text(item, "version");
    let (upper_raw, inclusive) = match (text(item, "lessThan"), text(item, "lessThanOrEqual")) {
        (Some(lt), _) => (Some(lt), false),
        (None, Some(le)) => (Some(le), true),
        (None, None) => (None, false),
    };
    let Some(upper_raw) = upper_raw else {
        let Some(version) = version.filter(|s| !is_placeholder(s)) else {
            return Ok(None);
        };
        if is_wildcard(version) {
            return Ok(Some(outcome(
                status,
                format!("所有版本：{}", status.label()),
            )));
        }
        let target = scheme.parse(version).ok_or(())?;
        return Ok(scheme
            .covers(&target, v)
            .ok_or(())?
            .then(|| outcome(status, format!("版本 = {version}：{}", status.label()))));
    };
    let lower = lower_bound(scheme, version, true).ok_or(())?;
    let upper = upper_bound(scheme, Some(upper_raw), inclusive).ok_or(())?;
    if !within(scheme, v, &lower, &upper).ok_or(())? {
        return Ok(None);
    }
    let range = format!(
        "{} ≤ v {} {upper_raw}",
        version.unwrap_or("0"),
        if inclusive { "≤" } else { "<" }
    );
    let changes = item
        .get("changes")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let (verdict, at) = apply_changes(scheme, v, changes, status).ok_or(())?;
    let summary = match at {
        Some(at) => format!("{range}，changes at {at}：{}", verdict.label()),
        None => format!("{range}：{}", verdict.label()),
    };
    Ok(Some(outcome(verdict, summary)))
}

/// 沒有區間、版本為佔位字、狀態為受影響的項目。
fn placeholder_affected(item: &Value) -> bool {
    let has_range = text(item, "lessThan").is_some() || text(item, "lessThanOrEqual").is_some();
    !has_range
        && text(item, "version").is_none_or(is_placeholder)
        && Verdict::from_cve5(text(item, "status").unwrap_or("unknown")) == Verdict::Affected
}

fn evaluate_entry(scheme: &dyn VersionScheme, v: &Version, entry: &AffectedEntry) -> Outcome {
    let mut blocked = false;
    for item in &entry.versions {
        match check_item(scheme, v, item) {
            Err(()) => blocked = true,
            Ok(None) => {}
            Ok(Some(_)) if blocked => {
                return outcome(
                    Verdict::Unknown,
                    "較前面的版本項目無法以產品格式解析".into(),
                );
            }
            Ok(Some(found)) => return found,
        }
    }
    if blocked {
        return outcome(Verdict::Unknown, "版本項目無法以產品格式解析".into());
    }
    // `{version: n/a, status: affected}`：產品受影響但版本未知，不可退回 defaultStatus（可能是 unaffected）
    if entry.versions.iter().any(placeholder_affected) {
        return outcome(
            Verdict::Unknown,
            "標示受影響但未提供版本（version 為 n/a 等佔位字）".into(),
        );
    }
    match entry.default_status.as_deref() {
        Some(d) => {
            let verdict = Verdict::from_cve5(d);
            outcome(verdict, format!("不在任何版本項目內，defaultStatus = {d}"))
        }
        None if entry.has_version_data() => outcome(
            Verdict::Unknown,
            "不在任何版本項目內，且未提供 defaultStatus".into(),
        ),
        None => outcome(Verdict::Unknown, "只列出產品，沒有版本資訊".into()),
    }
}

/// 同一來源的多個相符條目：任一受影響 → 受影響；否則任一未知 → 未知；否則不受影響。
pub fn evaluate(
    source: Source,
    entries: &[&AffectedEntry],
    version: &ComponentVersion<'_>,
) -> Option<Evidence> {
    if entries.is_empty() {
        return None;
    }
    let has_data = entries.iter().any(|e| e.has_version_data());
    let scheme = version.scheme;
    let Some(v) = &version.parsed else {
        let mut e = version.unparseable(source);
        e.has_version_data = has_data;
        return Some(e);
    };
    let outcomes: Vec<(Outcome, &AffectedEntry)> = entries
        .iter()
        .map(|entry| (evaluate_entry(scheme, v, entry), *entry))
        .collect();
    // 任一條目受影響 → 受影響；否則有條目無法判定 → 未知（不以其他條目的 defaultStatus 推定不受影響）
    let pick = [Verdict::Affected, Verdict::Unknown, Verdict::NotAffected]
        .into_iter()
        .find_map(|want| outcomes.iter().find(|(o, _)| o.verdict == want))?;
    let (chosen, entry) = pick;
    let mut evidence = Evidence::new(
        source,
        chosen.verdict,
        format!("{}：{}", entry.label(), chosen.summary),
    );
    evidence.has_version_data = has_data;
    Some(evidence)
}
