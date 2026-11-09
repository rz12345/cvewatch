//! 版本格式（version scheme）的寫法驗證：只判斷可否解析；比較邏輯在 `matching::version`。

use std::sync::LazyLock;

use regex::Regex;

use crate::version_text::vendor;

/// 解析結果；`normalized` 為修剪空白、統一小寫後的字串，存入 `component.version_norm`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedVersion {
    pub normalized: String,
}

pub trait VersionScheme: Send + Sync {
    fn name(&self) -> &'static str;
    /// 版本字串可否以此格式解析。
    fn parse(&self, raw: &str) -> Option<ParsedVersion>;
}

/// 以一組正規表示式判斷的格式；`reject` 排除比較器無法判讀的寫法（與比較器共用判斷）。
struct PatternScheme {
    name: &'static str,
    patterns: &'static LazyLock<Vec<Regex>>,
    reject: Option<fn(&str) -> bool>,
}

impl VersionScheme for PatternScheme {
    fn name(&self) -> &'static str {
        self.name
    }

    fn parse(&self, raw: &str) -> Option<ParsedVersion> {
        let normalized = canonical(raw);
        // 比較器以 version_text::canonical 解析（另把 `_` 視為空白），驗證也用同一種寫法判斷；
        // 只有含 `_` 時兩者不同，才需要再正規化一次
        let text = if normalized.contains('_') {
            std::borrow::Cow::Owned(crate::version_text::canonical(&normalized))
        } else {
            std::borrow::Cow::Borrowed(normalized.as_str())
        };
        let accepted = !self.reject.is_some_and(|f| f(&text))
            && self.patterns.iter().any(|re| re.is_match(&text));
        drop(text);
        accepted.then_some(ParsedVersion { normalized })
    }
}

fn canonical(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn compile(patterns: &[&str]) -> Vec<Regex> {
    patterns
        .iter()
        .map(|p| Regex::new(p).expect("固定的正規表示式必定可編譯"))
        .collect()
}

/// `7.10.30.00`、`2.16.0`、`10.0.20348.2700`
static DOTTED: LazyLock<Vec<Regex>> = LazyLock::new(|| compile(&[r"^\d+(\.\d+)*$"]));

/// `11.1.4`、`11.1.4-h7`
static PANOS: LazyLock<Vec<Regex>> = LazyLock::new(|| compile(&[r"^\d+\.\d+\.\d+(-h\d+)?$"]));

/// `7.2.1-69057`、`7.2.2-72806 update 3`
static DSM: LazyLock<Vec<Regex>> =
    LazyLock::new(|| compile(&[r"^\d+\.\d+(\.\d+)?-\d+( ?update ?\d+)?$"]));

/// `8.0 u3c`、`8.0 update 3c`、`8.0.3`、`8.0u3 p01`、build `24414501`、`8.0.3 build-24414501`
static ESXI: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    compile(&[
        r"^\d+\.\d+(\.\d+)?[a-z]?( ?(u|update) ?\d+[a-z]?)?( ?(p|patch) ?\d+)?( ?(ep|express patch) ?\d+)?$",
        r"^(build[- ]?)?\d{6,9}$",
        r"^\d+\.\d+(\.\d+)?[ ,]*\(?build[- ]?\d{6,9}\)?$",
    ])
});

/// `22.04`、`22.04.4`、`22.04.4 lts`
static UBUNTU: LazyLock<Vec<Regex>> =
    LazyLock::new(|| compile(&[r"^\d{2}\.\d{2}(\.\d+)?( ?lts)?$"]));

/// Cisco `15.2(7)e8`、`9.18(4)22`、`10.2(3t)`；ASA 點分寫法 `9.18.4.22`
static CISCO: LazyLock<Vec<Regex>> =
    LazyLock::new(|| vec![vendor::CISCO.clone(), vendor::DOTTED_FOUR.clone()]);

/// Juniper `21.4r3-s5`、Ivanti `22.7r2.4`、`25.1.1.3`
static JUNOS: LazyLock<Vec<Regex>> =
    LazyLock::new(|| vec![vendor::JUNOS.clone(), vendor::DOTTED_FOUR.clone()]);

/// `7.0.1-5145`、`14.1-29.72`、`5.1.7.2770 build 20240520`、`r81.20`
static NUMERIC: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![vendor::NUMERIC.clone()]);

/// Zyxel `v2.80(abcd.1)c0`
static ZYXEL: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![vendor::ZYXEL.clone()]);

static SCHEMES: [PatternScheme; 9] = [
    PatternScheme {
        name: "dotted",
        patterns: &DOTTED,
        reject: None,
    },
    PatternScheme {
        name: "panos",
        patterns: &PANOS,
        reject: None,
    },
    PatternScheme {
        name: "dsm",
        patterns: &DSM,
        reject: None,
    },
    PatternScheme {
        name: "esxi",
        patterns: &ESXI,
        reject: Some(crate::version_text::esxi::is_ambiguous),
    },
    PatternScheme {
        name: "ubuntu",
        patterns: &UBUNTU,
        reject: None,
    },
    PatternScheme {
        name: "cisco",
        patterns: &CISCO,
        reject: None,
    },
    PatternScheme {
        name: "junos",
        patterns: &JUNOS,
        reject: None,
    },
    PatternScheme {
        name: "numeric",
        patterns: &NUMERIC,
        reject: None,
    },
    PatternScheme {
        name: "zyxel",
        patterns: &ZYXEL,
        reject: None,
    },
];

pub fn scheme_by_name(name: &str) -> Option<&'static dyn VersionScheme> {
    SCHEMES
        .iter()
        .find(|s| s.name == name)
        .map(|s| s as &dyn VersionScheme)
}

pub fn scheme_names() -> Vec<&'static str> {
    SCHEMES.iter().map(|s| s.name).collect()
}
