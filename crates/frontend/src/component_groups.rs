//! 資產元件依目錄角色分組（GUI、TUI、CLI 共用；21 design D4）。
//!
//! 組的順序固定：目錄角色依 [`ComponentRole`] 的顯示順序，主機套件另成最後一組「套件」。
//! 角色只由目錄決定；目錄中已沒有的產品歸入「其他」。沒有元件的組不輸出，組內維持輸入順序。

use std::collections::BTreeMap;

use cvewatch_core::assets::{Component, ComponentKind};
use cvewatch_core::catalog::{Catalog, ComponentRole};

use crate::asset_form::ComponentDraft;

/// 主機套件組的名稱。
pub const PACKAGES_LABEL: &str = "套件";

/// 元件所屬的組；衍生的 `Ord` 即顯示順序（套件最後）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentGroup {
    Role(ComponentRole),
    Packages,
}

impl ComponentGroup {
    /// 角色中文名稱，或「套件」。
    pub fn label(self) -> &'static str {
        match self {
            ComponentGroup::Role(role) => role.label(),
            ComponentGroup::Packages => PACKAGES_LABEL,
        }
    }
}

/// 單一元件的組：套件元件為「套件」；目錄產品依角色，目錄中已沒有的 key 歸入「其他」。
pub fn group_of(catalog: &Catalog, key: &str, package: bool) -> ComponentGroup {
    if package {
        return ComponentGroup::Packages;
    }
    ComponentGroup::Role(catalog.get(key).map(|p| p.role).unwrap_or_default())
}

/// 資產元件依組排列；組內維持輸入順序，空組不輸出。
pub fn group_components<'a>(
    catalog: &Catalog,
    components: &'a [Component],
) -> Vec<(ComponentGroup, Vec<&'a Component>)> {
    let groups = components.iter().map(|c| {
        let package = matches!(c.kind, ComponentKind::Package { .. });
        group_of(catalog, c.key(), package)
    });
    group_indices(groups)
        .into_iter()
        .map(|(group, indices)| (group, indices.into_iter().map(|i| &components[i]).collect()))
        .collect()
}

/// 編輯草稿依組排列，回傳原索引（錯誤、焦點與移除仍以列索引對應）。
pub fn group_drafts(
    catalog: &Catalog,
    drafts: &[ComponentDraft],
) -> Vec<(ComponentGroup, Vec<usize>)> {
    group_indices(
        drafts
            .iter()
            .map(|d| group_of(catalog, &d.product_key, d.package)),
    )
}

/// 依組收集索引（`BTreeMap` 依組的 `Ord` 排序，組內依輸入順序；與 `groups.rs` 相同做法）。
fn group_indices(
    groups: impl Iterator<Item = ComponentGroup>,
) -> Vec<(ComponentGroup, Vec<usize>)> {
    let mut by_group: BTreeMap<ComponentGroup, Vec<usize>> = BTreeMap::new();
    for (i, group) in groups.enumerate() {
        by_group.entry(group).or_default().push(i);
    }
    by_group.into_iter().collect()
}
