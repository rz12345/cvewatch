//! 21 任務 3.2：元件與編輯草稿依目錄角色分組（套件另成一組、空組不輸出、組內維持順序）。

use cvewatch_core::assets::{Component, ComponentKind, ComponentSource};
use cvewatch_core::catalog::{Catalog, CatalogSource, ComponentRole, SEED_SOURCE_NAME, SEED_TOML};
use cvewatch_frontend::asset_form::ComponentDraft;
use cvewatch_frontend::component_groups::{
    group_components, group_drafts, group_of, ComponentGroup, PACKAGES_LABEL,
};

fn catalog_with(user: &str) -> Catalog {
    Catalog::from_sources(&[
        CatalogSource {
            name: SEED_SOURCE_NAME.into(),
            text: SEED_TOML.into(),
        },
        CatalogSource {
            name: "site.toml".into(),
            text: user.into(),
        },
    ])
    .unwrap()
}

fn product(id: i64, key: &str, version: &str) -> Component {
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
        updated_at: "2026-11-09T00:00:00+00:00".into(),
    }
}

fn package(id: i64, name: &str) -> Component {
    Component {
        id,
        asset_id: 1,
        kind: ComponentKind::Package {
            purl: format!("pkg:deb/ubuntu/{name}"),
            purl_full: format!("pkg:deb/ubuntu/{name}@1.0"),
        },
        version_raw: "1.0".into(),
        version_norm: None,
        source: ComponentSource::Dpkg,
        updated_at: "2026-11-09T00:00:00+00:00".into(),
    }
}

/// `SRV-01`：MySQL、Ubuntu、PowerEdge BIOS、nginx、iDRAC9 與兩個 dpkg 套件。
fn srv01() -> Vec<Component> {
    vec![
        product(1, "mysql", "8.0.36"),
        product(2, "ubuntu", "22.04"),
        product(3, "dell-poweredge-bios", "2.19.1"),
        package(4, "libc6"),
        product(5, "nginx", "1.24.0"),
        package(6, "zlib1g"),
        product(7, "dell-idrac9", "7.00.00.00"),
    ]
}

fn summary(groups: &[(ComponentGroup, Vec<&Component>)]) -> Vec<(&'static str, Vec<i64>)> {
    groups
        .iter()
        .map(|(g, cs)| (g.label(), cs.iter().map(|c| c.id).collect()))
        .collect()
}

#[test]
fn server_components_in_role_order() {
    let catalog = Catalog::seed().unwrap();
    let components = srv01();
    let groups = group_components(&catalog, &components);
    assert_eq!(
        summary(&groups),
        vec![
            ("BIOS", vec![3]),
            ("BMC", vec![7]),
            ("作業系統", vec![2]),
            ("資料庫管理系統", vec![1]),
            ("網站服務系統", vec![5]),
            (PACKAGES_LABEL, vec![4, 6]),
        ]
    );
    assert_eq!(components[0].id, 1, "不改變輸入");
}

#[test]
fn role_changed_locally() {
    let catalog = catalog_with("[[product]]\nkey = \"nginx\"\nrole = \"tool\"\n");
    let components = srv01();
    let groups = group_components(&catalog, &components);
    let labels: Vec<_> = summary(&groups);
    assert!(labels.contains(&("應用工具軟體", vec![5])), "{labels:?}");
    assert!(!labels.iter().any(|(l, _)| *l == "網站服務系統"));
}

#[test]
fn product_removed_from_the_catalog_is_other() {
    let catalog = Catalog::seed().unwrap();
    let components = vec![
        product(1, "example-fw", "1.0"),
        product(2, "paloalto-panos", "11.1.6"),
    ];
    let groups = group_components(&catalog, &components);
    assert_eq!(summary(&groups), vec![("韌體", vec![2]), ("其他", vec![1])]);
    assert_eq!(
        group_of(&catalog, "example-fw", false),
        ComponentGroup::Role(ComponentRole::Other)
    );
}

#[test]
fn pending_component_keeps_its_group() {
    let catalog = Catalog::seed().unwrap();
    let components = vec![product(1, "paloalto-panos", "")];
    assert!(components[0].is_pending());
    let groups = group_components(&catalog, &components);
    assert_eq!(summary(&groups), vec![("韌體", vec![1])]);
}

#[test]
fn drafts_return_original_indices_and_packages_last() {
    let catalog = Catalog::seed().unwrap();
    let pkg = ComponentDraft {
        package: true,
        ..ComponentDraft::new("pkg:deb/ubuntu/libc6")
    };
    let drafts = vec![
        ComponentDraft::new("mysql"),
        pkg,
        ComponentDraft::new("ubuntu"),
        ComponentDraft::new("dell-idrac9"),
        ComponentDraft::new("nginx"),
    ];
    let groups: Vec<_> = group_drafts(&catalog, &drafts)
        .into_iter()
        .map(|(g, ix)| (g.label(), ix))
        .collect();
    assert_eq!(
        groups,
        vec![
            ("BMC", vec![3]),
            ("作業系統", vec![2]),
            ("資料庫管理系統", vec![0]),
            ("網站服務系統", vec![4]),
            (PACKAGES_LABEL, vec![1]),
        ]
    );
}

#[test]
fn empty_input_has_no_groups() {
    let catalog = Catalog::seed().unwrap();
    assert!(group_components(&catalog, &[]).is_empty());
    assert!(group_drafts(&catalog, &[]).is_empty());
}
