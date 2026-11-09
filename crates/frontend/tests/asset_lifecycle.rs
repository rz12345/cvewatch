//! 22 任務 8.1、8.2：快照的生命週期與篩選、表單的日期與覆寫（儲存、欄位錯誤、不需要比對）。
#![allow(dead_code)]

#[path = "../../core/tests/common/mod.rs"]
mod common;

use chrono::NaiveDate;
use common::matching::{at, world, World};
use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::EolOverride;
use cvewatch_core::lifecycle::LifecycleFilter;
use cvewatch_frontend::asset_form::{
    AssetForm, DATE_INVALID, EOL_ERROR_PREFIX, OVERRIDE_SOURCE_REQUIRED,
};
use cvewatch_frontend::data::{AssetListFilter, Settings};
use cvewatch_frontend::test_support::{self, Recorder};
use cvewatch_frontend::{Context, Level, Workspace};

/// 生命週期以 UTC 日期 2027-01-10 計算（測試的顯示時區為 UTC）。
const NOW: &str = "2027-01-10T03:00:00+00:00";

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn open(w: World) -> (Workspace, Recorder) {
    let rec = Recorder::default();
    std::fs::create_dir_all(rec.documents_dir()).unwrap();
    let ctx = Context {
        conn: w.conn,
        settings: Settings {
            catalog: w.catalog,
            classes: w.classes,
            scoring: w.settings.scoring.clone(),
            stale_days: w.settings.stale_days,
            reminder_days: 90,
        },
        config: cvewatch_core::config::Config::default(),
        config_path: rec.config_path(),
        services: test_support::services(&rec, at(NOW)),
    };
    (Workspace::new(ctx).expect("workspace"), rec)
}

fn set(w: &World, tag: &str, column: &str, value: &str) {
    w.conn
        .execute(
            &format!("UPDATE asset SET {column} = ?2 WHERE asset_tag = ?1"),
            [tag, value],
        )
        .unwrap();
}

/// FW-EDGE 即將 EoL（硬體覆寫 2027-03-01）、SRV-01 合約即將到期、SW-CORE 已 EoL、
/// NAS-01 保固已到期且有版本待補、退役的 NAS-02 已 EoL。
fn lifecycle_world() -> World {
    let w = world();
    w.asset("FW-EDGE", "internet", &[("paloalto-panos", "10.2.9")]);
    set(&w, "FW-EDGE", "hw_eol_override", "2027-03-01");
    set(&w, "FW-EDGE", "hw_eol_source", "原廠公告");
    w.asset("SRV-01", "internal", &[("vmware-esxi", "8.0 U3c")]);
    set(&w, "SRV-01", "support_end", "2027-02-01");
    w.asset("SW-CORE", "internal", &[("hpe-aoscx", "10.13.1000")]);
    set(&w, "SW-CORE", "hw_eol_override", "2026-10-31");
    set(&w, "SW-CORE", "hw_eol_source", "原廠公告");
    w.asset("NAS-01", "internal", &[("synology-dsm", "")]);
    set(&w, "NAS-01", "warranty_end", "2026-12-31");
    w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.1-69057")]);
    set(&w, "NAS-02", "hw_eol_override", "2026-01-01");
    set(&w, "NAS-02", "hw_eol_source", "x");
    w.retire("NAS-02");
    w
}

fn tags(ws: &Workspace, filter: &AssetListFilter) -> Vec<String> {
    ws.data
        .asset_list(filter)
        .iter()
        .map(|a| a.asset.record.asset_tag.clone())
        .collect()
}

#[test]
fn lifecycle_filters_and_counts() {
    let (ws, _rec) = open(lifecycle_world());
    let by = |f: LifecycleFilter| AssetListFilter {
        lifecycle: Some(f),
        ..AssetListFilter::default()
    };
    assert_eq!(tags(&ws, &by(LifecycleFilter::Eol)), ["SW-CORE"]);
    assert_eq!(tags(&ws, &by(LifecycleFilter::EolSoon)), ["FW-EDGE"]);
    assert_eq!(
        tags(&ws, &by(LifecycleFilter::Expiry)),
        ["NAS-01", "SRV-01"]
    );
    let with_retired = AssetListFilter {
        include_retired: true,
        ..by(LifecycleFilter::Eol)
    };
    assert_eq!(
        tags(&ws, &with_retired),
        ["SW-CORE"],
        "退役資產不符合任何生命週期篩選"
    );
    let pending = AssetListFilter {
        pending_only: true,
        ..by(LifecycleFilter::Expiry)
    };
    assert_eq!(tags(&ws, &pending), ["NAS-01"], "可與版本待補同時使用");
    let c = ws.data.lifecycle_counts;
    assert_eq!((c.eol, c.eol_soon, c.expiry, c.window), (1, 1, 2, 90));
    assert_eq!(ws.data.lifecycle_today, ymd(2027, 1, 10));
}

