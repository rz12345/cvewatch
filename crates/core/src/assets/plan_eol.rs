//! 匯入差異中的 EoL 覆寫（22 design D7）：以目錄解析 `eol` 工作表的產品、決定各資產要寫入的
//! 覆寫、列出覆寫的新增／修改／移除，以及「清冊沒有 eol 工作表」的提醒。

use std::collections::BTreeMap;

use super::dates::format_date;
use super::model::EolOverride;
use super::plan::{CurrentAsset, PlannedAsset};
use super::plan_fields::BLANK_VALUE;
use super::validate::Inventory;
use super::validate_eol::EolInput;
use crate::catalog::Catalog;

/// 清冊沒有 `eol` 工作表、且有既有資產原本就有覆寫時的提醒（整批一次）。
pub const EOL_KEPT_NOTICE: &str = "清冊沒有 eol 工作表，保留資料庫中既有資產的 EoL 覆寫";

/// 覆寫的對象。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EolTarget {
    Hardware,
    /// 清冊元件的產品 key
    Product(String),
}

/// 一筆覆寫的變更（None 為沒有覆寫）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EolChange {
    pub target: EolTarget,
    pub from: Option<EolOverride>,
    pub to: Option<EolOverride>,
}

fn describe_override(o: Option<&EolOverride>) -> String {
    o.map_or_else(
        || BLANK_VALUE.to_string(),
        |o| format!("{}（{}）", format_date(o.date), o.source),
    )
}

impl EolChange {
    /// 「硬體 EoL 覆寫 - → 2027-06-30（原廠延長支援函）」「vmware-esxi EoL 覆寫將移除（…）」。
    pub fn describe(&self) -> String {
        let target = match &self.target {
            EolTarget::Hardware => "硬體".to_string(),
            EolTarget::Product(key) => key.clone(),
        };
        match (&self.from, &self.to) {
            (Some(from), None) => format!(
                "{target} EoL 覆寫將移除（{}）",
                describe_override(Some(from))
            ),
            (from, to) => format!(
                "{target} EoL 覆寫 {} → {}",
                describe_override(from.as_ref()),
                describe_override(to.as_ref())
            ),
        }
    }
}

/// 一個資產要寫入的元件覆寫：產品 key → 覆寫（清冊有 `eol` 工作表時；None 為不變更）。
pub type ComponentEol = BTreeMap<String, EolOverride>;

/// 依 `eol` 工作表決定此資產的硬體覆寫（寫入 `planned.record`）與元件覆寫；產品無法辨識、
/// 或此資產在清冊中沒有該產品的元件時記錄錯誤。沒有工作表時沿用既有硬體覆寫。
pub(super) fn plan_asset_eol(
    inventory: &Inventory,
    catalog: &Catalog,
    planned: &mut PlannedAsset,
    existing: Option<&CurrentAsset>,
    errors: &mut Vec<String>,
) {
    let EolInput::Sheet(rows) = &inventory.eol else {
        planned.record.hw_eol_override = existing.and_then(|e| e.record.hw_eol_override.clone());
        planned.component_eol = None;
        return;
    };
    let tag = planned.record.asset_tag.clone();
    let mut hardware = None;
    let mut components = ComponentEol::new();
    let mut first_row: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows.iter().filter(|r| r.asset_tag == tag) {
        let Some(raw) = &row.product_raw else {
            hardware = Some(row.value.clone());
            continue;
        };
        let Some(product) = catalog.resolve(raw) else {
            errors.push(format!("{}：產品「{raw}」無法辨識", row.row));
            continue;
        };
        if !planned
            .components
            .iter()
            .any(|c| c.product_key == product.key)
        {
            errors.push(format!(
                "{}：{tag} 在此清冊中沒有 {} 的元件，無法設定 EoL 覆寫",
                row.row, product.display_name
            ));
            continue;
        }
        if let Some(first) = first_row.insert(product.key.clone(), row.row.number) {
            errors.push(format!(
                "{}：{tag} 的 {} EoL 覆寫與第 {first} 列重複",
                row.row, product.display_name
            ));
            continue;
        }
        components.insert(product.key.clone(), row.value.clone());
    }
    planned.record.hw_eol_override = hardware;
    planned.component_eol = Some(components);
}

/// 既有資產的覆寫變更（硬體在前，元件依產品 key）。只比較這次仍在清冊中的元件；
/// 被移除的元件連同覆寫一起移除，已列在元件移除中。
pub(super) fn eol_changes(existing: &CurrentAsset, planned: &PlannedAsset) -> Vec<EolChange> {
    let mut out = Vec::new();
    let hw_from = &existing.record.hw_eol_override;
    let hw_to = &planned.record.hw_eol_override;
    if hw_from != hw_to {
        out.push(EolChange {
            target: EolTarget::Hardware,
            from: hw_from.clone(),
            to: hw_to.clone(),
        });
    }
    let Some(planned_eol) = &planned.component_eol else {
        return out;
    };
    let mut keys: Vec<&str> = planned
        .components
        .iter()
        .map(|c| c.product_key.as_str())
        .collect();
    keys.sort_unstable();
    for key in keys {
        let from = existing.component_eol.get(key);
        let to = planned_eol.get(key);
        if from != to {
            out.push(EolChange {
                target: EolTarget::Product(key.to_string()),
                from: from.cloned(),
                to: to.cloned(),
            });
        }
    }
    out
}

/// 清冊沒有 `eol` 工作表，而既有資產原本就有覆寫（需要 [`EOL_KEPT_NOTICE`]）。
pub(super) fn keeps_eol(inventory: &Inventory, existing: &CurrentAsset) -> bool {
    matches!(inventory.eol, EolInput::Missing)
        && (existing.record.hw_eol_override.is_some() || !existing.component_eol.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn eol(y: i32, m: u32, d: u32, source: &str) -> EolOverride {
        EolOverride {
            date: NaiveDate::from_ymd_opt(y, m, d).unwrap(),
            source: source.into(),
        }
    }

    #[test]
    fn describes_additions_changes_and_removals() {
        let added = EolChange {
            target: EolTarget::Hardware,
            from: None,
            to: Some(eol(2027, 6, 30, "原廠延長支援函")),
        };
        assert_eq!(
            added.describe(),
            "硬體 EoL 覆寫 - → 2027-06-30（原廠延長支援函）"
        );
        let removed = EolChange {
            target: EolTarget::Product("vmware-esxi".into()),
            from: Some(eol(2028, 12, 31, "延長支援合約")),
            to: None,
        };
        assert_eq!(
            removed.describe(),
            "vmware-esxi EoL 覆寫將移除（2028-12-31（延長支援合約））"
        );
    }
}
