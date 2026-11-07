//! 版本格式（version scheme）。02 只判斷可否解析；比較邏輯在 04 實作。

use std::sync::LazyLock;

use regex::Regex;

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

/// 以一組正規表示式判斷的格式。
struct PatternScheme {
    name: &'static str,
    patterns: &'static LazyLock<Vec<Regex>>,
}

impl VersionScheme for PatternScheme {
    fn name(&self) -> &'static str {
        self.name
    }

    fn parse(&self, raw: &str) -> Option<ParsedVersion> {
        let normalized = canonical(raw);
        self.patterns
            .iter()
            .any(|re| re.is_match(&normalized))
            .then_some(ParsedVersion { normalized })
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
        r"^\d+\.\d+(\.\d+)?( ?(u|update) ?\d+[a-z]?)?( ?(p|patch) ?\d+)?( ?(ep|express patch) ?\d+)?$",
        r"^(build[- ]?)?\d{6,9}$",
        r"^\d+\.\d+(\.\d+)?[ ,]*\(?build[- ]?\d{6,9}\)?$",
    ])
});

/// `22.04`、`22.04.4`、`22.04.4 lts`
static UBUNTU: LazyLock<Vec<Regex>> =
    LazyLock::new(|| compile(&[r"^\d{2}\.\d{2}(\.\d+)?( ?lts)?$"]));

static SCHEMES: [PatternScheme; 5] = [
    PatternScheme {
        name: "dotted",
        patterns: &DOTTED,
    },
    PatternScheme {
        name: "panos",
        patterns: &PANOS,
    },
    PatternScheme {
        name: "dsm",
        patterns: &DSM,
    },
    PatternScheme {
        name: "esxi",
        patterns: &ESXI,
    },
    PatternScheme {
        name: "ubuntu",
        patterns: &UBUNTU,
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