#[test]
fn crossing_a_day_recomputes_the_lifecycle() {
    let (mut ws, _rec) = open(lifecycle_world());
    assert!(!ws.data.refresh_lifecycle(&ws.settings, ymd(2027, 1, 10)));
    let settings = std::mem::replace(
        &mut ws.settings,
        Settings {
            catalog: cvewatch_core::catalog::Catalog::seed().unwrap(),
            classes: cvewatch_core::assets::AssetClasses::builtin(),
            scoring: Default::default(),
            stale_days: 14,
            reminder_days: 90,
        },
    );
    assert!(ws.data.refresh_lifecycle(&settings, ymd(2027, 3, 1)));
    assert_eq!(ws.data.lifecycle_counts.eol, 2, "FW-EDGE 當天到期");
}

#[test]
fn saving_a_warranty_date_does_not_need_matching() {
    let (mut ws, _rec) = open(lifecycle_world());
    let mut form = AssetForm::from_entry(ws.data.asset("SRV-01").unwrap());
    form.lifecycle.warranty_end = "2027/12/31".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    assert!(!r.needs_match, "只改生命週期日期不需要比對");
    let saved = ws.data.asset("SRV-01").unwrap();
    assert_eq!(saved.asset.record.warranty_end, Some(ymd(2027, 12, 31)));
    let form = AssetForm::from_entry(saved);
    assert_eq!(form.lifecycle.warranty_end, "2027-12-31");
}

#[test]
fn invalid_date_is_reported_on_its_field() {
    let (mut ws, _rec) = open(lifecycle_world());
    let mut form = AssetForm::from_entry(ws.data.asset("SRV-01").unwrap());
    form.lifecycle.support_end = "2027-13-01".into();
    let errors = ws.validate_asset_form(&form).unwrap_err();
    assert_eq!(errors.field(StandardField::SupportEnd), Some(DATE_INVALID));
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Error);
    assert_eq!(
        ws.data.asset("SRV-01").unwrap().asset.record.support_end,
        Some(ymd(2027, 2, 1)),
        "資料不變"
    );
}

#[test]
fn override_without_a_source_is_reported() {
    let (ws, _rec) = open(lifecycle_world());
    let mut form = AssetForm::from_entry(ws.data.asset("SRV-01").unwrap());
    form.lifecycle.hardware_date = "2027-06-30".into();
    let errors = ws.validate_asset_form(&form).unwrap_err();
    assert_eq!(
        errors.field(StandardField::Source),
        Some(OVERRIDE_SOURCE_REQUIRED)
    );
    let mut form = AssetForm::from_entry(ws.data.asset("SRV-01").unwrap());
    form.components[0].eol.date = "2028-12-31".into();
    let errors = ws.validate_asset_form(&form).unwrap_err();
    let row = errors.component(0);
    assert_eq!(
        row,
        [format!("{EOL_ERROR_PREFIX}{OVERRIDE_SOURCE_REQUIRED}")]
    );
}

#[test]
fn hardware_and_component_overrides_are_saved_and_cleared() {
    let (mut ws, _rec) = open(lifecycle_world());
    let mut form = AssetForm::from_entry(ws.data.asset("SRV-01").unwrap());
    form.lifecycle.hardware_date = "2029-01-31".into();
    form.lifecycle.hardware_source = "原廠延長支援函".into();
    form.components[0].eol.date = "2028-12-31".into();
    form.components[0].eol.source = "延長支援合約".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    assert!(!r.needs_match);
    let entry = ws.data.asset("SRV-01").unwrap();
    assert_eq!(
        entry.asset.record.hw_eol_override,
        Some(EolOverride {
            date: ymd(2029, 1, 31),
            source: "原廠延長支援函".into()
        })
    );
    assert_eq!(
        entry.components[0].eol_override.as_ref().map(|o| o.date),
        Some(ymd(2028, 12, 31))
    );
    let mut form = AssetForm::from_entry(entry);
    form.lifecycle.clear_hardware_override();
    form.components[0].eol.clear();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    let entry = ws.data.asset("SRV-01").unwrap();
    assert_eq!(entry.asset.record.hw_eol_override, None);
    assert_eq!(entry.components[0].eol_override, None);
}

#[test]
fn new_asset_with_lifecycle_fields() {
    let (mut ws, _rec) = open(world());
    let mut form = AssetForm::new();
    form.asset_tag = "SRV-09".into();
    form.name = "srv09".into();
    form.category = "HW01".into();
    form.add_product("vmware-esxi");
    form.components[0].version = "8.0 U3c".into();
    form.components[0].eol.date = "2028-12-31".into();
    form.components[0].eol.source = "延長支援合約".into();
    form.lifecycle.license_end = "2027-06-30".into();
    let r = ws.save_asset_form(&form, &[]);
    assert_eq!(r.report.level, Level::Info, "{}", r.report.message);
    let entry = ws.data.asset("SRV-09").unwrap();
    assert_eq!(entry.asset.record.license_end, Some(ymd(2027, 6, 30)));
    assert!(entry.components[0].eol_override.is_some());
}
