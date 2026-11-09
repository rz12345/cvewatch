//! 18 任務 3.1～3.2：編修版本待補的元件（填上待補、不得清空已知版本、新增仍須填版本）與
//! 未辨識產品改為目錄產品時沿用空白版本。

mod common;

use common::matching::{at, world, World, T0};
use cvewatch_core::assets::edit::{self, EditOp, NewComponent};
use cvewatch_core::assets::validate::RecordInput;
use cvewatch_core::assets::{AssetRepo, Component, ComponentRepo};
use cvewatch_core::matching::run::rematch_state;
use cvewatch_core::CoreError;
use rusqlite::params;

const NOW: &str = "2026-11-09T00:00:00+00:00";

fn message(r: Result<impl std::fmt::Debug, CoreError>) -> String {
    match r {
        Err(CoreError::Invalid(m)) => m,
        other => panic!("應以 Invalid 拒絕：{other:?}"),
    }
}

fn components(w: &World, tag: &str) -> Vec<Component> {
    let asset = AssetRepo::new(&w.conn).find_by_tag(tag).unwrap().unwrap();
    ComponentRepo::new(&w.conn).for_asset(asset.id).unwrap()
}

fn comp(key: &str, version: &str) -> NewComponent {
    NewComponent {
        product_key: key.into(),
        version_raw: version.into(),
    }
}

#[test]
fn filling_a_pending_version() {
    let mut w = world();
    let id = w.asset("NET-HW-003", "internal", &[("synology-dsm", "")])[0];
    edit::set_component_version(&mut w.conn, &w.catalog, id, "7.2.2-72806", at(NOW)).unwrap();
    let c = &components(&w, "NET-HW-003")[0];
    assert_eq!(c.version_raw, "7.2.2-72806");
    assert!(c.version_norm.is_some());
    assert!(ComponentRepo::new(&w.conn).history(id).unwrap().is_empty());
    assert!(
        rematch_state(&w.conn, &w.catalog, &w.settings)
            .unwrap()
            .needed(),
        "填上版本後自動比對"
    );
}

#[test]
fn clearing_a_known_version_is_refused() {
    let mut w = world();
    let id = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.6")])[0];
    let m = message(edit::set_component_version(
        &mut w.conn,
        &w.catalog,
        id,
        "",
        at(NOW),
    ));
    assert!(m.contains("必填"), "{m}");
    assert_eq!(components(&w, "FW-01")[0].version_raw, "11.1.6");
}

#[test]
fn saving_other_fields_keeps_a_pending_component() {
    let mut w = world();
    let id = w.asset("NET-HW-003", "internal", &[("synology-dsm", "")])[0];
    let input = RecordInput {
        asset_tag: "NET-HW-003",
        name: "NET-HW-003",
        category: "HW01",
        custodian: "李小華",
        ..RecordInput::default()
    };
    let ops = [
        EditOp::Fields(Box::new(input)),
        EditOp::SetVersion {
            component_id: id,
            version_raw: String::new(),
        },
    ];
    edit::apply_edits(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        "NET-HW-003",
        &ops,
        at(NOW),
    )
    .unwrap();
    let asset = AssetRepo::new(&w.conn)
        .find_by_tag("NET-HW-003")
        .unwrap()
        .unwrap();
    assert_eq!(asset.record.custodian.as_deref(), Some("李小華"));
    assert!(components(&w, "NET-HW-003")[0].is_pending());
}

#[test]
fn new_components_need_a_version() {
    let mut w = world();
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.6")]);
    let m = message(edit::add_component(
        &mut w.conn,
        &w.catalog,
        "FW-01",
        &comp("dell-idrac9", ""),
        at(NOW),
    ));
    assert!(m.contains("version"), "{m}");
    let input = RecordInput {
        asset_tag: "FW-02",
        name: "fw02",
        category: "HW03",
        ..RecordInput::default()
    };
    message(edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &w.classes,
        &input,
        &[comp("paloalto-panos", " ")],
        &[],
        at(NOW),
    ));
    assert!(AssetRepo::new(&w.conn)
        .find_by_tag("FW-02")
        .unwrap()
        .is_none());
}

#[test]
fn mapping_a_pending_unresolved_product() {
    let mut w = world();
    w.asset("NET-HW-004", "internal", &[("synology-dsm", "7.2")]);
    w.conn
        .execute(
            "INSERT INTO asset_unresolved (asset_id, product_raw, version_raw, seen_at)
             VALUES ((SELECT id FROM asset WHERE asset_tag = 'NET-HW-004'), 'Zyxel ABCD', '', ?1)",
            params![T0],
        )
        .unwrap();
    let id = w.conn.last_insert_rowid();
    let outcome = edit::resolve_unresolved(
        &mut w.conn,
        &w.catalog,
        "NET-HW-004",
        id,
        "paloalto-panos",
        at(NOW),
    )
    .unwrap();
    assert!(outcome.warnings.is_empty(), "待補不產生版本格式警告");
    let added = components(&w, "NET-HW-004")
        .into_iter()
        .find(|c| c.label() == "paloalto-panos")
        .expect("新元件");
    assert!(added.is_pending());
    assert!(AssetRepo::new(&w.conn).unresolved().unwrap().is_empty());
}
