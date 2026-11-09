//! 硬體 EoL（22 design D5）：資產的手動覆寫優先，其次以廠牌＋型號對應目錄的硬體型號清單。
//! 廠牌與型號都有值才比對（不只用型號比對，避免不同廠牌同名型號誤對）。

use super::model::{ItemState, MissingReason, Source};
use crate::assets::AssetRecord;
use crate::catalog::HardwareCatalog;

fn filled(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// 硬體 EoL 的判定結果與對應到的目錄型號（「Aruba XYZ-100」）。
pub fn hardware_state(
    record: &AssetRecord,
    hardware: &HardwareCatalog,
) -> (ItemState, Option<String>) {
    if let Some(o) = &record.hw_eol_override {
        return (
            ItemState::Dated {
                date: o.date,
                source: Source::Override(o.source.clone()),
            },
            None,
        );
    }
    let (Some(brand), Some(model)) = (filled(&record.brand), filled(&record.model)) else {
        return (ItemState::Missing(MissingReason::NoBrandOrModel), None);
    };
    match hardware.lookup(brand, model) {
        Some(m) => (
            ItemState::Dated {
                date: m.eol,
                source: Source::Catalog(m.source.clone()),
            },
            Some(m.label()),
        ),
        None => (ItemState::Missing(MissingReason::ModelNotInCatalog), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, CatalogSource, SEED_SOURCE_NAME, SEED_TOML};
    use chrono::NaiveDate;

    const LOCAL: &str = r#"
[[hardware]]
vendor = "Aruba"
model = "XYZ-100"
aliases = ["XYZ-100F"]
eol = "2026-10-31"
source = "https://vendor.example/eol/xyz-100"
"#;

    fn catalog() -> Catalog {
        Catalog::from_sources(&[
            CatalogSource {
                name: SEED_SOURCE_NAME.into(),
                text: SEED_TOML.into(),
            },
            CatalogSource {
                name: "hardware.toml".into(),
                text: LOCAL.into(),
            },
        ])
        .unwrap()
    }

    fn record(brand: Option<&str>, model: Option<&str>) -> AssetRecord {
        let input = crate::assets::validate::RecordInput {
            asset_tag: "SW-CORE",
            name: "sw-core",
            category: "HW02",
            ..Default::default()
        };
        let mut r = crate::assets::validate::validate_import_record(
            &input,
            &crate::assets::AssetClasses::builtin(),
        )
        .unwrap()
        .record;
        r.brand = brand.map(String::from);
        r.model = model.map(String::from);
        r
    }

    #[test]
    fn model_listed_in_the_catalog() {
        let (state, matched) =
            hardware_state(&record(Some("aruba"), Some("xyz100")), catalog().hardware());
        assert_eq!(
            state,
            ItemState::Dated {
                date: NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(),
                source: Source::Catalog("https://vendor.example/eol/xyz-100".into()),
            }
        );
        assert_eq!(matched.as_deref(), Some("Aruba XYZ-100"));
    }

    #[test]
    fn alias_of_a_model() {
        let (state, matched) = hardware_state(
            &record(Some("Aruba"), Some("XYZ-100F")),
            catalog().hardware(),
        );
        assert!(matches!(state, ItemState::Dated { .. }));
        assert_eq!(matched.as_deref(), Some("Aruba XYZ-100"));
    }

    #[test]
    fn model_not_in_the_catalog() {
        let (state, _) = hardware_state(
            &record(Some("Palo Alto"), Some("XYZ-999")),
            catalog().hardware(),
        );
        assert_eq!(state, ItemState::Missing(MissingReason::ModelNotInCatalog));
    }

    #[test]
    fn model_without_a_brand() {
        let (state, _) = hardware_state(&record(None, Some("XYZ-100")), catalog().hardware());
        assert_eq!(state, ItemState::Missing(MissingReason::NoBrandOrModel));
        let (state, _) = hardware_state(&record(Some(" "), Some("XYZ-100")), catalog().hardware());
        assert_eq!(state, ItemState::Missing(MissingReason::NoBrandOrModel));
    }
}
