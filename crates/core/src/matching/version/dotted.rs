//! `dotted`：數字段序列（缺段補 0），可帶以字母開頭的尾碼。
//!
//! 尾碼排序：預覽版（dev < alpha < beta < pre/preview < rc）早於正式版，其他尾碼（`1.0.2a`、`p1`）
//! 晚於正式版；尾碼內的數字依數值比較（`rc2 < rc10`）。
//! `ubuntu` 沿用相同規則，另忽略結尾的 `LTS`。

use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

use super::{canonical, Version, VersionScheme};

/// `7.16.2`、`v2.16.0`、`1.2.3-rc1`、`1.0a`
static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^v?(\d+(?:\.\d+)*)(?:[-+~. ]?([a-z][0-9a-z.+~-]*))?$")
        .expect("固定的正規表示式必定可編譯")
});

const LTS_SUFFIX: &str = " lts";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dotted {
    pub numbers: Vec<u64>,
    /// 非數字尾碼（小寫）；沒有時為空字串
    pub suffix: String,
}

impl Dotted {
    pub fn parse(raw: &str) -> Option<Self> {
        Self::parse_canonical(&canonical(raw))
    }

    /// `text` 已經 [`canonical`] 正規化。
    fn parse_canonical(text: &str) -> Option<Self> {
        let caps = PATTERN.captures(text)?;
        let numbers = caps[1]
            .split('.')
            .map(|n| n.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()?;
        let suffix = caps.get(2).map_or("", |m| m.as_str()).to_string();
        Some(Dotted { numbers, suffix })
    }

    fn cmp_with(&self, other: &Dotted) -> Ordering {
        super::cmp_padded(&self.numbers, &other.numbers)
            .then_with(|| suffix_key(&self.suffix).cmp(&suffix_key(&other.suffix)))
    }
}

/// 預覽版標記與先後順序：dev < alpha < beta < pre / preview < rc（皆早於正式版）。
const PRE_RELEASE: [(&str, u8); 6] = [
    ("dev", 0),
    ("alpha", 1),
    ("beta", 2),
    ("preview", 3),
    ("pre", 3),
    ("rc", 4),
];

/// 尾碼分類：預覽版 < 無尾碼 < 其他尾碼（例如 OpenSSL `1.0.2a`、`p1`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SuffixClass {
    PreRelease,
    Release,
    PostRelease,
}

/// 自然排序的片段：數字依數值比較（`rc2 < rc10`），數字排在文字之前。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Num(u64),
    Text(String),
}

fn natural(s: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, parts: &mut Vec<Part>| {
        if current.is_empty() {
            return;
        }
        let token = std::mem::take(current);
        parts.push(match token.parse::<u64>() {
            Ok(n) if token.bytes().all(|b| b.is_ascii_digit()) => Part::Num(n),
            _ => Part::Text(token),
        });
    };
    for c in s.chars() {
        if c.is_alphanumeric() {
            let switches = current
                .chars()
                .last()
                .is_some_and(|last| last.is_ascii_digit() != c.is_ascii_digit());
            if switches {
                flush(&mut current, &mut parts);
            }
            current.push(c);
        } else {
            flush(&mut current, &mut parts);
        }
    }
    flush(&mut current, &mut parts);
    parts
}

fn suffix_key(suffix: &str) -> (SuffixClass, u8, Vec<Part>) {
    if suffix.is_empty() {
        return (SuffixClass::Release, 0, Vec::new());
    }
    let pre = PRE_RELEASE.iter().find_map(|(marker, rank)| {
        let rest = suffix.strip_prefix(marker)?;
        let whole_word = !rest.starts_with(|c: char| c.is_ascii_alphabetic());
        whole_word.then_some((*rank, rest))
    });
    match pre {
        Some((rank, rest)) => (SuffixClass::PreRelease, rank, natural(rest)),
        None => (SuffixClass::PostRelease, 0, natural(suffix)),
    }
}

pub struct DottedScheme {
    name: &'static str,
    strip_lts: bool,
}

impl DottedScheme {
    pub fn dotted() -> Self {
        DottedScheme {
            name: "dotted",
            strip_lts: false,
        }
    }

    pub fn ubuntu() -> Self {
        DottedScheme {
            name: "ubuntu",
            strip_lts: true,
        }
    }
}

impl VersionScheme for DottedScheme {
    fn name(&self) -> &'static str {
        self.name
    }

    fn parse(&self, raw: &str) -> Option<Version> {
        let text = canonical(raw);
        let text = match self.strip_lts {
            true => text.strip_suffix(LTS_SUFFIX).unwrap_or(&text),
            false => &text,
        };
        Dotted::parse_canonical(text).map(Version::Dotted)
    }

    fn cmp(&self, a: &Version, b: &Version) -> Option<Ordering> {
        match (a, b) {
            (Version::Dotted(x), Version::Dotted(y)) => Some(x.cmp_with(y)),
            _ => None,
        }
    }

    /// 數字段相同即屬同一版本線（尾碼不限）。
    fn same_line(&self, base: &Version, v: &Version) -> Option<bool> {
        match (base, v) {
            (Version::Dotted(b), Version::Dotted(x)) => {
                let plain = |d: &Dotted| Dotted {
                    numbers: d.numbers.clone(),
                    suffix: String::new(),
                };
                Some(plain(b).cmp_with(&plain(x)) == Ordering::Equal)
            }
            _ => None,
        }
    }
}
