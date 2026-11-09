//! 產品選擇器的搜尋、排序與筆數上限（GUI「加入元件」與未辨識產品的「改為…」共用）。
//!
//! 搜尋字與候選字串（顯示名稱、別名、廠商、key）都先經 [`normalize`]（不分大小寫、忽略空白與 `-_./`），
//! 顯示名稱或別名以搜尋字開頭者排在只有中間相符者之前，同級依顯示名稱排序。

use cvewatch_core::catalog::{normalize, Catalog, Product};

/// 選擇器一次顯示的搜尋結果上限。
pub const PICKER_LIMIT: usize = 12;

/// 目錄產品選單的一個選項。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductChoice {
    pub key: String,
    pub display_name: String,
    pub vendor_display: String,
}

impl ProductChoice {
    fn from_product(p: &Product) -> ProductChoice {
        ProductChoice {
            key: p.key.clone(),
            display_name: p.display_name.clone(),
            vendor_display: p.vendor_display.clone(),
        }
    }

    /// 選單顯示文字。
    pub fn label(&self) -> String {
        format!("{}（{}）", self.display_name, self.vendor_display)
    }
}

/// 相符程度：開頭相符優先於中間相符。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Prefix,
    Contains,
}

fn tier(p: &Product, q: &str) -> Option<Tier> {
    if q.is_empty() {
        return Some(Tier::Prefix);
    }
    // aliases 已含 key 與 display_name
    let names: Vec<String> = p.aliases.iter().map(|a| normalize(a)).collect();
    if names.iter().any(|n| n.starts_with(q)) {
        return Some(Tier::Prefix);
    }
    let vendor = normalize(&p.vendor_display);
    (names.iter().any(|n| n.contains(q)) || vendor.contains(q)).then_some(Tier::Contains)
}

/// 依搜尋字列出相符產品（排序後的完整結果）；空字串依顯示名稱列出全部。
pub fn search_products(catalog: &Catalog, query: &str) -> Vec<ProductChoice> {
    let q = normalize(query);
    let mut hits: Vec<(Tier, String, &Product)> = catalog
        .products()
        .iter()
        .filter_map(|p| tier(p, &q).map(|t| (t, p.display_name.to_lowercase(), p)))
        .collect();
    hits.sort_by(|a, b| (a.0, &a.1, &a.2.key).cmp(&(b.0, &b.1, &b.2.key)));
    hits.into_iter()
        .map(|(_, _, p)| ProductChoice::from_product(p))
        .collect()
}

/// 截斷後的結果：前 `limit` 筆與其餘筆數。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limited {
    pub shown: Vec<ProductChoice>,
    pub remaining: usize,
}

impl Limited {
    /// 清單末端的提示；沒有結果時為「沒有相符的產品」，沒有截斷時為 None。
    pub fn footer(&self) -> Option<String> {
        if self.shown.is_empty() && self.remaining == 0 {
            Some(NO_MATCH.to_string())
        } else if self.remaining > 0 {
            Some(format!("還有 {} 項，再輸入文字縮小範圍", self.remaining))
        } else {
            None
        }
    }
}

/// 沒有相符產品時的提示。
pub const NO_MATCH: &str = "沒有相符的產品";

/// 只保留前 `limit` 筆。
pub fn limit(results: Vec<ProductChoice>, limit: usize) -> Limited {
    let remaining = results.len().saturating_sub(limit);
    let shown = results.into_iter().take(limit).collect();
    Limited { shown, remaining }
}

/// 選擇器的候選清單。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerList {
    /// 未辨識模式的相似建議（與匯入報表相同，最多 3 個）；加入元件模式為空
    pub suggestions: Vec<ProductChoice>,
    /// 搜尋結果（已去除建議中的產品）
    pub results: Limited,
    /// 搜尋相符、但已列在建議中而未重複列出的筆數
    pub in_suggestions: usize,
}

impl PickerList {
    /// 清單末端的提示：搜尋相符的產品都已列在建議中時不提示「沒有相符的產品」。
    pub fn footer(&self) -> Option<String> {
        if self.in_suggestions > 0 && self.results.shown.is_empty() {
            return None;
        }
        self.results.footer()
    }
}

/// 加入元件模式：只有搜尋結果。
pub fn picker_list(catalog: &Catalog, query: &str) -> PickerList {
    PickerList {
        suggestions: Vec::new(),
        results: limit(search_products(catalog, query), PICKER_LIMIT),
        in_suggestions: 0,
    }
}

