//! 22 任務 3.5：畫面編修的生命週期欄位——設定到期日與覆寫、清除硬體覆寫、與其他欄位同一交易、
//! 不標記「比對待補」（asset-inventory 的 Editing lifecycle fields、EoL overrides）。

mod common;

use chrono::NaiveDate;
use common::matching::{at, world, World};
use cvewatch_core::assets::edit::{self, EditOp, NewComponent, NewLifecycle};
use cvewatch_core::assets::validate::RecordInput;
use cvewatch_core::assets::{AssetRecord, AssetRepo, ComponentRepo, EolOverride};
use cvewatch_core::matching::run::match_is_stale;
use cvewatch_core::CoreError;

const NOW: &str = "2026-11-09T00:00:00+00:00";

fn now() -> chrono::DateTime<chrono::Utc> {
    at(NOW)
}

fn message(r: Result<impl std::fmt::Debug, CoreError>) -> String {
    match r {
        Err(CoreError::Invalid(m)) => m,
        other => panic!("應以 Invalid 拒絕：{other:?}"),
    }
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn eol(date: NaiveDate, source: &str) -> EolOverride {
    EolOverride {
        date,
        source: source.into(),
    }
}

fn base(name: &str) -> RecordInput<'_> {
    RecordInput {
        name,
        category: "HW01",
        exposure: "internal",
        ..RecordInput::default()
    }
}

fn record(w: &World, tag: &str) -> AssetRecord {
    AssetRepo::new(&w.conn)
        .find_by_tag(tag)
        .unwrap()
        .unwrap()
        .record
}

