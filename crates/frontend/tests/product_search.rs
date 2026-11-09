//! 14 任務 4.3：產品選擇器的搜尋、排序與上限（gui-console「GUI product picker」相關 scenario）。

use cvewatch_core::catalog::{Catalog, CatalogSource};
use cvewatch_frontend::product_search::{
    picker_list, search_products, unresolved_picker_list, NO_MATCH, PICKER_LIMIT,
};

fn seed() -> Catalog {
    Catalog::seed().expect("seed")
}

fn labels(catalog: &Catalog, q: &str) -> Vec<String> {
    picker_list(catalog, q)
        .results
        .shown
        .iter()
        .map(|c| c.label())
        .collect()
}

#[test]
fn searching_by_vendor() {
    let found = labels(&seed(), "aruba");
    assert!(
        found.contains(&"AOS-CX（HPE Aruba Networking）".to_string()),
        "{found:?}"
    );
}

#[test]
fn search_ignores_separators() {
    let catalog = seed();
    for q in ["aoscx", "AOS CX", "aos_cx", "Aos.Cx"] {
        let keys: Vec<String> = search_products(&catalog, q)
            .into_iter()
            .map(|c| c.key)
            .collect();
        assert!(keys.contains(&"hpe-aoscx".to_string()), "{q}：{keys:?}");
    }
}

#[test]
fn prefix_matches_first() {
    let names: Vec<String> = search_products(&seed(), "ruby")
        .into_iter()
        .map(|c| c.display_name)
        .collect();
    assert_eq!(&names[..2], ["Ruby", "Ruby on Rails"], "{names:?}");
}

#[test]
fn too_many_results() {
    let text: String = (0..30)
        .map(|i| {
            format!(
                "[[product]]\nkey = \"p{i:02}\"\ndisplay_name = \"Product {i:02}\"\n\
                 vendor_display = \"Vendor\"\nversion_scheme = \"dotted\"\n\n"
            )
        })
        .collect();
    let catalog = Catalog::from_sources(&[CatalogSource {
        name: "thirty.toml".into(),
        text,
    }])
    .unwrap();
    let list = picker_list(&catalog, "");
    assert_eq!(list.results.shown.len(), PICKER_LIMIT);
    assert_eq!(
        list.results.footer().as_deref(),
        Some("還有 18 項，再輸入文字縮小範圍")
    );
    // 內建目錄 78 項
    assert_eq!(picker_list(&seed(), "").results.remaining, 66);
}

#[test]
fn no_match() {
    let list = picker_list(&seed(), "zzz");
    assert!(list.results.shown.is_empty());
    assert_eq!(list.results.footer().as_deref(), Some(NO_MATCH));
}

#[test]
fn unresolved_mode_lists_suggestions_once() {
    let catalog = seed();
    let typo = unresolved_picker_list(&catalog, "Synolgy DSM", "");
    assert_eq!(typo.suggestions[0].display_name, "DiskStation Manager");
    assert!(typo.results.shown.iter().all(|c| c.key != "synology-dsm"));
    let beyond = unresolved_picker_list(&catalog, "核心交換器", "cx");
    assert!(beyond.suggestions.is_empty());
    assert!(beyond.results.shown.iter().any(|c| c.key == "hpe-aoscx"));
}
