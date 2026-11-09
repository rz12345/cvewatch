//! ISMS 匯出的資產描述（23 design D5、D6）：廠牌型號行＋每個清冊元件一行「產品（版本）」，
//! 組好後立刻以 ISMS 匯入的描述規則讀回檢查，讀回結果與資產不同時產生警告。
//!
//! - 廠牌型號行寫成「廠牌型號：【…】」：讀回時標籤與【】註記都被去除，整行略過
//! - 版本放在全形括號內：讀回時被註記規則去除，剩下的產品以完全比對解析
//! - 廠牌、型號與版本中的分隔字元、冒號與括號換成空白；產品文字不替換，由讀回檢查把關

use std::collections::HashSet;

use unicode_normalization::UnicodeNormalization;

use super::describe::{
    describe_resolution, is_note_bracket, split_lines, DescribedItem, SEPARATORS,
};
use crate::assets::export::InventoryItem;
use crate::assets::model::is_pending_version;
use crate::catalog::{normalize, Catalog};

/// 廠牌型號行的標籤（讀回時被視為標籤去除）。
pub const MODEL_LABEL: &str = "廠牌型號";

/// 組成描述所需的資產欄位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescriptionIdentity<'a> {
    /// 警告中的資產編號
    pub asset_tag: &'a str,
    pub brand: Option<&'a str>,
    pub model: Option<&'a str>,
}

/// 組成結果：描述文字（以 `\n` 連接）與讀回檢查的警告。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Composed {
    pub text: String,
    pub warnings: Vec<String>,
}

