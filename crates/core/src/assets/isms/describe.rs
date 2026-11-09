//! 資產描述 → 元件（17 design D4）：拆行、去除標籤／註記／數量、以產品目錄別名比對、
//! 中文前綴重試、同產品合併；比對不到的行原文保留並附相似產品。
//!
//! 不自動採用相似產品：門檻內的相似別名可能是不同型號，會直接影響 CVE 比對結果。

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::catalog::Catalog;

/// 拆行的分隔字元（NFKC 之後；全形逗號、分號已轉為半形）。
const SEPARATORS: [char; 6] = ['\n', '\r', '、', ',', ';', '，'];

/// 結尾的數量 `*2`、`×2`，可帶單位。
static STAR_QUANTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*[*×]\s*\d+\s*[台部套組個]?\s*$").expect("數量規則"));

/// 結尾的數量 ` x2`：x 前須為空白（避免吃掉型號中的 X），可帶單位。
static X_QUANTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+[xX]\s*(\d+)\s*([台部套組個]?)\s*$").expect("數量規則"));

/// ` x64`、` x86` 這類寫法是平台而不是數量（沒有單位時保留）。
const PLATFORM_BITS: [&str; 3] = ["32", "64", "86"];

/// 冒號前的標籤最多幾個字、幾個詞（`Core:`、`邊緣：`、`Core Switch:`）。
const LABEL_MAX_CHARS: usize = 16;
const LABEL_MAX_WORDS: usize = 2;

/// 括號與括號內的註記（NFKC 之後全形括號已轉為半形）。
static NOTES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\([^)]*\)|\[[^\]]*\]|【[^】]*】").expect("註記規則"));

/// 比對結果的一項（依描述中第一次出現的順序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescribedItem {
    /// 比對到目錄產品；`lines` 為比對到同一產品的描述原文行（兩行以上即為合併）
    Resolved { key: String, lines: Vec<String> },
    /// 比對不到：`text` 為清理後的文字（寫入草稿的 product），`line` 為原文行
    Unresolved {
        text: String,
        line: String,
        suggestions: Vec<String>,
    },
}

