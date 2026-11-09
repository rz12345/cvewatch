//! 生命週期的 CLI 輸出（22 design D4、D13）：純函式，便於單元測試。

use cvewatch_core::assets::dates::format_date;
use cvewatch_core::catalog::Product;
use cvewatch_core::lifecycle::AssetLifecycle;

/// `asset show` 的生命週期段落（22 design D13）：硬體、各清冊元件、保固、合約、授權各一行。
pub fn lifecycle_lines(l: &AssetLifecycle, window: u32) -> Vec<String> {
    let mut out = vec![format!("生命週期（提醒 {window} 天）：")];
    out.extend(l.items.iter().map(|i| format!("  {}", i.line())));
    out
}

/// `asset catalog show` 的 endoflife.date 代稱與目錄宣告的版本分支 EoL。
pub fn catalog_lifecycle_lines(p: &Product) -> Vec<String> {
    let mut out = vec![format!(
        "  endoflife.date：{}",
        p.endoflife.as_deref().unwrap_or("無")
    )];
    if p.eol_cycles.is_empty() {
        out.push("  目錄宣告的版本分支 EoL：（無）".to_string());
        return out;
    }
    out.push("  目錄宣告的版本分支 EoL：".to_string());
    out.extend(
        p.eol_cycles
            .iter()
            .map(|c| format!("    {:<10} {}  {}", c.cycle, format_date(c.eol), c.source)),
    );
    out
}

#[cfg(test)]
mod tests {
    use cvewatch_core::catalog::{Catalog, CatalogSource, SEED_SOURCE_NAME, SEED_TOML};

    use super::*;

    mod show {
        use std::collections::HashMap;

        use chrono::NaiveDate;
        use cvewatch_core::assets::validate::{validate_import_record, RecordInput};
        use cvewatch_core::assets::{
            Asset, AssetClasses, AssetStatus, Component, ComponentKind, ComponentSource,
        };
        use cvewatch_core::catalog::Catalog;
        use cvewatch_core::lifecycle::eol_data::{EolProduct, EolRelease};
        use cvewatch_core::lifecycle::{evaluate, EolData};

        use super::super::lifecycle_lines;

        fn asset(tag: &str, brand: Option<&str>) -> Asset {
            let mut record = validate_import_record(
                &RecordInput {
                    asset_tag: tag,
                    name: tag,
                    category: "HW03",
                    ..RecordInput::default()
                },
                &AssetClasses::builtin(),
            )
            .unwrap()
            .record;
            record.brand = brand.map(String::from);
            record.model = brand.map(|_| "XYZ-999".to_string());
            Asset {
                id: 1,
                record,
                status: AssetStatus::Active,
                first_seen: String::new(),
                last_seen: String::new(),
                retired_at: None,
            }
        }

        fn component(id: i64, key: &str, version: &str) -> Component {
            Component {
                id,
                asset_id: 1,
                kind: ComponentKind::Product {
                    product_id: id,
                    product_key: key.into(),
                },
                version_raw: version.into(),
                version_norm: None,
                source: ComponentSource::Excel,
                updated_at: String::new(),
                eol_override: None,
            }
        }

        fn eol() -> EolData {
            EolData {
                products: HashMap::from([(
                    "panos".to_string(),
                    EolProduct {
                        not_found: false,
                        html_link: None,
                        releases: vec![EolRelease {
                            name: "10.2".into(),
                            is_eol: false,
                            eol_from: NaiveDate::from_ymd_opt(2027, 3, 1),
                            latest_name: None,
                        }],
                    },
                )]),
            }
        }

        /// Scenario: Showing lifecycle items（並涵蓋未填廠牌或型號、無 EoL 資料、版本待補）
        #[test]
        fn showing_lifecycle_items() {
            let today = NaiveDate::from_ymd_opt(2027, 1, 10).unwrap();
            let l = evaluate(
                &asset("FW-EDGE", None),
                &[
                    component(1, "paloalto-panos", "10.2.9"),
                    component(2, "dell-idrac9", "7.10.30.00"),
                    component(3, "synology-dsm", ""),
                ],
                &Catalog::seed().unwrap(),
                &eol(),
                today,
                90,
            );
            let lines = lifecycle_lines(&l, 90);
            assert_eq!(
                lines,
                [
                    "生命週期（提醒 90 天）：",
                    "  硬體 EoL 未填廠牌或型號",
                    "  PAN-OS 10.2.9（分支 10.2）EoL 2027-03-01 即將 EoL（剩 50 天），來源 endoflife.date",
                    "  iDRAC9 7.10.30.00 無 EoL 資料",
                    "  DiskStation Manager 版本待補，無法判斷 EoL",
                    "  保固到期 未填",
                    "  維護合約到期 未填",
                    "  軟體授權到期 未填",
                ]
            );
            let l = evaluate(
                &asset("FW-EDGE", Some("Palo Alto Networks")),
                &[],
                &Catalog::seed().unwrap(),
                &eol(),
                today,
                90,
            );
            assert_eq!(lifecycle_lines(&l, 90)[1], "  硬體 EoL 目錄沒有此型號");
        }
    }

    #[test]
    fn product_with_slug() {
        let catalog = Catalog::seed().unwrap();
        let lines = catalog_lifecycle_lines(catalog.get("paloalto-panos").unwrap());
        assert_eq!(lines[0], "  endoflife.date：panos");
        assert_eq!(lines[1], "  目錄宣告的版本分支 EoL：（無）");
    }

    #[test]
    fn product_without_slug() {
        let catalog = Catalog::seed().unwrap();
        let lines = catalog_lifecycle_lines(catalog.get("dell-idrac9").unwrap());
        assert_eq!(lines[0], "  endoflife.date：無");
    }

    #[test]
    fn product_with_cycle_table() {
        let local = r#"
[[product]]
key = "synology-dsm"
eol_cycles = [{ cycle = "7.9", eol = "2040-01-31", source = "https://vendor.example/dsm" }]
"#;
        let catalog = Catalog::from_sources(&[
            CatalogSource {
                name: SEED_SOURCE_NAME.into(),
                text: SEED_TOML.into(),
            },
            CatalogSource {
                name: "local.toml".into(),
                text: local.into(),
            },
        ])
        .unwrap();
        let lines = catalog_lifecycle_lines(catalog.get("synology-dsm").unwrap());
        assert_eq!(lines[0], "  endoflife.date：無");
        assert_eq!(lines[1], "  目錄宣告的版本分支 EoL：");
        assert!(lines.iter().any(|l| l.contains("7.2")
            && l.contains("2025-12-01")
            && l.contains("https://kb.synology.com/")));
        assert_eq!(
            lines.last().unwrap(),
            "    7.9        2040-01-31  https://vendor.example/dsm"
        );
    }
}
