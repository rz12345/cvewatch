//! 版本區間判定：CVE 5.x / NVD 的上下界與 `vers:` 字串，一律以產品的 scheme 比較。

use std::cmp::Ordering;

use crate::ingest::csaf::vers::VersRange;
use crate::matching::version::{at_most, Version, VersionScheme};

/// 視為「沒有版本資訊」的寫法（CVE 5.x 常見）。
const PLACEHOLDERS: [&str; 5] = ["", "n/a", "unspecified", "unknown", "-"];
/// 代表「所有版本」的寫法。
const WILDCARDS: [&str; 2] = ["*", "all"];

pub fn is_placeholder(s: &str) -> bool {
    PLACEHOLDERS.contains(&s.trim().to_ascii_lowercase().as_str())
}

pub fn is_wildcard(s: &str) -> bool {
    WILDCARDS.contains(&s.trim().to_ascii_lowercase().as_str())
}

/// 區間端點；`Unbounded` 為無限制（`0`、`*` 或未提供）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound {
    Unbounded,
    At { version: Version, inclusive: bool },
}

/// 下界：`0`、`*`、佔位字視為無下界；無法解析為 None。
pub fn lower_bound(
    scheme: &dyn VersionScheme,
    raw: Option<&str>,
    inclusive: bool,
) -> Option<Bound> {
    match raw.map(str::trim) {
        None => Some(Bound::Unbounded),
        Some(s) if s == "0" || is_wildcard(s) || is_placeholder(s) => Some(Bound::Unbounded),
        Some(s) => scheme
            .parse(s)
            .map(|version| Bound::At { version, inclusive }),
    }
}

/// 上界：`*` 或未提供視為無上界；無法解析為 None。
pub fn upper_bound(
    scheme: &dyn VersionScheme,
    raw: Option<&str>,
    inclusive: bool,
) -> Option<Bound> {
    match raw.map(str::trim) {
        None => Some(Bound::Unbounded),
        Some(s) if is_wildcard(s) => Some(Bound::Unbounded),
        Some(s) => scheme
            .parse(s)
            .map(|version| Bound::At { version, inclusive }),
    }
}

/// `v` 是否在 lower..upper 內；比較失敗時為 None。
pub fn within(
    scheme: &dyn VersionScheme,
    v: &Version,
    lower: &Bound,
    upper: &Bound,
) -> Option<bool> {
    let above = match lower {
        Bound::Unbounded => true,
        Bound::At {
            version,
            inclusive: true,
        } => scheme.cmp(v, version)? != Ordering::Less,
        // 排除的下界也排除它涵蓋的版本線（與 ≤ 上界一致：DSM `> 7.2.1` 不含 7.2.1 的任何 build）
        Bound::At {
            version,
            inclusive: false,
        } => scheme.cmp(v, version)? == Ordering::Greater && !scheme.covers(version, v)?,
    };
    let below = match upper {
        Bound::Unbounded => true,
        Bound::At {
            version,
            inclusive: true,
        } => at_most(scheme, v, version)?,
        Bound::At {
            version,
            inclusive: false,
        } => scheme.cmp(v, version)? == Ordering::Less,
    };
    Some(above && below)
}

/// `vers:` 區間的命中結果；`lower` 為命中區段的下界（越大越具體），無下界為 None。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersHit {
    pub lower: Option<Version>,
}

struct Parsed {
    op: String,
    version: Version,
}

fn satisfies(scheme: &dyn VersionScheme, v: &Version, c: &Parsed) -> Option<bool> {
    let o = scheme.cmp(v, &c.version)?;
    Some(match c.op.as_str() {
        ">" => o == Ordering::Greater && !scheme.covers(&c.version, v)?,
        ">=" => o != Ordering::Less,
        "<" => o == Ordering::Less,
        "<=" => o == Ordering::Less || scheme.covers(&c.version, v)?,
        "=" => scheme.covers(&c.version, v)?,
        "!=" => !scheme.covers(&c.version, v)?,
        _ => false,
    })
}