/// 依換行、頓號、逗號與分號拆開（原文、已去除前後空白，不含空行）。
pub fn split_lines(description: &str) -> Vec<String> {
    description
        .nfkc()
        .collect::<String>()
        .split(|c| SEPARATORS.contains(&c))
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// 去除冒號前的標籤。URL（冒號後接 `/`）、時間與含數字的名稱（`Windows Server 2019: Standard`）
/// 的冒號不是標籤。
fn strip_label(line: &str) -> &str {
    let Some((label, rest)) = line.split_once(':') else {
        return line;
    };
    let label = label.trim();
    let is_label = !label.is_empty()
        && label.chars().count() <= LABEL_MAX_CHARS
        && label.split_whitespace().count() <= LABEL_MAX_WORDS
        && !label.chars().any(|c| c.is_ascii_digit())
        && !rest.starts_with('/');
    if is_label {
        rest
    } else {
        line
    }
}

/// 去除結尾數量（平台寫法 ` x64` 保留）。
fn strip_quantity(line: &str) -> String {
    let line = STAR_QUANTITY.replace(line, "");
    let platform = X_QUANTITY.captures(&line).is_some_and(|c| {
        PLATFORM_BITS.contains(&&c[1]) && c.get(2).is_none_or(|u| u.as_str().is_empty())
    });
    if platform {
        line.into_owned()
    } else {
        X_QUANTITY.replace(&line, "").into_owned()
    }
}

/// 一行的清理：去除冒號前的標籤、括號註記與結尾數量。
pub fn clean_line(line: &str) -> String {
    let line: String = line.nfkc().collect();
    let line = NOTES.replace_all(strip_label(&line), " ");
    let line = strip_quantity(line.trim());
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 資產名稱：描述第一個清理後非空的行；沒有時為空字串。
pub fn display_name(description: &str) -> String {
    split_lines(description)
        .iter()
        .map(|l| clean_line(l))
        .find(|l| !l.is_empty())
        .unwrap_or_default()
}

fn is_cjk(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}

/// 以中文字開頭且後面有英數字時，去除開頭連續的中文字詞與空白（「備份軟體 Veeam …」→「Veeam …」）。
fn without_chinese_prefix(text: &str) -> Option<&str> {
    let first = text.chars().next()?;
    if !is_cjk(first) || !text.chars().any(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let rest = text.trim_start_matches(|c: char| is_cjk(c) || c.is_whitespace());
    (!rest.is_empty()).then_some(rest)
}

fn resolve<'c>(catalog: &'c Catalog, text: &str) -> Option<&'c str> {
    catalog
        .resolve(text)
        .or_else(|| without_chinese_prefix(text).and_then(|t| catalog.resolve(t)))
        .map(|p| p.key.as_str())
}

fn suggestions(catalog: &Catalog, text: &str) -> Vec<String> {
    let mut found = catalog.suggest(text);
    if found.is_empty() {
        if let Some(t) = without_chinese_prefix(text) {
            found = catalog.suggest(t);
        }
    }
    found.into_iter().map(str::to_string).collect()
}

/// 資產描述 → 比對結果；同一產品的多行合併為一項。
pub fn describe(description: &str, catalog: &Catalog) -> Vec<DescribedItem> {
    let mut items: Vec<DescribedItem> = Vec::new();
    for line in split_lines(description) {
        let text = clean_line(&line);
        if text.is_empty() {
            continue;
        }
        let Some(key) = resolve(catalog, &text) else {
            items.push(DescribedItem::Unresolved {
                suggestions: suggestions(catalog, &text),
                text,
                line,
            });
            continue;
        };
        let merged = items.iter_mut().find_map(|item| match item {
            DescribedItem::Resolved { key: k, lines } if k == key => Some(lines),
            _ => None,
        });
        match merged {
            Some(lines) => lines.push(line),
            None => items.push(DescribedItem::Resolved {
                key: key.to_string(),
                lines: vec![line],
            }),
        }
    }
    items
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::catalog::{CatalogSource, SEED_SOURCE_NAME, SEED_TOML};

    /// 測試專用的別名（虛構型號；與 CLI、GUI 測試共用的 fixture）。
    pub(crate) const TEST_ALIASES: &str =
        include_str!("../../../../../tests/fixtures/isms_catalog.d/isms-test.toml");

    pub(crate) fn catalog() -> Catalog {
        Catalog::from_sources(&[
            CatalogSource {
                name: SEED_SOURCE_NAME.into(),
                text: SEED_TOML.into(),
            },
            CatalogSource {
                name: "test.toml".into(),
                text: TEST_ALIASES.into(),
            },
        ])
        .expect("測試目錄")
    }

    #[test]
    fn lines_are_cleaned() {
        assert_eq!(clean_line("Core: Aruba XYZ-100 *2"), "Aruba XYZ-100");
        assert_eq!(clean_line("Zyxel ABCD ＊1"), "Zyxel ABCD");
        assert_eq!(clean_line("Aruba XYZ-100 x 6"), "Aruba XYZ-100");
        assert_eq!(clean_line("Aruba XYZ-100 ×2台"), "Aruba XYZ-100");
        assert_eq!(clean_line("NAS（備援） *1"), "NAS");
        assert_eq!(
            clean_line("Zyxel ABX 100"),
            "Zyxel ABX 100",
            "型號中的 X 不是數量"
        );
        assert_eq!(clean_line("X100"), "X100");
        assert_eq!(
            clean_line("Windows 11 x64"),
            "Windows 11 x64",
            "平台不是數量"
        );
        assert_eq!(clean_line("Windows 11 x 2台"), "Windows 11");
        assert_eq!(
            clean_line("https://www.example.edu.tw"),
            "https://www.example.edu.tw"
        );
        assert_eq!(
            clean_line("Windows Server 2019: Standard"),
            "Windows Server 2019: Standard"
        );
        assert_eq!(clean_line("Core Switch: Aruba XYZ-100"), "Aruba XYZ-100");
        assert_eq!(clean_line("Core:"), "", "只有標籤的行");
        assert_eq!(clean_line("邊緣：Aruba XYZ-100"), "Aruba XYZ-100");
        assert_eq!(
            display_name("\n *2\nSynology XYZ-200 *1"),
            "Synology XYZ-200"
        );
    }

    #[test]
    fn multi_line_description() {
        let items = describe("Core: Aruba XYZ-100 *2、\nZyxel ABCD *1", &catalog());
        assert_eq!(
            items,
            vec![
                DescribedItem::Resolved {
                    key: "hpe-aoscx".into(),
                    lines: vec!["Core: Aruba XYZ-100 *2".into()],
                },
                DescribedItem::Unresolved {
                    text: "Zyxel ABCD".into(),
                    line: "Zyxel ABCD *1".into(),
                    suggestions: Vec::new(),
                },
            ]
        );
    }

    #[test]
    fn leading_chinese_words() {
        let items = describe("備份軟體 Veeam Backup & Replication", &catalog());
        assert!(
            matches!(&items[..], [DescribedItem::Resolved { key, .. }] if key == "veeam-backup-replication"),
            "{items:?}"
        );
        assert_eq!(without_chinese_prefix("官網系統"), None, "沒有英數字");
    }

    #[test]
    fn same_switch_used_twice() {
        let items = describe("Core: Aruba XYZ-100 *2、Edge: Aruba XYZ-100 *9", &catalog());
        assert_eq!(
            items,
            vec![DescribedItem::Resolved {
                key: "hpe-aoscx".into(),
                lines: vec![
                    "Core: Aruba XYZ-100 *2".into(),
                    "Edge: Aruba XYZ-100 *9".into()
                ],
            }]
        );
    }

    #[test]
    fn unresolved_lines_carry_suggestions() {
        let items = describe("Veeam Backup Replicatio", &catalog());
        let DescribedItem::Unresolved { suggestions, .. } = &items[0] else {
            panic!("{items:?}");
        };
        assert!(
            suggestions.iter().any(|s| s == "veeam-backup-replication"),
            "{suggestions:?}"
        );
    }
}
