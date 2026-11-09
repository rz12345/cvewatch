//! [`super`] 的單元測試（spec asset-lifecycle 的判定與狀態情境）。

use std::collections::HashMap;

use chrono::NaiveDate;

use super::*;
use crate::assets::{AssetRecord, AssetStatus, ComponentSource, EolOverride};
use crate::catalog::{CatalogSource, SEED_SOURCE_NAME, SEED_TOML};
use crate::lifecycle::eol_data::{EolProduct, EolRelease};

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

const TODAY: (i32, u32, u32) = (2027, 1, 10);

fn today() -> NaiveDate {
    ymd(TODAY.0, TODAY.1, TODAY.2)
}

fn catalog(extra: &str) -> Catalog {
    Catalog::from_sources(&[
        CatalogSource {
            name: SEED_SOURCE_NAME.into(),
            text: SEED_TOML.into(),
        },
        CatalogSource {
            name: "local.toml".into(),
            text: extra.into(),
        },
    ])
    .unwrap()
}

fn asset(tag: &str) -> Asset {
    let input = crate::assets::validate::RecordInput {
        asset_tag: tag,
        name: tag,
        category: "HW01",
        ..Default::default()
    };
    let record: AssetRecord = crate::assets::validate::validate_import_record(
        &input,
        &crate::assets::AssetClasses::builtin(),
    )
    .unwrap()
    .record;
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

fn release(name: &str, eol: Option<NaiveDate>, is_eol: bool, latest: Option<&str>) -> EolRelease {
    EolRelease {
        name: name.into(),
        is_eol,
        eol_from: eol,
        latest_name: latest.map(String::from),
    }
}

fn eol_data() -> EolData {
    let panos = EolProduct {
        not_found: false,
        html_link: Some("https://endoflife.date/panos".into()),
        releases: vec![
            release("11.2", Some(ymd(2027, 5, 2)), false, None),
            release("11.1", Some(ymd(2027, 5, 3)), false, None),
            release("10.2", Some(ymd(2027, 3, 1)), false, None),
        ],
    };
    let esxi = EolProduct {
        not_found: false,
        html_link: None,
        releases: vec![release("8.0", Some(ymd(2027, 10, 11)), false, None)],
    };
    let pve = EolProduct {
        not_found: false,
        html_link: None,
        releases: vec![
            release("9", None, false, Some("9.2")),
            release("8", None, true, Some("8.4")),
        ],
    };
    EolData {
        products: HashMap::from([
            ("panos".to_string(), panos),
            ("esxi".to_string(), esxi),
            ("proxmox-ve".to_string(), pve),
        ]),
    }
}

fn eval(a: &Asset, components: &[Component], catalog: &Catalog, window: u32) -> AssetLifecycle {
    evaluate(a, components, catalog, &eol_data(), today(), window)
}

fn software(l: &AssetLifecycle) -> &LifecycleItem {
    l.items
        .iter()
        .find(|i| matches!(i.kind, ItemKind::Software { .. }))
        .unwrap()
}

fn kind(l: &AssetLifecycle, kind: ItemKind) -> &LifecycleItem {
    l.items.iter().find(|i| i.kind == kind).unwrap()
}

/// Scenario: EoL from endoflife.date、EoL within the reminder window、Showing lifecycle items
#[test]
fn eol_from_endoflife_date_within_the_window() {
    let l = eval(
        &asset("FW-EDGE"),
        &[component(1, "paloalto-panos", "10.2.9")],
        &catalog(""),
        90,
    );
    let s = software(&l);
    assert_eq!(s.date(), Some(ymd(2027, 3, 1)));
    assert_eq!(s.cycle.as_deref(), Some("10.2"));
    assert_eq!(s.status, Status::Soon { days_left: 50 });
    assert!(l.flags().eol_soon && !l.flags().eol);
    assert_eq!(
        s.line(),
        "PAN-OS 10.2.9（分支 10.2）EoL 2027-03-01 即將 EoL（剩 50 天），來源 endoflife.date"
    );
    assert_eq!(kind(&l, ItemKind::Warranty).line(), "保固到期 未填");
    assert_eq!(
        s.source().and_then(Source::url),
        Some("https://endoflife.date/panos")
    );
}

/// Scenario: Catalog entry for a product not covered
#[test]
fn catalog_entry_for_a_product_not_covered() {
    let local = r#"
[[product]]
key = "synology-dsm"
eol_cycles = [{ cycle = "7.1", eol = "2026-06-01", source = "https://vendor.example/dsm-7.1" }]
"#;
    let l = eval(
        &asset("NAS-01"),
        &[component(1, "synology-dsm", "7.1.1-42962")],
        &catalog(local),
        90,
    );
    let s = software(&l);
    assert_eq!(s.date(), Some(ymd(2026, 6, 1)));
    assert_eq!(
        s.source(),
        Some(&Source::Catalog("https://vendor.example/dsm-7.1".into()))
    );
    assert_eq!(
        s.status,
        Status::Past {
            days_ago: Some(223)
        }
    );
    assert!(l.flags().eol);
}

/// Scenario: Component override wins
#[test]
fn component_override_wins() {
    let mut c = component(1, "vmware-esxi", "8.0 U3c");
    c.eol_override = Some(EolOverride {
        date: ymd(2028, 12, 31),
        source: "延長支援合約".into(),
    });
    let l = eval(&asset("SRV-01"), &[c], &catalog(""), 90);
    let s = software(&l);
    assert_eq!(s.date(), Some(ymd(2028, 12, 31)));
    assert_eq!(
        s.source().and_then(Source::text).as_deref(),
        Some("延長支援合約（手動覆寫）")
    );
}

/// Scenario: Pending version
#[test]
fn pending_version_is_not_mapped() {
    let l = eval(
        &asset("NET-HW-003"),
        &[component(1, "synology-dsm", " ")],
        &catalog(""),
        90,
    );
    let s = software(&l);
    assert_eq!(s.state, ItemState::Missing(MissingReason::PendingVersion));
    assert_eq!(s.status, Status::None);
    assert!(!l.flags().any());
    assert_eq!(s.line(), "DiskStation Manager 版本待補，無法判斷 EoL");
}

/// Scenario: Hardware override、Model not in the catalog
#[test]
fn hardware_override_and_unknown_model() {
    let mut a = asset("SW-CORE");
    a.record.brand = Some("Aruba".into());
    a.record.model = Some("XYZ-100".into());
    let local = "[[hardware]]\nvendor = \"Aruba\"\nmodel = \"XYZ-100\"\neol = \"2026-10-31\"\nsource = \"https://vendor.example/xyz\"\n";
    let l = eval(&a, &[], &catalog(local), 90);
    assert_eq!(kind(&l, ItemKind::Hardware).date(), Some(ymd(2026, 10, 31)));
    assert!(l.flags().eol);
    a.record.hw_eol_override = Some(EolOverride {
        date: ymd(2027, 6, 30),
        source: "原廠延長支援函".into(),
    });
    let l = eval(&a, &[], &catalog(local), 90);
    let hw = kind(&l, ItemKind::Hardware);
    assert_eq!(hw.date(), Some(ymd(2027, 6, 30)));
    assert_eq!(
        hw.source().and_then(Source::text).as_deref(),
        Some("原廠延長支援函（手動覆寫）")
    );
    let l = eval(&asset("FW-EDGE"), &[], &catalog(""), 90);
    assert_eq!(
        kind(&l, ItemKind::Hardware).line(),
        "硬體 EoL 未填廠牌或型號"
    );
    assert!(!l.flags().eol);
}

/// Scenario: Warranty already expired、Shorter reminder window
#[test]
fn warranty_expired_and_shorter_window() {
    let mut a = asset("SRV-01");
    a.record.warranty_end = Some(ymd(2026, 12, 31));
    a.record.support_end = Some(ymd(2027, 3, 1));
    let l = eval(&a, &[], &catalog(""), 90);
    let w = kind(&l, ItemKind::Warranty);
    assert_eq!(w.status, Status::Past { days_ago: Some(10) });
    assert_eq!(w.line(), "保固到期 2026-12-31 已到期（10 天前）");
    assert!(l.flags().expired && l.flags().expiring);
    let l = eval(&a, &[], &catalog(""), 30);
    assert_eq!(
        kind(&l, ItemKind::Support).status,
        Status::Later { days_left: 50 }
    );
    assert!(!l.flags().expiring, "提醒 30 天時沒有即將到期");
}

#[test]
fn undated_and_unannounced_cycles() {
    let l = eval(
        &asset("PVE-01"),
        &[
            component(1, "proxmox-ve", "8.2.4"),
            component(2, "proxmox-ve", "9.1"),
        ],
        &catalog(""),
        90,
    );
    let items: Vec<&LifecycleItem> = l
        .items
        .iter()
        .filter(|i| matches!(i.kind, ItemKind::Software { .. }))
        .collect();
    assert!(matches!(items[0].state, ItemState::EolUndated { .. }));
    assert_eq!(items[0].status, Status::Past { days_ago: None });
    assert!(
        items[0].line().contains("已 EoL（日期未公布）"),
        "{}",
        items[0].line()
    );
    assert_eq!(
        items[1].state,
        ItemState::NotAnnounced { cycle: "9".into() }
    );
    assert!(items[1].line().contains("未公布 EoL"));
}

#[test]
fn unmapped_unsynced_and_no_data() {
    let l = eval(
        &asset("PVE-01"),
        &[
            component(1, "proxmox-ve", "6.4"),
            component(2, "nginx", "1.99.0"),
            component(3, "dell-idrac9", "7.10.30.00"),
        ],
        &catalog(""),
        90,
    );
    let states: Vec<&ItemState> = l
        .items
        .iter()
        .filter(|i| matches!(i.kind, ItemKind::Software { .. }))
        .map(|i| &i.state)
        .collect();
    assert_eq!(
        states,
        [
            &ItemState::Missing(MissingReason::Unmapped {
                version: "6.4".into()
            }),
            &ItemState::Missing(MissingReason::NotSynced),
            &ItemState::Missing(MissingReason::NoEolData),
        ]
    );
    assert_eq!(l.items[1].line(), "Proxmox VE 6.4 無法對應版本分支（6.4）");
}

#[test]
fn packages_and_unknown_products_have_no_items() {
    let pkg = Component {
        kind: ComponentKind::Package {
            purl: "pkg:deb/debian/openssl".into(),
            purl_full: "pkg:deb/debian/openssl@3.0.15".into(),
        },
        source: ComponentSource::Dpkg,
        ..component(1, "x", "3.0.15")
    };
    let unknown = component(2, "not-in-catalog", "1.0");
    let l = eval(&asset("SRV-01"), &[pkg, unknown], &catalog(""), 90);
    assert_eq!(l.items.len(), 4, "硬體與三個到期日");
}

#[test]
fn eol_today_counts_as_reached() {
    let mut a = asset("SW-CORE");
    a.record.hw_eol_override = Some(EolOverride {
        date: today(),
        source: "x".into(),
    });
    let l = eval(&a, &[], &catalog(""), 90);
    assert_eq!(
        kind(&l, ItemKind::Hardware).status,
        Status::Past { days_ago: Some(0) }
    );
}
