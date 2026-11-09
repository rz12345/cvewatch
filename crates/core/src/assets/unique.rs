//! 不分大小寫唯一的資產欄位（13 的 ISMS 編號、20 的財產編號；20 code review 合併）：
//! 匯入時與清冊以外資產的衝突、編修時的檢查、互換時先清除，以及資料庫查詢共用同一套規則
//! （只比 ASCII 大小寫，與 SQLite `COLLATE NOCASE` 一致；含已退役資產）。

use std::collections::{HashMap, HashSet};

use super::model::{AssetRecord, AssetStatus};
use super::plan::{isms_in_use_message, Snapshot};
use super::plan_identity::property_in_use_message;
use super::validate::Inventory;

/// 唯一欄位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniqueField {
    IsmsId,
    PropertyNumber,
}

impl UniqueField {
    pub const ALL: [UniqueField; 2] = [UniqueField::IsmsId, UniqueField::PropertyNumber];

    /// 資料表欄名（固定字串，可安全放進 SQL）。
    pub(super) fn column(self) -> &'static str {
        match self {
            UniqueField::IsmsId => "isms_id",
            UniqueField::PropertyNumber => "property_number",
        }
    }

    pub fn value(self, r: &AssetRecord) -> Option<&String> {
        match self {
            UniqueField::IsmsId => r.isms_id.as_ref(),
            UniqueField::PropertyNumber => r.property_number.as_ref(),
        }
    }

    /// 「ISMS 編號 … 已被 NAS-02 使用（該資產已退役）」「財產編號 … 已被 …」。
    pub fn in_use_message(self, value: &str, owner: &str, retired: bool) -> String {
        match self {
            UniqueField::IsmsId => isms_in_use_message(value, owner, retired),
            UniqueField::PropertyNumber => property_in_use_message(value, owner, retired),
        }
    }
}

/// 值（小寫）→（asset_tag, 是否退役）。
pub(super) type Owners = HashMap<String, (String, bool)>;

/// 匯入後仍保留該欄位值的既有資產（不在這次清冊中者，含 retired）。清冊中的資產會改用清冊的
/// 值，不列入。
pub(super) fn owners(field: UniqueField, inventory: &Inventory, current: &Snapshot) -> Owners {
    let incoming: HashSet<&str> = inventory
        .assets
        .iter()
        .map(|a| a.record.asset_tag.as_str())
        .collect();
    current
        .values()
        .filter(|a| !incoming.contains(a.record.asset_tag.as_str()))
        .filter_map(|a| {
            let v = field.value(&a.record)?;
            Some((
                v.to_ascii_lowercase(),
                (a.record.asset_tag.clone(), a.status == AssetStatus::Retired),
            ))
        })
        .collect()
}

/// 值已被清冊以外的既有資產使用時的錯誤訊息。
pub(super) fn conflict(
    field: UniqueField,
    record: &AssetRecord,
    owners: &Owners,
) -> Option<String> {
    let v = field.value(record)?;
    let (owner, retired) = owners.get(&v.to_ascii_lowercase())?;
    Some(field.in_use_message(v, owner, *retired))
}