/// 未辨識模式：建議在前，搜尋結果去除已列在建議中的產品。
pub fn unresolved_picker_list(catalog: &Catalog, product_raw: &str, query: &str) -> PickerList {
    let suggestions: Vec<ProductChoice> = catalog
        .suggest(product_raw)
        .into_iter()
        .filter_map(|key| catalog.get(key))
        .map(ProductChoice::from_product)
        .collect();
    let (duplicates, results): (Vec<ProductChoice>, Vec<ProductChoice>) =
        search_products(catalog, query)
            .into_iter()
            .partition(|c| suggestions.iter().any(|s| s.key == c.key));
    PickerList {
        suggestions,
        results: limit(results, PICKER_LIMIT),
        in_suggestions: duplicates.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cvewatch_core::catalog::CatalogSource;

    fn keys(catalog: &Catalog, q: &str) -> Vec<String> {
        search_products(catalog, q)
            .into_iter()
            .map(|p| p.key)
            .collect()
    }

    /// `n` 項產品的虛構目錄。
    fn synthetic(n: usize) -> Catalog {
        let text: String = (0..n)
            .map(|i| {
                format!(
                    "[[product]]\nkey = \"p{i:02}\"\ndisplay_name = \"Product {i:02}\"\n\
                     vendor_display = \"Vendor\"\nversion_scheme = \"dotted\"\n\n"
                )
            })
            .collect();
        Catalog::from_sources(&[CatalogSource {
            name: "synthetic.toml".into(),
            text,
        }])
        .unwrap()
    }

    #[test]
    fn matches_display_name_aliases_and_vendor_ignoring_separators() {
        let catalog = Catalog::seed().unwrap();
        assert!(keys(&catalog, "pan").contains(&"paloalto-panos".to_string()));
        assert!(
            keys(&catalog, "pve").contains(&"proxmox-ve".to_string()),
            "別名"
        );
        assert!(keys(&catalog, "aoscx").contains(&"hpe-aoscx".to_string()));
        assert!(keys(&catalog, "ckeditor 4").contains(&"ckeditor4".to_string()));
        assert!(
            keys(&catalog, "aruba").contains(&"hpe-aoscx".to_string()),
            "廠商"
        );
        assert!(keys(&catalog, "no-such-thing").is_empty());
    }

    #[test]
    fn prefix_matches_come_first() {
        let catalog = Catalog::seed().unwrap();
        let ruby = keys(&catalog, "ruby");
        assert_eq!(&ruby[..2], ["ruby", "rails"]);

        let text = r#"
[[product]]
key = "alpha"
display_name = "Alpha Ruby Tool"
vendor_display = "A"
version_scheme = "dotted"

[[product]]
key = "zed"
display_name = "Zed"
vendor_display = "Z"
version_scheme = "dotted"
aliases = ["ruby-z"]

[[product]]
key = "vendor-only"
display_name = "Beta"
vendor_display = "Ruby Labs"
version_scheme = "dotted"
"#;
        let catalog = Catalog::from_sources(&[CatalogSource {
            name: "t.toml".into(),
            text: text.into(),
        }])
        .unwrap();
        // 別名開頭相符（Zed）排在只有中間或廠商相符者之前；同級依顯示名稱
        assert_eq!(keys(&catalog, "ruby"), ["zed", "alpha", "vendor-only"]);
    }

    #[test]
    fn empty_query_lists_all_by_display_name() {
        let catalog = Catalog::seed().unwrap();
        let all = search_products(&catalog, "");
        assert_eq!(all.len(), catalog.products().len());
        let names: Vec<String> = all.iter().map(|p| p.display_name.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn limit_reports_remaining() {
        let list = picker_list(&synthetic(30), "");
        assert_eq!(list.results.shown.len(), PICKER_LIMIT);
        assert_eq!(list.results.remaining, 18);
        assert_eq!(
            list.results.footer().as_deref(),
            Some("還有 18 項，再輸入文字縮小範圍")
        );
        let seed = Catalog::seed().unwrap();
        assert_eq!(seed.products().len(), 78);
        assert_eq!(picker_list(&seed, "").results.remaining, 66);
        let few = picker_list(&synthetic(5), "");
        assert_eq!((few.results.shown.len(), few.results.footer()), (5, None));
    }

    #[test]
    fn no_match_footer() {
        let list = picker_list(&Catalog::seed().unwrap(), "zzz");
        assert!(list.results.shown.is_empty());
        assert_eq!(list.results.footer().as_deref(), Some(NO_MATCH));
    }

    #[test]
    fn unresolved_suggestions_come_first_without_duplicates() {
        let catalog = Catalog::seed().unwrap();
        let list = unresolved_picker_list(&catalog, "Synolgy DSM", "");
        assert_eq!(list.suggestions[0].key, "synology-dsm");
        assert!(list
            .results
            .shown
            .iter()
            .all(|c| list.suggestions.iter().all(|s| s.key != c.key)));
        assert_eq!(
            list.results.shown.len() + list.results.remaining + list.suggestions.len(),
            catalog.products().len()
        );
        // 唯一的搜尋結果已列在建議中：不提示「沒有相符的產品」
        let only_suggested = unresolved_picker_list(&catalog, "Synolgy DSM", "diskstation");
        assert!(only_suggested.results.shown.is_empty());
        assert_eq!(only_suggested.footer(), None);
        let nothing = unresolved_picker_list(&catalog, "Synolgy DSM", "zzz");
        assert_eq!(nothing.footer().as_deref(), Some(NO_MATCH));
        let none = unresolved_picker_list(&catalog, "核心交換器", "cx");
        assert!(none.suggestions.is_empty());
        assert!(none.results.shown.iter().any(|c| c.key == "hpe-aoscx"));
    }
}