/// 廠牌、型號與版本的字元替換：NFKC 後把分隔字元、冒號與註記括號換成空白並合併連續空白。
pub fn sanitize(value: &str) -> String {
    value
        .nfkc()
        .map(|c| {
            if SEPARATORS.contains(&c) || c == ':' || is_note_bracket(c) {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 廠牌型號行；廠牌與型號都沒有值時為 None。
pub fn model_line(brand: Option<&str>, model: Option<&str>) -> Option<String> {
    let parts: Vec<String> = [brand, model]
        .into_iter()
        .flatten()
        .map(sanitize)
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| format!("{MODEL_LABEL}：【{}】", parts.join(" ")))
}

/// 元件行：「產品（版本）」；版本待補（或替換後為空）時只寫產品。
pub fn component_line(product: &str, version: &str) -> String {
    let version = if is_pending_version(version) {
        String::new()
    } else {
        sanitize(version)
    };
    if version.is_empty() {
        product.to_string()
    } else {
        format!("{product}（{version}）")
    }
}

/// 讀回結果的文字（警告用）。
fn readback_text(items: &[DescribedItem]) -> String {
    items
        .iter()
        .map(|item| match item {
            DescribedItem::Resolved { key, .. } => key.clone(),
            DescribedItem::Unresolved { text, .. } => text.clone(),
        })
        .collect::<Vec<_>>()
        .join("、")
}

/// 一行單獨讀回的結果是否與元件相同。
fn reads_back(found: &[DescribedItem], item: &InventoryItem) -> bool {
    match (found, &item.key) {
        ([DescribedItem::Resolved { key, .. }], Some(expected)) => key == expected,
        ([DescribedItem::Unresolved { text, .. }], None) => {
            normalize(text) == normalize(&item.product)
        }
        _ => false,
    }
}

/// 讀回不一致的警告。
fn mismatch_warning(tag: &str, found: &[DescribedItem], item: &InventoryItem) -> String {
    match (found, &item.key) {
        ([], _) => format!("{tag} 的「{}」以 ISMS 匯入讀回時會被略過", item.product),
        ([DescribedItem::Resolved { key, .. }], None) => {
            format!("{tag} 的未辨識產品「{}」讀回時會辨識為 {key}", item.product)
        }
        _ => format!(
            "{tag} 的「{}」以 ISMS 匯入讀回時會成為「{}」",
            item.product,
            readback_text(found)
        ),
    }
}

/// 一個元件的描述行與警告：目錄產品的顯示名稱讀回不一致時改寫 key 再檢查一次。
fn item_line(tag: &str, item: &InventoryItem, catalog: &Catalog) -> (String, Option<String>) {
    let line = component_line(&item.product, &item.version);
    let found = describe_resolution(&line, catalog);
    if reads_back(&found, item) {
        return (line, None);
    }
    if let Some(key) = item.key.as_deref().filter(|k| *k != item.product) {
        let by_key = component_line(key, &item.version);
        if reads_back(&describe_resolution(&by_key, catalog), item) {
            return (by_key, None);
        }
    }
    let warning = mismatch_warning(tag, &found, item);
    (line, Some(warning))
}

/// 廠牌型號行讀回時應整行略過；不是時警告。
fn model_warning(tag: &str, line: &str, catalog: &Catalog) -> Option<String> {
    let found = describe_resolution(line, catalog);
    (!found.is_empty()).then(|| {
        format!(
            "{tag} 的「{line}」以 ISMS 匯入讀回時會成為「{}」",
            readback_text(&found)
        )
    })
}

/// 整段描述讀回時，兩行以上成為同一項（同一目錄產品，或正規化後相同的未辨識產品）的警告。
/// 其中有行已有單行警告時不重複警告（`warned` 為那些行經 [`split_lines`] 後的文字）。
fn merge_warnings(
    tag: &str,
    text: &str,
    catalog: &Catalog,
    warned: &HashSet<String>,
) -> Vec<String> {
    let fresh = |lines: &[String]| !lines.iter().any(|l| warned.contains(l));
    let mut out = Vec::new();
    let mut unresolved: Vec<(String, Vec<String>)> = Vec::new();
    for item in describe_resolution(text, catalog) {
        match item {
            DescribedItem::Resolved { key, lines } => {
                if lines.len() > 1 && fresh(&lines) {
                    out.push(format!(
                        "{tag} 的「{}」以 ISMS 匯入讀回時會合併為 {key}",
                        lines.join("」「")
                    ));
                }
            }
            DescribedItem::Unresolved { text, line, .. } => {
                let norm = normalize(&text);
                match unresolved.iter_mut().find(|(n, _)| *n == norm) {
                    Some((_, lines)) => lines.push(line),
                    None => unresolved.push((norm, vec![line])),
                }
            }
        }
    }
    out.extend(
        unresolved
            .into_iter()
            .filter(|(_, lines)| lines.len() > 1 && fresh(lines))
            .map(|(_, lines)| {
                format!(
                    "{tag} 的「{}」以 ISMS 匯入讀回時會視為同一個未辨識產品，只保留第一個",
                    lines.join("」「")
                )
            }),
    );
    out
}

/// 組成資產描述並以 ISMS 匯入的規則讀回檢查（design D5、D6）。
pub fn compose_description(
    identity: DescriptionIdentity<'_>,
    items: &[InventoryItem],
    catalog: &Catalog,
) -> Composed {
    let tag = identity.asset_tag;
    let model = model_line(identity.brand, identity.model);
    let mut warnings: Vec<String> = model
        .as_deref()
        .and_then(|line| model_warning(tag, line, catalog))
        .into_iter()
        .collect();
    let mut lines: Vec<String> = model.into_iter().collect();
    let mut warned: HashSet<String> = HashSet::new();
    for item in items {
        let (line, warning) = item_line(tag, item, catalog);
        if let Some(w) = warning {
            warned.extend(split_lines(&line));
            warnings.push(w);
        }
        lines.push(line);
    }
    let text = lines.join("\n");
    warnings.extend(merge_warnings(tag, &text, catalog, &warned));
    if items.is_empty() {
        warnings.push(format!(
            "{tag} 的資產描述沒有元件行，以 ISMS 匯入建立新資產時會被略過"
        ));
    }
    Composed { text, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::export::product_text;
    use crate::assets::isms::describe::tests::catalog;
    use crate::assets::isms::describe::{clean_line, describe};

    fn product(catalog: &Catalog, key: &str, version: &str) -> InventoryItem {
        InventoryItem {
            key: Some(key.into()),
            product: product_text(catalog, key),
            version: version.into(),
        }
    }

    fn unresolved(product: &str, version: &str) -> InventoryItem {
        InventoryItem {
            key: None,
            product: product.into(),
            version: version.into(),
        }
    }

    fn identity<'a>(brand: Option<&'a str>, model: Option<&'a str>) -> DescriptionIdentity<'a> {
        DescriptionIdentity {
            asset_tag: "FW-EDGE",
            brand,
            model,
        }
    }

    #[test]
    fn description_with_model_and_components() {
        let catalog = catalog();
        let composed = compose_description(
            identity(Some("Aruba"), Some("XYZ-100")),
            &[product(&catalog, "hpe-aoscx", "1.2.3")],
            &catalog,
        );
        assert_eq!(
            composed.text,
            "廠牌型號：【Aruba XYZ-100】\nAOS-CX（1.2.3）"
        );
        assert!(composed.warnings.is_empty(), "{:?}", composed.warnings);
    }

    #[test]
    fn model_line_alias_is_not_read_back() {
        // 測試目錄把 `Aruba XYZ-100` 宣告為 hpe-aoscx 的別名：廠牌型號行不能被推測成產品
        let catalog = catalog();
        let composed = compose_description(
            identity(Some("Aruba"), Some("XYZ-100")),
            &[product(&catalog, "hpe-aoscx", "1.2.3")],
            &catalog,
        );
        let found = describe(&composed.text, &catalog);
        assert_eq!(
            found,
            vec![DescribedItem::Resolved {
                key: "hpe-aoscx".into(),
                lines: vec!["AOS-CX(1.2.3)".into()],
            }],
            "只有一個 hpe-aoscx、沒有合併"
        );
    }

    #[test]
    fn pending_and_unresolved_items() {
        let catalog = catalog();
        let composed = compose_description(
            identity(None, None),
            &[
                product(&catalog, "synology-dsm", ""),
                unresolved("Zyxel ABCD", "2.0"),
            ],
            &catalog,
        );
        assert_eq!(composed.text, "DiskStation Manager\nZyxel ABCD（2.0）");
        assert!(composed.warnings.is_empty(), "{:?}", composed.warnings);
    }

    #[test]
    fn separator_in_a_version() {
        let catalog = catalog();
        let composed = compose_description(
            identity(None, None),
            &[product(&catalog, "hpe-aoscx", "1.0, build 5")],
            &catalog,
        );
        assert_eq!(composed.text, "AOS-CX（1.0 build 5）");
        assert_eq!(split_lines(&composed.text).len(), 1);
        assert!(composed.warnings.is_empty(), "{:?}", composed.warnings);
    }

    #[test]
    fn special_characters_in_brand_and_model() {
        let line = model_line(Some("A,B:C"), Some("X】Y(Z\nW；V（全形）")).unwrap();
        assert_eq!(split_lines(&line).len(), 1, "{line}");
        assert_eq!(clean_line(&line), "", "{line}");
        assert_eq!(
            model_line(None, Some("XYZ-100")).unwrap(),
            "廠牌型號：【XYZ-100】"
        );
        assert_eq!(model_line(Some(" "), None), None);
    }

    #[test]
    fn line_that_cannot_be_read_back() {
        let catalog = catalog();
        let composed = compose_description(
            identity(None, None),
            &[unresolved("Zyxel ABCD x2", "")],
            &catalog,
        );
        assert_eq!(composed.text, "Zyxel ABCD x2", "照原樣寫出");
        assert_eq!(
            composed.warnings,
            vec!["FW-EDGE 的「Zyxel ABCD x2」以 ISMS 匯入讀回時會成為「Zyxel ABCD」"]
        );
    }

    #[test]
    fn unresolved_text_that_matches_the_catalog() {
        let catalog = catalog();
        let composed = compose_description(
            identity(None, None),
            &[unresolved("Aruba XYZ-100", "")],
            &catalog,
        );
        assert_eq!(
            composed.warnings,
            vec!["FW-EDGE 的未辨識產品「Aruba XYZ-100」讀回時會辨識為 hpe-aoscx"]
        );
    }

    #[test]
    fn display_name_that_does_not_read_back_uses_the_key() {
        let catalog = catalog();
        let item = InventoryItem {
            key: Some("hpe-aoscx".into()),
            product: "AOS-CX (switch)、CX".into(),
            version: "1.2.3".into(),
        };
        let composed = compose_description(identity(None, None), &[item], &catalog);
        assert_eq!(composed.text, "hpe-aoscx（1.2.3）");
        assert!(composed.warnings.is_empty(), "{:?}", composed.warnings);
    }

    #[test]
    fn merged_lines_are_reported_once() {
        let catalog = catalog();
        let composed = compose_description(
            identity(None, None),
            &[
                product(&catalog, "hpe-aoscx", "1.2.3"),
                unresolved("Aruba XYZ-100", ""),
            ],
            &catalog,
        );
        assert_eq!(
            composed.warnings,
            vec!["FW-EDGE 的未辨識產品「Aruba XYZ-100」讀回時會辨識為 hpe-aoscx"],
            "同一個問題只警告一次"
        );
        let composed = compose_description(
            identity(None, None),
            &[
                product(&catalog, "hpe-aoscx", "1.2.3"),
                product(&catalog, "hpe-aoscx", "1.2.4"),
            ],
            &catalog,
        );
        assert_eq!(
            composed.warnings,
            vec![
                "FW-EDGE 的「AOS-CX(1.2.3)」「AOS-CX(1.2.4)」以 ISMS 匯入讀回時會合併為 hpe-aoscx"
            ]
        );
    }

    #[test]
    fn same_unresolved_text_is_reported() {
        let catalog = catalog();
        let composed = compose_description(
            identity(None, None),
            &[
                unresolved("Zyxel ABCD", "1.0"),
                unresolved("zyxel abcd", "2.0"),
            ],
            &catalog,
        );
        assert_eq!(
            composed.warnings,
            vec!["FW-EDGE 的「Zyxel ABCD(1.0)」「zyxel abcd(2.0)」以 ISMS 匯入讀回時會視為同一個未辨識產品，只保留第一個"]
        );
    }

    #[test]
    fn asset_with_only_a_model() {
        let catalog = catalog();
        let composed = compose_description(identity(Some("Aruba"), Some("XYZ-100")), &[], &catalog);
        assert_eq!(composed.text, "廠牌型號：【Aruba XYZ-100】");
        assert_eq!(
            composed.warnings,
            vec!["FW-EDGE 的資產描述沒有元件行，以 ISMS 匯入建立新資產時會被略過"]
        );
    }
}
