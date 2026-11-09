//! 22 任務 1.1、1.2、6.1、6.3：目錄的 endoflife.date 代稱、版本分支 EoL 與硬體型號清單。

use chrono::NaiveDate;
use cvewatch_core::catalog::{Catalog, CatalogSource, SEED_SOURCE_NAME, SEED_TOML};
use cvewatch_core::CoreError;

fn with_user(files: &[(&str, &str)]) -> Result<Catalog, CoreError> {
    let mut sources = vec![CatalogSource {
        name: SEED_SOURCE_NAME.into(),
        text: SEED_TOML.into(),
    }];
    sources.extend(files.iter().map(|(n, t)| CatalogSource {
        name: (*n).into(),
        text: (*t).into(),
    }));
    Catalog::from_sources(&sources)
}

fn err_text(result: Result<Catalog, CoreError>) -> String {
    result.expect_err("應拒絕載入").to_string()
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

const DSM_CYCLE_75: &str = r#"
[[product]]
key = "synology-dsm"
eol_cycles = [
  { cycle = "7.5", eol = "2031-12-31", source = "https://vendor.example/dsm-7.5" },
]
"#;

#[test]
fn declaring_a_slug_locally() {
    let toml = "[[product]]\nkey = \"nginx\"\nendoflife = \"nginx\"\n";
    let catalog = with_user(&[("local.toml", toml)]).unwrap();
    assert_eq!(
        catalog.get("nginx").unwrap().endoflife.as_deref(),
        Some("nginx")
    );
}

#[test]
fn disabling_a_seed_slug() {
    let seed = Catalog::seed().unwrap();
    assert!(seed.get("proxmox-ve").unwrap().endoflife.is_some());
    let toml = "[[product]]\nkey = \"proxmox-ve\"\nendoflife = \"\"\n";
    let catalog = with_user(&[("local.toml", toml)]).unwrap();
    assert_eq!(catalog.get("proxmox-ve").unwrap().endoflife, None);
}

#[test]
fn malformed_slug_is_rejected_with_product_key() {
    let toml = "[[product]]\nkey = \"proxmox-ve\"\nendoflife = \"Proxmox VE\"\n";
    let text = err_text(with_user(&[("local.toml", toml)]));
    assert!(text.contains("local.toml"), "{text}");
    assert!(text.contains("proxmox-ve"), "{text}");
    assert!(text.contains("代稱"), "{text}");
}

#[test]
fn adding_a_cycle_locally() {
    let seed = Catalog::seed().unwrap();
    let seed_cycles = seed.get("synology-dsm").unwrap().eol_cycles.len();
    let catalog = with_user(&[("local.toml", DSM_CYCLE_75)]).unwrap();
    let cycles = &catalog.get("synology-dsm").unwrap().eol_cycles;
    assert_eq!(cycles.len(), seed_cycles + 1);
    let added = cycles.iter().find(|c| c.cycle == "7.5").unwrap();
    assert_eq!(added.eol, ymd(2031, 12, 31));
    assert_eq!(added.source, "https://vendor.example/dsm-7.5");
}

#[test]
fn correcting_a_seed_date() {
    let seed = Catalog::seed().unwrap();
    let first = seed.get("synology-dsm").unwrap().eol_cycles[0].clone();
    let toml = format!(
        "[[product]]\nkey = \"synology-dsm\"\neol_cycles = [{{ cycle = \"{}\", eol = \"2035-01-31\", source = \"https://vendor.example/fix\" }}]\n",
        first.cycle
    );
    let catalog = with_user(&[("local.toml", &toml)]).unwrap();
    let cycles = &catalog.get("synology-dsm").unwrap().eol_cycles;
    assert_eq!(
        cycles.len(),
        seed.get("synology-dsm").unwrap().eol_cycles.len()
    );
    let fixed = cycles.iter().find(|c| c.cycle == first.cycle).unwrap();
    assert_eq!(fixed.eol, ymd(2035, 1, 31));
    assert_eq!(fixed.source, "https://vendor.example/fix");
}

#[test]
fn missing_source_url_names_product_and_cycle() {
    let toml = "[[product]]\nkey = \"synology-dsm\"\neol_cycles = [{ cycle = \"7.3\", eol = \"2031-12-31\" }]\n";
    let text = err_text(with_user(&[("local.toml", toml)]));
    assert!(text.contains("synology-dsm"), "{text}");
    assert!(text.contains("7.3"), "{text}");
    assert!(text.contains("來源網址"), "{text}");
}

#[test]
fn cycle_date_and_url_are_validated() {
    let bad_date = "[[product]]\nkey = \"synology-dsm\"\neol_cycles = [{ cycle = \"7.3\", eol = \"2031/12/31\", source = \"https://x.example\" }]\n";
    assert!(err_text(with_user(&[("a.toml", bad_date)])).contains("YYYY-MM-DD"));
    let bad_url = "[[product]]\nkey = \"synology-dsm\"\neol_cycles = [{ cycle = \"7.3\", eol = \"2031-12-31\", source = \"vendor.example\" }]\n";
    assert!(err_text(with_user(&[("a.toml", bad_url)])).contains("http"));
    let duplicate = "[[product]]\nkey = \"synology-dsm\"\neol_cycles = [\n { cycle = \"7.3\", eol = \"2031-12-31\", source = \"https://x.example\" },\n { cycle = \"7.3\", eol = \"2032-12-31\", source = \"https://x.example\" },\n]\n";
    assert!(err_text(with_user(&[("a.toml", duplicate)])).contains("重複"));
}

#[test]
fn fingerprint_ignores_lifecycle_fields() {
    let seed = Catalog::seed().unwrap();
    let toml =
        format!("{DSM_CYCLE_75}\n[[product]]\nkey = \"nginx\"\nendoflife = \"\"\n\n{HARDWARE}");
    let catalog = with_user(&[("local.toml", &toml)]).unwrap();
    assert_eq!(seed.fingerprint(), catalog.fingerprint());
}

const HARDWARE: &str = r#"
[[hardware]]
vendor = "Aruba"
vendor_aliases = ["HPE Aruba Networking"]
model = "XYZ-100"
aliases = ["XYZ-100F"]
eol = "2026-10-31"
source = "https://vendor.example/eol/xyz-100"

[[hardware]]
vendor = "Aruba"
model = "XYZ-200"
eol = "2028-03-31"
source = "https://vendor.example/eol/xyz-200"
"#;

#[test]
fn declaring_a_model_locally() {
    let catalog = with_user(&[("hardware.toml", HARDWARE)]).unwrap();
    let hw = catalog.hardware();
    assert_eq!(hw.models().len(), 2);
    let found = hw.lookup("aruba", "xyz100").unwrap();
    assert_eq!(found.label(), "Aruba XYZ-100");
    assert_eq!(found.eol, ymd(2026, 10, 31));
    assert_eq!(
        hw.lookup("ＨＰＥ Aruba Networking", "XYZ-100F")
            .unwrap()
            .model,
        "XYZ-100"
    );
    assert!(hw.lookup("Cisco", "XYZ-100").is_none());
    assert!(hw.lookup("", "XYZ-100").is_none());
}

#[test]
fn later_model_declaration_wins() {
    let fix = "[[hardware]]\nvendor = \"ARUBA\"\nmodel = \"xyz 100\"\neol = \"2027-01-31\"\nsource = \"https://vendor.example/fix\"\n";
    let catalog = with_user(&[("a.toml", HARDWARE), ("b.toml", fix)]).unwrap();
    let hw = catalog.hardware();
    assert_eq!(hw.models().len(), 2);
    assert_eq!(hw.lookup("Aruba", "XYZ-100").unwrap().eol, ymd(2027, 1, 31));
    // 後者沒有別名，覆寫後舊別名不再有效
    assert!(hw.lookup("Aruba", "XYZ-100F").is_none());
}

#[test]
fn conflicting_model_alias_names_both_models() {
    let toml = r#"
[[hardware]]
vendor = "Aruba"
model = "XYZ-100"
aliases = ["XYZ-1X0"]
eol = "2026-10-31"
source = "https://vendor.example/a"

[[hardware]]
vendor = "Aruba"
model = "XYZ-200"
aliases = ["XYZ-1X0"]
eol = "2028-03-31"
source = "https://vendor.example/b"
"#;
    let text = err_text(with_user(&[("hardware.toml", toml)]));
    assert!(text.contains("XYZ-1X0"), "{text}");
    assert!(
        text.contains("Aruba XYZ-100") && text.contains("Aruba XYZ-200"),
        "{text}"
    );
}

#[test]
fn model_requires_date_and_source() {
    let toml = "[[hardware]]\nvendor = \"Aruba\"\nmodel = \"XYZ-100\"\neol = \"2026-10-31\"\n";
    let text = err_text(with_user(&[("hardware.toml", toml)]));
    assert!(
        text.contains("Aruba XYZ-100") && text.contains("來源網址"),
        "{text}"
    );
}

#[test]
fn seed_has_no_models() {
    assert!(Catalog::seed().unwrap().hardware().is_empty());
    assert!(!SEED_TOML
        .lines()
        .any(|l| l.trim_start().starts_with("[[hardware]]")));
}

#[test]
fn seed_slugs_are_well_formed_and_panos_is_declared() {
    let seed = Catalog::seed().unwrap();
    assert_eq!(
        seed.get("paloalto-panos").unwrap().endoflife.as_deref(),
        Some("panos")
    );
    for p in seed.products() {
        if let Some(slug) = &p.endoflife {
            assert!(
                cvewatch_core::catalog::lifecycle::is_valid_slug(slug),
                "{}: {slug}",
                p.key
            );
        }
    }
}

#[test]
fn seed_cycles_have_https_sources_and_dsm_has_no_slug() {
    let seed = Catalog::seed().unwrap();
    let dsm = seed.get("synology-dsm").unwrap();
    assert!(!dsm.eol_cycles.is_empty());
    assert_eq!(dsm.endoflife, None);
    for p in seed.products() {
        for c in &p.eol_cycles {
            assert!(c.source.starts_with("https://"), "{} {}", p.key, c.cycle);
        }
    }
    assert!(seed.get("dell-idrac9").unwrap().eol_cycles.is_empty());
}

/// 任務 6.3：使用說明的種子 EoL 涵蓋清單與 seed.toml 一致。
#[test]
fn user_guide_lists_the_seed_coverage() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/user-guide/lifecycle.md");
    let doc = std::fs::read_to_string(path).unwrap();
    let seed = Catalog::seed().unwrap();
    let mut slugs = 0;
    for p in seed.products() {
        if let Some(slug) = &p.endoflife {
            slugs += 1;
            let row = format!("| `{}` | `{slug}` |", p.key);
            assert!(doc.contains(&row), "使用說明缺少 {row}");
        }
        for c in &p.eol_cycles {
            let row = format!("| {} | {} |", c.cycle, c.eol);
            assert!(doc.contains(&row), "使用說明缺少 {} {row}", p.key);
        }
    }
    let listed = doc.lines().filter(|l| l.starts_with("| `")).count();
    assert_eq!(listed, slugs, "使用說明的代稱表與種子數量一致");
}
