//! `numeric`：數字分段，分隔可為 `.`、`-`、`_`（視為空白）或 `build`，可有一個字母前綴。
//!
//! SonicOS `7.0.1-5145`、NetScaler `14.1-29.72`、QNAP `5.1.7.2770 build 20240520`、QuTS hero `h5.1.7`、
//! Check Point `R81.20`、Acronis `15 build 41800`。逐段依數值比較、缺段補 0；
//! 前綴字母只用於辨識：前綴不同（例如 `h` 與無前綴）時比較結果為未知。
//! 開頭的 `v` 視為版本標記而非前綴；結尾的 `.x`（CNA 的 `5.1.x`）代表整條版本線：排序時為該版本線的起點，
//! 等值與 `≤` 上界時涵蓋該版本線的所有版本。NVD CPE 的 update 為 `*` 時，較短的版本涵蓋以它開頭的版本
//! （`5.1.7.2770` 涵蓋 `5.1.7.2770 build 20240520`）。

use std::cmp::Ordering;

use super::{canonical, Version, VersionScheme};
use crate::version_text::vendor::NUMERIC;

/// 結尾的版本線萬用字元（`5.1.x`、`5.1.*`）。
const LINE_SUFFIXES: [&str; 2] = [".x", ".*"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Numeric {
    /// 字母前綴（小寫）；沒有時為 None
    pub prefix: Option<char>,
    pub numbers: Vec<u64>,
    /// 結尾為 `.x`：代表以 `numbers` 開頭的整條版本線
    pub line: bool,
}

impl Numeric {
    pub fn parse(raw: &str) -> Option<Self> {
        let text = canonical(raw);
        let caps = NUMERIC.captures(&text)?;
        let prefix = caps.get(1).and_then(|m| m.as_str().chars().next());
        let numbers = caps[2]
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()?;
        let line = LINE_SUFFIXES.iter().any(|s| text.ends_with(s));
        Some(Numeric {
            prefix,
            numbers,
            line,
        })
    }

    /// `v` 以本版本的數字段開頭（同一前綴）。
    fn starts(&self, v: &Numeric) -> bool {
        self.prefix == v.prefix && v.numbers.starts_with(&self.numbers)
    }
}

pub struct NumericScheme;

impl VersionScheme for NumericScheme {
    fn name(&self) -> &'static str {
        "numeric"
    }

    fn parse(&self, raw: &str) -> Option<Version> {
        Numeric::parse(raw).map(Version::Numeric)
    }

    fn cmp(&self, a: &Version, b: &Version) -> Option<Ordering> {
        match (a, b) {
            (Version::Numeric(x), Version::Numeric(y)) if x.prefix == y.prefix => {
                Some(super::cmp_padded(&x.numbers, &y.numbers))
            }
            _ => None,
        }
    }

    fn covers(&self, bound: &Version, v: &Version) -> Option<bool> {
        match (bound, v) {
            (Version::Numeric(b), Version::Numeric(x)) if b.prefix == x.prefix => Some(if b.line {
                b.starts(x)
            } else {
                super::cmp_padded(&b.numbers, &x.numbers).is_eq()
            }),
            _ => None,
        }
    }

    fn same_line(&self, base: &Version, v: &Version) -> Option<bool> {
        match (base, v) {
            (Version::Numeric(b), Version::Numeric(x)) if b.prefix == x.prefix => Some(b.starts(x)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(a: &str, b: &str) -> Option<Ordering> {
        super::super::compare(&NumericScheme, a, b)
    }

    #[test]
    fn numeric_segments_with_build_numbers() {
        assert_eq!(cmp("14.1-8.50", "14.1-29.72"), Some(Ordering::Less));
        assert_eq!(
            cmp("5.1.7.2770 build 20240315", "5.1.7.2770 build 20240520"),
            Some(Ordering::Less)
        );
        assert_eq!(cmp("7.0.1-5145", "7.0.1_5145"), Some(Ordering::Equal));
        assert_eq!(
            cmp("15 build 41800", "15 Build 39938"),
            Some(Ordering::Greater)
        );
        assert_eq!(cmp("7.0.1", "7.0.1-0"), Some(Ordering::Equal));
    }

    #[test]
    fn letter_prefix_must_match() {
        assert_eq!(cmp("R81.20", "r81.10"), Some(Ordering::Greater));
        assert_eq!(cmp("h5.1.7", "5.1.7"), None);
        assert_eq!(cmp("v7.0.1", "7.0.1"), Some(Ordering::Equal));
    }

    #[test]
    fn trailing_x_is_the_start_of_the_line() {
        assert_eq!(cmp("5.1.x", "5.1.0"), Some(Ordering::Equal));
        assert_eq!(
            cmp("h5.1.x", "h5.1.7.2770 build 20240520"),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn trailing_x_covers_the_whole_line() {
        let bound = NumericScheme.parse("5.1.x").unwrap();
        let inside = NumericScheme.parse("5.1.7.2770 build 20240520").unwrap();
        let other = NumericScheme.parse("5.2.0").unwrap();
        assert_eq!(NumericScheme.covers(&bound, &inside), Some(true));
        assert_eq!(NumericScheme.covers(&bound, &other), Some(false));
        let exact = NumericScheme.parse("5.1").unwrap();
        assert_eq!(NumericScheme.covers(&exact, &inside), Some(false));
        assert_eq!(
            NumericScheme.covers(&exact, &NumericScheme.parse("5.1.0").unwrap()),
            Some(true)
        );
    }

    #[test]
    fn nvd_any_update_covers_longer_versions() {
        let base = NumericScheme.parse("5.1.7.2770").unwrap();
        let v = NumericScheme.parse("5.1.7.2770 build 20240520").unwrap();
        assert_eq!(NumericScheme.same_line(&base, &v), Some(true));
        let other = NumericScheme.parse("5.1.7.2771").unwrap();
        assert_eq!(NumericScheme.same_line(&base, &other), Some(false));
        let hero = NumericScheme.parse("h5.1.7.2770").unwrap();
        assert_eq!(NumericScheme.same_line(&base, &hero), None);
    }

    #[test]
    fn rejects_other_spellings() {
        assert!(Numeric::parse("21.0 MR1").is_none());
        assert!(Numeric::parse("R81.20 Jumbo Take 50").is_none());
        assert!(Numeric::parse("ab1.0").is_none());
    }
}
