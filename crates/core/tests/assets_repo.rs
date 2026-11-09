//! 1.3：upsert 冪等、版本變更寫 history、retired 資產不參與比對。

mod common;

use chrono::{Duration, Utc};
use cvewatch_core::assets::{
    AssetRecord, AssetRepo, AssetStatus, Cia, ClassCode, ComponentKind, ComponentRepo,
    ComponentSource, Exposure, PackageUpsert, UpsertOutcome,
};
use cvewatch_core::db::migrate::{current_version, latest_version};
use cvewatch_core::db::open_working_db;
use rusqlite::Connection;

fn product(conn: &Connection, key: &str) -> i64 {
    conn.execute(
        "INSERT INTO catalog_product (key, display_name, vendor_display, version_scheme)
         VALUES (?1, ?1, 'v', 'dotted')",
        [key],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn record(tag: &str) -> AssetRecord {
    AssetRecord {
        asset_tag: tag.to_string(),
        name: format!("{tag}-host"),
        category: ClassCode::parse("HW05").unwrap(),
        location: None,
        exposure: Exposure::Internal,
        cia: Cia::new(2, 3, 4),
        isms_id: None,
        system_code: None,
        responsible_unit: None,
        custodian: None,
        user_unit: None,
        compliance: None,
        personal_data: false,
        brand: None,
        model: None,
        serial_number: None,
        property_number: None,
        hostname: None,
        management_url: None,
        notes: None,
    }
}

#[test]
fn migration_0002_applies() {
    let env = common::initialized();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    assert_eq!(current_version(&conn).unwrap(), latest_version());
    assert!(latest_version() >= 2);
}

#[test]
fn product_upsert_is_idempotent_and_records_history() {
    let env = common::initialized();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    let dsm = product(&conn, "synology-dsm");
    let assets = AssetRepo::new(&conn);
    let components = ComponentRepo::new(&conn);
    let id = assets.insert(&record("NAS-01"), Utc::now()).unwrap();

    let t0 = Utc::now();
    assert_eq!(
        components
            .upsert_product(id, dsm, "7.2.1-69057", None, t0)
            .unwrap(),
        UpsertOutcome::Inserted
    );
    assert_eq!(
        components
            .upsert_product(id, dsm, "7.2.1-69057", None, t0)
            .unwrap(),
        UpsertOutcome::Unchanged
    );
    let t1 = t0 + Duration::days(1);
    assert_eq!(
        components
            .upsert_product(id, dsm, "7.2.2-72806", None, t1)
            .unwrap(),
        UpsertOutcome::VersionChanged {
            previous: "7.2.1-69057".into()
        }
    );

    let list = components.for_asset(id).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].version_raw, "7.2.2-72806");
    let history = components.history(list[0].id).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].version_raw, "7.2.1-69057");
    assert_eq!(history[0].changed_at, t1.to_rfc3339());
}

/// 21 code review：`asset show` 以一次查詢取得單一資產的版本歷史，不含其他資產。
#[test]
fn history_for_asset_matches_per_component_history() {
    let env = common::initialized();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    let dsm = product(&conn, "synology-dsm");
    let assets = AssetRepo::new(&conn);
    let components = ComponentRepo::new(&conn);
    let nas1 = assets.insert(&record("NAS-01"), Utc::now()).unwrap();
    let nas2 = assets.insert(&record("NAS-02"), Utc::now()).unwrap();
    let t0 = Utc::now();
    for (asset, versions) in [(nas1, ["7.1", "7.2", "7.3"]), (nas2, ["6.0", "6.1", "6.1"])] {
        for (i, v) in versions.iter().enumerate() {
            let at = t0 + Duration::days(i64::try_from(i).unwrap());
            components.upsert_product(asset, dsm, v, None, at).unwrap();
        }
    }
    let nas1_component = components.for_asset(nas1).unwrap()[0].id;
    let by_asset = components.history_for_asset(nas1).unwrap();
    assert_eq!(by_asset.len(), 1, "只含 NAS-01 的元件");
    let versions: Vec<_> = by_asset[&nas1_component]
        .iter()
        .map(|h| h.version_raw.as_str())
        .collect();
    assert_eq!(versions, ["7.2", "7.1"], "新到舊");
    assert_eq!(
        by_asset[&nas1_component],
        components.history(nas1_component).unwrap()
    );
}

#[test]
fn package_upsert_keys_on_versionless_purl() {
    let env = common::initialized();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    let assets = AssetRepo::new(&conn);
    let components = ComponentRepo::new(&conn);
    let id = assets.insert(&record("WEB-01"), Utc::now()).unwrap();
    let pkg = |v: &'static str, full: &'static str| PackageUpsert {
        purl: "pkg:deb/debian/openssl?distro=debian-12",
        purl_full: full,
        version_raw: v,
        source: ComponentSource::Dpkg,
    };
    components
        .upsert_package(
            id,
            &pkg("3.0.14", "pkg:deb/debian/openssl@3.0.14?distro=debian-12"),
            Utc::now(),
        )
        .unwrap();
    let outcome = components
        .upsert_package(
            id,
            &pkg("3.0.15", "pkg:deb/debian/openssl@3.0.15?distro=debian-12"),
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(outcome, UpsertOutcome::VersionChanged { .. }));
    let list = components.for_asset(id).unwrap();
    assert_eq!(list.len(), 1);
    assert!(matches!(
        &list[0].kind,
        ComponentKind::Package { purl_full, .. } if purl_full.contains("@3.0.15")
    ));
}

#[test]
fn retired_assets_are_excluded_from_active_components() {
    let env = common::initialized();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    let dsm = product(&conn, "synology-dsm");
    let assets = AssetRepo::new(&conn);
    let components = ComponentRepo::new(&conn);
    let a = assets.insert(&record("NAS-01"), Utc::now()).unwrap();
    let b = assets.insert(&record("NAS-02"), Utc::now()).unwrap();
    components
        .upsert_product(a, dsm, "7.2", None, Utc::now())
        .unwrap();
    components
        .upsert_product(b, dsm, "7.2", None, Utc::now())
        .unwrap();

    assets.retire(b, Utc::now()).unwrap();

    let active: Vec<String> = cvewatch_core::matching::load::MatchData::load(&conn)
        .unwrap()
        .components
        .into_iter()
        .map(|c| c.asset_tag)
        .collect();
    assert_eq!(active, vec!["NAS-01".to_string()]);
    let retired = assets.find_by_tag("NAS-02").unwrap().unwrap();
    assert_eq!(retired.status, AssetStatus::Retired);
    assert!(retired.retired_at.is_some());
    // 歷史與元件保留
    assert_eq!(components.for_asset(b).unwrap().len(), 1);
}

#[test]
fn update_record_reactivates_retired_asset() {
    let env = common::initialized();
    let conn = open_working_db(&env.paths, &env.store).unwrap();
    let assets = AssetRepo::new(&conn);
    let id = assets.insert(&record("NAS-02"), Utc::now()).unwrap();
    assets.retire(id, Utc::now()).unwrap();
    let mut changed = record("NAS-02");
    changed.exposure = Exposure::Dmz;
    assets.update_record(id, &changed, Utc::now()).unwrap();
    let a = assets.find_by_tag("NAS-02").unwrap().unwrap();
    assert_eq!(a.status, AssetStatus::Active);
    assert_eq!(a.record.exposure, Exposure::Dmz);
    assert!(a.retired_at.is_none());
}
