//! `asset show` 的元件區塊：依目錄角色分組輸出元件與版本歷史，未辨識產品在所有組之後
//! （21 design D7）。

use std::collections::HashMap;

use cvewatch_core::assets::{Component, HistoryEntry, UnresolvedEntry};
use cvewatch_core::catalog::Catalog;
use cvewatch_frontend::component_groups::group_components;

/// 「元件：」之下的輸出行：每組 `  {組名}：`，組內元件 `    {label}  {version}（{source}）`，
/// 版本歷史 `        先前 …，於 … 變更`，最後是 `  未辨識：「…」…`。
pub fn component_lines(
    catalog: &Catalog,
    components: &[Component],
    history: &HashMap<i64, Vec<HistoryEntry>>,
    unresolved: &[UnresolvedEntry],
) -> Vec<String> {
    let grouped = group_components(catalog, components)
        .into_iter()
        .flat_map(|(group, members)| {
            std::iter::once(format!("  {}：", group.label())).chain(
                members
                    .into_iter()
                    .flat_map(|c| component_entry(c, history.get(&c.id))),
            )
        });
    let unresolved = unresolved
        .iter()
        .map(|u| format!("  未辨識：「{}」{}", u.product_raw, u.version_raw));
    grouped.chain(unresolved).collect()
}

fn component_entry(c: &Component, history: Option<&Vec<HistoryEntry>>) -> Vec<String> {
    std::iter::once(format!(
        "    {}  {}（{}）",
        c.label(),
        c.version_raw,
        c.source
    ))
    .chain(
        history
            .into_iter()
            .flatten()
            .map(|h| format!("        先前 {}，於 {} 變更", h.version_raw, h.changed_at)),
    )
    .collect()
}

#[cfg(test)]
mod tests {
    use cvewatch_core::assets::{ComponentKind, ComponentSource};

    use super::*;

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
            eol_override: None,
        }
    }

    fn package(id: i64, purl: &str, version: &str) -> Component {
        Component {
            id,
            asset_id: 1,
            kind: ComponentKind::Package {
                purl: purl.into(),
                purl_full: format!("{purl}@{version}"),
            },
            version_raw: version.into(),
            version_norm: None,
            source: ComponentSource::Dpkg,
            updated_at: "2026-11-09T00:00:00+00:00".into(),
            eol_override: None,
        }
    }

    /// 21 asset-inventory：Grouped output on the command line。
    #[test]
    fn grouped_output_on_the_command_line() {
        let catalog = Catalog::seed().unwrap();
        let components = vec![
            product(1, "ubuntu", "22.04"),
            package(2, "pkg:deb/ubuntu/libc6", "2.35"),
            product(3, "dell-idrac9", "7.00.00.00"),
        ];
        let history = HashMap::from([(
            3,
            vec![HistoryEntry {
                version_raw: "6.10.00.00".into(),
                changed_at: "2026-10-01T00:00:00+00:00".into(),
            }],
        )]);
        assert_eq!(
            component_lines(&catalog, &components, &history, &[]),
            [
                "  BMC：",
                "    dell-idrac9  7.00.00.00（excel）",
                "        先前 6.10.00.00，於 2026-10-01T00:00:00+00:00 變更",
                "  作業系統：",
                "    ubuntu  22.04（excel）",
                "  套件：",
                "    pkg:deb/ubuntu/libc6@2.35  2.35（dpkg）",
            ]
        );
    }

    /// 21 asset-inventory：Unresolved products after the groups。
    #[test]
    fn unresolved_products_after_the_groups() {
        let catalog = Catalog::seed().unwrap();
        let unresolved = [UnresolvedEntry {
            id: 1,
            asset_tag: "FW-EDGE".into(),
            product_raw: "Example Agent".into(),
            version_raw: "1.0".into(),
        }];
        assert_eq!(
            component_lines(
                &catalog,
                &[product(1, "paloalto-panos", "11.1.2")],
                &HashMap::new(),
                &unresolved
            ),
            [
                "  韌體：",
                "    paloalto-panos  11.1.2（excel）",
                "  未辨識：「Example Agent」1.0",
            ]
        );
    }
}
