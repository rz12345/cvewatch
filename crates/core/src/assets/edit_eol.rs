//! 畫面編修的生命週期欄位（22 design D6、spec Editing lifecycle fields）：硬體與元件的 EoL
//! 覆寫；三個到期日隨 [`super::edit::EditOp::Fields`] 寫入。生命週期欄位不影響比對。

use rusqlite::{params, Connection};

use super::edit::invalid;
use super::model::{Asset, AssetRecord, ComponentKind, EolOverride};
use super::record_lifecycle::OVERRIDE_SOURCE_REQUIRED;
use super::repo::{override_params, ComponentRepo};
use crate::error::CoreError;

/// 新增資產時一併設定的 EoL 覆寫。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewLifecycle {
    pub hardware: Option<EolOverride>,
    /// 產品 key → 該清冊元件的軟體 EoL 覆寫
    pub components: Vec<(String, EolOverride)>,
}

/// 覆寫的來源必須有內容（畫面已先驗證；此處防止呼叫端繞過）。
fn ensure_source(value: Option<&EolOverride>) -> Result<(), CoreError> {
    match value {
        Some(o) if o.source.trim().is_empty() => Err(invalid(OVERRIDE_SOURCE_REQUIRED)),
        _ => Ok(()),
    }
}

/// 設定或清除資產的硬體 EoL 覆寫（日期與來源一起寫入）。
pub(super) fn set_hardware_eol(
    conn: &Connection,
    asset: &Asset,
    value: Option<&EolOverride>,
) -> Result<(), CoreError> {
    ensure_source(value)?;
    let (date, source) = override_params(value);
    conn.execute(
        "UPDATE asset SET hw_eol_override = ?2, hw_eol_source = ?3 WHERE id = ?1",
        params![asset.id, date, source],
    )?;
    Ok(())
}

/// 設定或清除該資產某產品清冊元件的軟體 EoL 覆寫；沒有該產品的清冊元件時拒絕。
pub(super) fn set_component_eol(
    conn: &Connection,
    asset: &Asset,
    product_key: &str,
    value: Option<&EolOverride>,
) -> Result<(), CoreError> {
    ensure_source(value)?;
    let component = ComponentRepo::new(conn)
        .for_asset(asset.id)?
        .into_iter()
        .filter(|c| c.is_inventory_product())
        .find(|c| matches!(&c.kind, ComponentKind::Product { product_key: k, .. } if k == product_key))
        .ok_or_else(|| {
            invalid(format!(
                "{} 沒有 {product_key} 的元件，無法設定 EoL 覆寫",
                asset.record.asset_tag
            ))
        })?;
    ComponentRepo::new(conn).set_eol_override(component.id, value)
}

/// 新增資產後寫入覆寫（與新增同一交易）。
pub(super) fn apply_new(
    conn: &Connection,
    asset: &Asset,
    lifecycle: &NewLifecycle,
) -> Result<(), CoreError> {
    if lifecycle.hardware.is_some() {
        set_hardware_eol(conn, asset, lifecycle.hardware.as_ref())?;
    }
    for (key, value) in &lifecycle.components {
        set_component_eol(conn, asset, key, Some(value))?;
    }
    Ok(())
}

/// 兩筆紀錄只差在生命週期日期（不影響比對，不需要「比對待補」）。
pub fn only_lifecycle_changed(old: &AssetRecord, new: &AssetRecord) -> bool {
    let strip = |r: &AssetRecord| AssetRecord {
        warranty_end: None,
        support_end: None,
        license_end: None,
        hw_eol_override: None,
        ..r.clone()
    };
    old != new && strip(old) == strip(new)
}