fn is_lower(op: &str) -> bool {
    op == ">" || op == ">="
}

/// 依 vers 規格判定 `v` 是否在區間內。外層 None：版本無法解析或比較；`Some(None)`：不在區間內。
pub fn vers_contains(
    scheme: &dyn VersionScheme,
    v: &Version,
    range: &VersRange,
) -> Option<Option<VersHit>> {
    if range.constraints.iter().any(|c| c.op == "*") {
        return Some(Some(VersHit { lower: None }));
    }
    let parsed = range
        .constraints
        .iter()
        .map(|c| {
            scheme.parse(&c.version).map(|version| Parsed {
                op: c.op.clone(),
                version,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    if let Some(decided) = equality_hit(scheme, v, &parsed)? {
        return Some(decided);
    }
    let mut bounds: Vec<&Parsed> = parsed
        .iter()
        .filter(|c| c.op != "=" && c.op != "!=")
        .collect();
    if bounds.is_empty() {
        // 只有 `!=`（且皆通過）：除了排除的版本以外都在區間內；只有 `=` 而未命中：不在區間內
        let only_exclusions = parsed.iter().any(|c| c.op == "!=");
        return Some(only_exclusions.then_some(VersHit { lower: None }));
    }
    if bounds.iter().any(|a| {
        bounds
            .iter()
            .any(|b| scheme.cmp(&a.version, &b.version).is_none())
    }) {
        return None;
    }
    bounds.sort_by(|a, b| {
        scheme
            .cmp(&a.version, &b.version)
            .unwrap_or(Ordering::Equal)
    });
    interval_hit(scheme, v, &bounds)
}

/// `=` 命中 → 在區間內（下界即該版本）；`!=` 命中排除 → 不在區間內；都沒決定 → None。
fn equality_hit(
    scheme: &dyn VersionScheme,
    v: &Version,
    parsed: &[Parsed],
) -> Option<Option<Option<VersHit>>> {
    for c in parsed.iter().filter(|c| c.op == "=" || c.op == "!=") {
        if c.op == "=" && satisfies(scheme, v, c)? {
            return Some(Some(Some(VersHit {
                lower: Some(c.version.clone()),
            })));
        }
        if c.op == "!=" && !satisfies(scheme, v, c)? {
            return Some(Some(None));
        }
    }
    Some(None)
}

/// 排序後的比較子兩兩成區段：下界後接上界為一段；開頭的上界與結尾的下界各自成段。
fn interval_hit(
    scheme: &dyn VersionScheme,
    v: &Version,
    bounds: &[&Parsed],
) -> Option<Option<VersHit>> {
    let mut best: Option<VersHit> = None;
    for (i, c) in bounds.iter().enumerate() {
        let hit = if is_lower(&c.op) {
            let next_upper = bounds.get(i + 1).filter(|n| !is_lower(&n.op));
            // 只有下界的區間（Palo Alto 以此表示各版本線的修正點）依變更點規則判斷適用範圍
            let in_upper = match next_upper {
                Some(n) => satisfies(scheme, v, n)?,
                None => scheme.change_applies(&c.version, v)?,
            };
            (satisfies(scheme, v, c)? && in_upper).then(|| VersHit {
                lower: Some(c.version.clone()),
            })
        } else {
            let opens = i == 0 || !is_lower(&bounds[i - 1].op);
            (opens && satisfies(scheme, v, c)?).then_some(VersHit { lower: None })
        };
        if let Some(h) = hit {
            best = Some(more_specific(scheme, best, h));
        }
    }
    Some(best)
}

/// 兩個命中取下界較大者（較具體）。
pub fn more_specific(
    scheme: &dyn VersionScheme,
    current: Option<VersHit>,
    new: VersHit,
) -> VersHit {
    match current {
        None => new,
        Some(cur) => match (&cur.lower, &new.lower) {
            (_, None) => cur,
            (None, Some(_)) => new,
            (Some(a), Some(b)) => match scheme.cmp(b, a) {
                Some(Ordering::Greater) => new,
                _ => cur,
            },
        },
    }
}