fn edit(w: &mut World, tag: &str, ops: &[EditOp<'_>]) -> Result<(), CoreError> {
    edit::apply_edits(&mut w.conn, &w.catalog, &w.classes, tag, ops, now()).map(|_| ())
}

/// 資產以畫面欄位寫過一次並完成比對（之後只改生命週期欄位時不應再需要比對）。
fn settled(tag: &str, components: &[(&str, &str)]) -> World {
    let mut w = world();
    w.asset(tag, "internal", components);
    edit(&mut w, tag, &[EditOp::Fields(Box::new(base(tag)))]).unwrap();
    w.run_at(NOW);
    assert!(!match_is_stale(&w.conn).unwrap());
    w
}

/// Scenario: Setting a support date on screen
#[test]
fn setting_a_support_date_on_screen() {
    let mut w = settled("SRV-01", &[("vmware-esxi", "8.0 U3c")]);
    let input = RecordInput {
        support_end: "2027-06-30",
        warranty_end: "2027/12/31",
        ..base("SRV-01")
    };
    edit(&mut w, "SRV-01", &[EditOp::Fields(Box::new(input))]).unwrap();
    let r = record(&w, "SRV-01");
    assert_eq!(r.support_end, Some(ymd(2027, 6, 30)));
    assert_eq!(r.warranty_end, Some(ymd(2027, 12, 31)));
    assert!(!match_is_stale(&w.conn).unwrap(), "命中不重新比對");

    // 同時修改影響比對的欄位時仍記錄比對待補
    let exposed = RecordInput {
        exposure: "internet",
        ..input
    };
    edit(&mut w, "SRV-01", &[EditOp::Fields(Box::new(exposed))]).unwrap();
    assert!(match_is_stale(&w.conn).unwrap());
}

#[test]
fn invalid_date_on_screen_changes_nothing() {
    let mut w = settled("SRV-01", &[("vmware-esxi", "8.0 U3c")]);
    let input = RecordInput {
        support_end: "2027-13-01",
        ..base("SRV-01")
    };
    let m = message(edit(&mut w, "SRV-01", &[EditOp::Fields(Box::new(input))]));
    assert!(m.contains("support_end 的「2027-13-01」"), "{m}");
    assert_eq!(record(&w, "SRV-01").support_end, None);
}

/// Scenario: Clearing a hardware override
#[test]
fn clearing_a_hardware_override() {
    let mut w = settled("SW-CORE", &[("hpe-aoscx", "10.13.1000")]);
    let value = eol(ymd(2027, 6, 30), "原廠延長支援函");
    // 同一批先設定覆寫、再修改欄位：欄位不覆蓋同次設定的覆寫
    let input = RecordInput {
        license_end: "2027-01-31",
        ..base("SW-CORE")
    };
    edit(
        &mut w,
        "SW-CORE",
        &[
            EditOp::SetHardwareEol(Some(value.clone())),
            EditOp::Fields(Box::new(input)),
        ],
    )
    .unwrap();
    let r = record(&w, "SW-CORE");
    assert_eq!(r.hw_eol_override, Some(value));
    assert_eq!(r.license_end, Some(ymd(2027, 1, 31)));
    assert!(!match_is_stale(&w.conn).unwrap());

    edit(
        &mut w,
        "SW-CORE",
        &[EditOp::Fields(Box::new(base("SW-CORE")))],
    )
    .unwrap();
    assert!(
        record(&w, "SW-CORE").hw_eol_override.is_some(),
        "只改欄位時保留覆寫"
    );
    edit(&mut w, "SW-CORE", &[EditOp::SetHardwareEol(None)]).unwrap();
    assert_eq!(record(&w, "SW-CORE").hw_eol_override, None);
    assert!(!match_is_stale(&w.conn).unwrap());
    assert!(!EditOp::SetHardwareEol(None).affects_matching());
}

/// Scenario: Override without a source
#[test]
fn override_without_a_source_is_rejected() {
    let mut w = settled("SW-CORE", &[("hpe-aoscx", "10.13.1000")]);
    let m = message(edit(
        &mut w,
        "SW-CORE",
        &[EditOp::SetHardwareEol(Some(eol(ymd(2027, 6, 30), " ")))],
    ));
    assert!(m.contains("來源"), "{m}");
    assert_eq!(record(&w, "SW-CORE").hw_eol_override, None);
}

#[test]
fn component_override_for_new_and_missing_products() {
    let mut w = settled("SRV-01", &[("vmware-esxi", "8.0 U2")]);
    let value = eol(ymd(2028, 12, 31), "延長支援合約");
    edit(
        &mut w,
        "SRV-01",
        &[
            EditOp::AddComponent(NewComponent {
                product_key: "dell-idrac9".into(),
                version_raw: "7.10.30.00".into(),
            }),
            EditOp::SetComponentEol {
                product_key: "dell-idrac9".into(),
                value: Some(value.clone()),
            },
            EditOp::SetComponentEol {
                product_key: "vmware-esxi".into(),
                value: Some(value.clone()),
            },
        ],
    )
    .unwrap();
    let id = AssetRepo::new(&w.conn)
        .find_by_tag("SRV-01")
        .unwrap()
        .unwrap()
        .id;
    let overrides: Vec<_> = ComponentRepo::new(&w.conn)
        .for_asset(id)
        .unwrap()
        .into_iter()
        .map(|c| c.eol_override)
        .collect();
    assert_eq!(overrides, [Some(value.clone()), Some(value.clone())]);

    let m = message(edit(
        &mut w,
        "SRV-01",
        &[EditOp::SetComponentEol {
            product_key: "synology-dsm".into(),
            value: Some(value),
        }],
    ));
    assert!(m.contains("沒有 synology-dsm 的元件"), "{m}");
}

#[test]
fn creating_an_asset_with_lifecycle_overrides() {
    let mut w = world();
    let value = eol(ymd(2028, 12, 31), "延長支援合約");
    let input = RecordInput {
        asset_tag: "SRV-09",
        warranty_end: "2027-12-31",
        ..base("srv09")
    };
    edit::create_asset_with(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &input,
        &[NewComponent {
            product_key: "vmware-esxi".into(),
            version_raw: "8.0 U3c".into(),
        }],
        &[],
        &NewLifecycle {
            hardware: Some(eol(ymd(2029, 1, 31), "原廠公告")),
            components: vec![("vmware-esxi".into(), value.clone())],
        },
        now(),
    )
    .unwrap();
    let r = record(&w, "SRV-09");
    assert_eq!(r.warranty_end, Some(ymd(2027, 12, 31)));
    assert_eq!(r.hw_eol_override.unwrap().source, "原廠公告");
}
