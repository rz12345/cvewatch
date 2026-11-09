//! 畫面編修（design D1、D9）：新增資產、修改欄位與 IP（20 design D9）、更名（12 design D1）、
//! 元件與未辨識產品的編修；退役、恢復與永久刪除見 [`super::edit_lifecycle`]（由此重新匯出）。
//!
//! 每個公開操作一個交易；表單儲存以 [`apply_edits`] 把多個步驟包在同一個交易（任何一步失敗就
//! 全部不寫入）。檢查（資產狀態、重複產品、至少一個清冊元件）都在交易內查詢，不信任呼叫端的
//! 畫面狀態；寫入後在同一交易記錄「比對待補」。驗證規則與清冊匯入相同
//! （[`validate_record`]、[`resolve_version`]）。

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::classes::AssetClasses;
use super::edit_eol::{set_component_eol, set_hardware_eol};
use super::ips::{validate_ips, AssetIpRepo, IpError, IpInput};
use super::model::{
    is_pending_version, Asset, AssetRecord, AssetStatus, Component, ComponentKind, EolOverride,
};
use super::plan::{resolve_version, VersionWarning};
use super::repo::{AssetRepo, ComponentRepo};
use super::systems::SystemDirectory;
use super::unique::UniqueField;
use super::validate::{validate_record, FieldError, RecordContext, RecordInput};
use crate::catalog::store::sync_catalog;
use crate::catalog::Catalog;
use crate::error::CoreError;
use crate::matching::run::mark_match_stale;

pub use super::edit_eol::{only_lifecycle_changed, NewLifecycle};
pub use super::edit_lifecycle::{
    purge_asset, purge_preview, restore_asset, retire_asset, PurgePreview,
};

/// 編修的結果：版本格式警告（仍保存原始字串）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditOutcome {
    pub warnings: Vec<VersionWarning>,
}

impl EditOutcome {
    fn with(warning: Option<VersionWarning>) -> EditOutcome {
        EditOutcome {
            warnings: warning.into_iter().collect(),
        }
    }
}

/// 要加入的目錄產品元件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewComponent {
    pub product_key: String,
    pub version_raw: String,
}

/// 編輯既有資產的一個步驟（[`apply_edits`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp<'a> {
    /// 更改資產編號（12 design D1）；放在其他步驟之前，之後的步驟以資產 id 操作
    Rename(&'a str),
    /// 修改欄位（`asset_tag` 不使用，編號只能以 [`EditOp::Rename`] 修改）；欄位多，以 Box 存放
    Fields(Box<RecordInput<'a>>),
    AddComponent(NewComponent),
    SetVersion {
        component_id: i64,
        version_raw: String,
    },
    RemoveComponent(i64),
    /// 未辨識產品（`asset_unresolved.id`）改為目錄產品
    ResolveUnresolved {
        unresolved_id: i64,
        product_key: String,
    },
    DeleteUnresolved(i64),
    /// 以整份清單取代 IP（位址與用途的原文；20 design D9）
    SetIps(Vec<IpInput<'a>>),
    /// 設定（Some）或清除（None）硬體 EoL 覆寫（22 design D6）
    SetHardwareEol(Option<EolOverride>),
    /// 設定或清除某產品清冊元件的軟體 EoL 覆寫（以產品 key 指定，可用於同次新增的元件）
    SetComponentEol {
        product_key: String,
        value: Option<EolOverride>,
    },
}

impl EditOp<'_> {
    /// 這個步驟是否影響比對（12 design D2）：比對結果與編號、IP、EoL 覆寫無關，更名、修改 IP
    /// 與 EoL 覆寫不需要重新比對。修改欄位只改生命週期日期時也不需要（由 [`apply_edits`] 判斷）。
    pub fn affects_matching(&self) -> bool {
        !matches!(
            self,
            EditOp::Rename(_)
                | EditOp::SetIps(_)
                | EditOp::SetHardwareEol(_)
                | EditOp::SetComponentEol { .. }
        )
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::Invalid(message.into())
}

/// 寫入後記錄「比對待補」並提交。
fn commit(tx: Transaction<'_>, now: DateTime<Utc>) -> Result<(), CoreError> {
    mark_match_stale(&tx, now)?;
    tx.commit()?;
    Ok(())
}

/// 以一個交易執行 `op`，成功時記錄「比對待補」並提交；失敗時整批回復。
pub(super) fn in_tx<T>(
    conn: &mut Connection,
    now: DateTime<Utc>,
    op: impl FnOnce(&Connection) -> Result<T, CoreError>,
) -> Result<T, CoreError> {
    let tx = conn.transaction()?;
    let out = op(&tx)?;
    commit(tx, now)?;
    Ok(out)
}

pub(super) fn find_asset(conn: &Connection, tag: &str) -> Result<Asset, CoreError> {
    AssetRepo::new(conn)
        .find_by_tag(tag)?
        .ok_or_else(|| invalid(format!("找不到資產 {tag}")))
}

/// 只允許編修 active 資產。
fn active_asset(conn: &Connection, tag: &str) -> Result<Asset, CoreError> {
    let asset = find_asset(conn, tag)?;
    if asset.status == AssetStatus::Retired {
        return Err(invalid(format!("{tag} 已退役，請先恢復再編修")));
    }
    Ok(asset)
}

/// 目錄產品在資料庫中的 id；目錄有、資料庫尚未同步（例如剛新增 catalog.d）時先同步目錄。
fn product_id(conn: &Connection, catalog: &Catalog, key: &str) -> Result<i64, CoreError> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT id FROM catalog_product WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()?;
    match found {
        Some(id) => Ok(id),
        None => sync_catalog(conn, catalog)?
            .get(key)
            .copied()
            .ok_or_else(|| invalid(format!("產品目錄中沒有 {key}"))),
    }
}

/// 清冊管理的產品元件（未移除）。
fn inventory_components(conn: &Connection, asset_id: i64) -> Result<Vec<Component>, CoreError> {
    Ok(ComponentRepo::new(conn)
        .for_asset(asset_id)?
        .into_iter()
        .filter(Component::is_inventory_product)
        .collect())
}

fn product_key(c: &Component) -> &str {
    match &c.kind {
        ComponentKind::Product { product_key, .. } => product_key,
        ComponentKind::Package { purl, .. } => purl,
    }
}

fn unresolved_count(conn: &Connection, asset_id: i64) -> Result<usize, CoreError> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM asset_unresolved WHERE asset_id = ?1",
        params![asset_id],
        |r| r.get(0),
    )?;
    Ok(usize::try_from(n).unwrap_or(0))
}

/// 資產須至少保留一個清冊元件或未辨識產品。
fn ensure_minimum(conn: &Connection, asset: &Asset) -> Result<(), CoreError> {
    if inventory_components(conn, asset.id)?.is_empty() && unresolved_count(conn, asset.id)? == 0 {
        return Err(invalid(format!(
            "{} 至少需要一個元件；不再使用此資產時請改為刪除資產",
            asset.record.asset_tag
        )));
    }
    Ok(())
}

fn version_text(version_raw: &str) -> Result<&str, CoreError> {
    let v = version_raw.trim();
    if v.is_empty() {
        return Err(invalid("缺少 version"));
    }
    Ok(v)
}

/// 寫入一個產品元件（不存在時新增；曾移除時恢復；版本變更時記錄歷史）。
fn put_product(
    conn: &Connection,
    catalog: &Catalog,
    asset: &Asset,
    key: &str,
    version_raw: &str,
    now: DateTime<Utc>,
) -> Result<Option<VersionWarning>, CoreError> {
    let product = catalog
        .get(key)
        .ok_or_else(|| invalid(format!("產品目錄中沒有 {key}")))?;
    let (norm, warning) = resolve_version(product, version_raw, &asset.record.asset_tag, None);
    let id = product_id(conn, catalog, key)?;
    ComponentRepo::new(conn).upsert_product(asset.id, id, version_raw, norm.as_deref(), now)?;
    Ok(warning)
}

fn duplicate_product(tag: &str, key: &str) -> CoreError {
    invalid(format!("{tag} 已有產品 {key}，請改為修改版本"))
}

/// 資產（未移除的元件）已有此產品時拒絕。
fn ensure_new_product(conn: &Connection, asset: &Asset, key: &str) -> Result<(), CoreError> {
    let exists = ComponentRepo::new(conn).for_asset(asset.id)?.iter().any(
        |c| matches!(&c.kind, ComponentKind::Product { product_key, .. } if product_key == key),
    );
    if exists {
        return Err(duplicate_product(&asset.record.asset_tag, key));
    }
    Ok(())
}

/// 以資料庫中的資產說明驗證欄位（16 design D2）。
fn validate_in(
    conn: &Connection,
    classes: &AssetClasses,
    input: &RecordInput<'_>,
) -> Result<AssetRecord, CoreError> {
    let systems = SystemDirectory::load(conn)?;
    let ctx = RecordContext {
        classes,
        systems: &systems,
    };
    validate_record(input, &ctx).map_err(join_field_errors)
}

fn join_field_errors(errors: Vec<FieldError>) -> CoreError {
    let messages: Vec<String> = errors.into_iter().map(|e| e.message).collect();
    invalid(messages.join("；"))
}

/// 已被使用的資產編號（不分大小寫，與畫面、等待區的查詢方式一致）；`exclude` 為不列入的
/// 資產 id（更名時排除自身，因此只改大小寫可通過）。
pub(crate) fn tag_in_use(
    conn: &Connection,
    tag: &str,
    exclude: Option<i64>,
) -> Result<Option<(String, AssetStatus)>, CoreError> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT asset_tag, status FROM asset
             WHERE asset_tag = ?1 COLLATE NOCASE AND (?2 IS NULL OR id <> ?2) LIMIT 1",
            params![tag, exclude],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.map(|(t, s)| (t, AssetStatus::parse(&s).unwrap_or(AssetStatus::Active))))
}

/// 「資產編號 NWS-HW-001 已被使用（屬於已退役的資產）」（更名與修改資產說明代碼共用）。
pub(crate) fn tag_in_use_message(tag: &str, status: AssetStatus) -> String {
    match status {
        AssetStatus::Retired => format!("資產編號 {tag} 已被使用（屬於已退役的資產）"),
        AssetStatus::Active => format!("資產編號 {tag} 已被使用"),
    }
}

/// ISMS 編號或財產編號已被其他資產（含 retired、不分大小寫）使用時拒絕；`exclude` 為資產自身
/// （20 design D3）。
fn ensure_unique_free(
    conn: &Connection,
    record: &AssetRecord,
    exclude: Option<i64>,
) -> Result<(), CoreError> {
    let repo = AssetRepo::new(conn);
    for field in UniqueField::ALL {
        let Some(value) = field.value(record) else {
            continue;
        };
        if let Some((owner, status)) = repo.value_in_use(field, value, exclude)? {
            let retired = status == AssetStatus::Retired;
            return Err(invalid(field.in_use_message(value, &owner, retired)));
        }
    }
    Ok(())
}

fn join_ip_errors(errors: Vec<IpError>) -> CoreError {
    let messages: Vec<String> = errors.into_iter().map(|e| e.message).collect();
    invalid(messages.join("；"))
}

/// 驗證並寫入 IP（整份取代）。
fn set_ips(conn: &Connection, asset: &Asset, ips: &[IpInput<'_>]) -> Result<(), CoreError> {
    let parsed = validate_ips(ips).map_err(join_ip_errors)?;
    AssetIpRepo::new(conn).replace(asset.id, &parsed)
}

/// 新增 active 資產（至少一個目錄產品元件）。`asset_tag` 已被任何資產（含 retired、
/// 不分大小寫）使用時拒絕；ISMS 編號與財產編號亦同。`ips` 依輸入順序寫入。
pub fn create_asset(
    conn: &mut Connection,
    catalog: &Catalog,
    classes: &AssetClasses,
    input: &RecordInput<'_>,
    components: &[NewComponent],
    ips: &[IpInput<'_>],
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    let lifecycle = NewLifecycle::default();
    create_asset_with(
        conn, catalog, classes, input, components, ips, &lifecycle, now,
    )
}

/// 同 [`create_asset`]，並在同一交易寫入 EoL 覆寫（22 spec Editing lifecycle fields）。
#[allow(clippy::too_many_arguments)]
pub fn create_asset_with(
    conn: &mut Connection,
    catalog: &Catalog,
    classes: &AssetClasses,
    input: &RecordInput<'_>,
    components: &[NewComponent],
    ips: &[IpInput<'_>],
    lifecycle: &NewLifecycle,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    let record = validate_in(conn, classes, input)?;
    let ips = validate_ips(ips).map_err(join_ip_errors)?;
    if components.is_empty() {
        return Err(invalid("至少需要一個元件（產品與版本）"));
    }
    let out = in_tx(conn, now, |tx| {
        if let Some((existing, status)) = tag_in_use(tx, &record.asset_tag, None)? {
            return Err(invalid(match status {
                AssetStatus::Retired => {
                    format!("資產編號 {existing} 屬於已退役的資產，可改為恢復該資產")
                }
                AssetStatus::Active => format!("資產編號 {existing} 已存在"),
            }));
        }
        ensure_unique_free(tx, &record, None)?;
        AssetRepo::new(tx).insert(&record, now)?;
        let asset = find_asset(tx, &record.asset_tag)?;
        AssetIpRepo::new(tx).replace(asset.id, &ips)?;
        let mut outcome = EditOutcome::default();
        for c in components {
            ensure_new_product(tx, &asset, &c.product_key)?;
            let version = version_text(&c.version_raw)?;
            let warning = put_product(tx, catalog, &asset, &c.product_key, version, now)?;
            outcome.warnings.extend(warning);
        }
        super::edit_eol::apply_new(tx, &asset, lifecycle)?;
        Ok(outcome)
    })?;
    tracing::info!(asset = %record.asset_tag, components = components.len(), "畫面新增資產");
    Ok(out)
}

/// 修改欄位；回傳是否需要「比對待補」（只改生命週期日期時不需要）。硬體 EoL 覆寫不在欄位
/// 輸入中，沿用資料庫中的值（同次以 [`EditOp::SetHardwareEol`] 修改者亦保留）。
fn update_fields(
    conn: &Connection,
    classes: &AssetClasses,
    asset: &Asset,
    input: &RecordInput<'_>,
    now: DateTime<Utc>,
) -> Result<bool, CoreError> {
    let input = RecordInput {
        asset_tag: &asset.record.asset_tag,
        ..*input
    };
    let current = find_asset(conn, &asset.record.asset_tag)?.record;
    let record = AssetRecord {
        hw_eol_override: current.hw_eol_override.clone(),
        ..validate_in(conn, classes, &input)?
    };
    ensure_unique_free(conn, &record, Some(asset.id))?;
    AssetRepo::new(conn).update_record(asset.id, &record, now)?;
    Ok(!only_lifecycle_changed(&current, &record))
}

fn add_to(
    conn: &Connection,
    catalog: &Catalog,
    asset: &Asset,
    component: &NewComponent,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    let version = version_text(&component.version_raw)?;
    ensure_new_product(conn, asset, &component.product_key)?;
    let warning = put_product(conn, catalog, asset, &component.product_key, version, now)?;
    Ok(EditOutcome::with(warning))
}

/// 未移除的清冊元件；不屬於此資產或為套件元件時拒絕。
fn editable_component(
    conn: &Connection,
    asset: &Asset,
    component_id: i64,
) -> Result<Component, CoreError> {
    let component = ComponentRepo::new(conn)
        .for_asset(asset.id)?
        .into_iter()
        .find(|c| c.id == component_id)
        .ok_or_else(|| {
            invalid(format!(
                "{} 沒有元件 {component_id}（可能已被移除），請重新載入",
                asset.record.asset_tag
            ))
        })?;
    if !component.is_inventory_product() {
        return Err(invalid(format!(
            "{} 由套件清單管理，請以 asset import-packages 更新",
            component.label()
        )));
    }
    Ok(component)
}

/// 元件所屬的 active 資產。
fn asset_of_component(conn: &Connection, component_id: i64) -> Result<Asset, CoreError> {
    let tag: Option<String> = conn
        .query_row(
            "SELECT a.asset_tag FROM component c JOIN asset a ON a.id = c.asset_id
             WHERE c.id = ?1 AND c.removed_at IS NULL",
            params![component_id],
            |r| r.get(0),
        )
        .optional()?;
    let tag = tag.ok_or_else(|| invalid(format!("找不到元件 {component_id}")))?;
    active_asset(conn, &tag)
}

fn set_version_of(
    conn: &Connection,
    catalog: &Catalog,
    asset: &Asset,
    component_id: i64,
    version_raw: &str,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    let component = editable_component(conn, asset, component_id)?;
    let key = product_key(&component).to_string();
    // 18 design D8：待補元件送出空白視為未變更；已知版本不能清成待補
    if is_pending_version(version_raw) {
        if component.is_pending() {
            return Ok(EditOutcome::default());
        }
        return Err(invalid(format!(
            "{} 的 {key} 版本為必填，已有版本 {} 不能清成空白",
            asset.record.asset_tag, component.version_raw
        )));
    }
    let version = version_text(version_raw)?;
    let warning = put_product(conn, catalog, asset, &key, version, now)?;
    Ok(EditOutcome::with(warning))
}

/// 移除（軟刪除）；「至少一個元件」由呼叫端檢查。
fn remove_from(
    conn: &Connection,
    asset: &Asset,
    component_id: i64,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    editable_component(conn, asset, component_id)?;
    ComponentRepo::new(conn).remove(component_id, now)?;
    Ok(EditOutcome::default())
}

/// 該資產的一筆未辨識產品（寫法與版本字串）。
fn unresolved_entry(
    conn: &Connection,
    asset: &Asset,
    unresolved_id: i64,
) -> Result<(String, String), CoreError> {
    conn.query_row(
        "SELECT product_raw, version_raw FROM asset_unresolved WHERE id = ?1 AND asset_id = ?2",
        params![unresolved_id, asset.id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| {
        invalid(format!(
            "{} 沒有這筆未辨識產品（可能已處理），請重新載入",
            asset.record.asset_tag
        ))
    })
}

fn resolve_in(
    conn: &Connection,
    catalog: &Catalog,
    asset: &Asset,
    unresolved_id: i64,
    key: &str,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    // 版本待補的未辨識產品沿用空白版本，成為版本待補元件（18 spec Mapping a pending unresolved product）
    let (_, version_raw) = unresolved_entry(conn, asset, unresolved_id)?;
    ensure_new_product(conn, asset, key)?;
    let warning = put_product(conn, catalog, asset, key, &version_raw, now)?;
    conn.execute(
        "DELETE FROM asset_unresolved WHERE id = ?1",
        params![unresolved_id],
    )?;
    Ok(EditOutcome::with(warning))
}

/// 刪除一筆未辨識產品；「至少一個元件」由呼叫端檢查。
fn delete_in(
    conn: &Connection,
    asset: &Asset,
    unresolved_id: i64,
) -> Result<EditOutcome, CoreError> {
    unresolved_entry(conn, asset, unresolved_id)?;
    conn.execute(
        "DELETE FROM asset_unresolved WHERE id = ?1",
        params![unresolved_id],
    )?;
    Ok(EditOutcome::default())
}

/// 執行一個步驟；更名時把 `asset` 換成更名後的資產，之後的步驟與錯誤訊息使用新編號。
/// 回傳結果與這個步驟是否需要「比對待補」。
fn apply_op(
    conn: &Connection,
    catalog: &Catalog,
    classes: &AssetClasses,
    asset: &mut Asset,
    op: &EditOp<'_>,
    now: DateTime<Utc>,
) -> Result<(EditOutcome, bool), CoreError> {
    if let EditOp::Fields(input) = op {
        let stale = update_fields(conn, classes, asset, input, now)?;
        return Ok((EditOutcome::default(), stale));
    }
    apply_other(conn, catalog, asset, op, now).map(|out| (out, op.affects_matching()))
}

fn apply_other(
    conn: &Connection,
    catalog: &Catalog,
    asset: &mut Asset,
    op: &EditOp<'_>,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    match op {
        EditOp::Rename(new_tag) => {
            *asset = rename_in(conn, asset, new_tag)?;
            Ok(EditOutcome::default())
        }
        EditOp::Fields(_) => unreachable!("由 apply_op 處理"),
        EditOp::AddComponent(c) => add_to(conn, catalog, asset, c, now),
        EditOp::SetVersion {
            component_id,
            version_raw,
        } => set_version_of(conn, catalog, asset, *component_id, version_raw, now),
        EditOp::RemoveComponent(id) => remove_from(conn, asset, *id, now),
        EditOp::ResolveUnresolved {
            unresolved_id,
            product_key,
        } => resolve_in(conn, catalog, asset, *unresolved_id, product_key, now),
        EditOp::DeleteUnresolved(id) => delete_in(conn, asset, *id),
        EditOp::SetIps(ips) => {
            set_ips(conn, asset, ips)?;
            Ok(EditOutcome::default())
        }
        EditOp::SetHardwareEol(value) => {
            set_hardware_eol(conn, asset, value.as_ref())?;
            Ok(EditOutcome::default())
        }
        EditOp::SetComponentEol { product_key, value } => {
            set_component_eol(conn, asset, product_key, value.as_ref())?;
            Ok(EditOutcome::default())
        }
    }
}

/// 在同一個交易依序執行編輯步驟；「至少一個元件」在全部步驟之後檢查一次
/// （因此可以先移除、再加回同一產品）。任何一步失敗時整批不寫入。有影響比對的步驟時
/// （[`EditOp::affects_matching`]；修改欄位且不只改生命週期日期）在同一交易記錄「比對待補」；
/// 只更名、修改 IP 或生命週期欄位時不記錄。
pub fn apply_edits(
    conn: &mut Connection,
    catalog: &Catalog,
    classes: &AssetClasses,
    tag: &str,
    ops: &[EditOp<'_>],
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    let tx = conn.transaction()?;
    let original = active_asset(&tx, tag)?;
    let mut asset = original.clone();
    let mut outcome = EditOutcome::default();
    let mut stale = false;
    for op in ops {
        let (out, affects) = apply_op(&tx, catalog, classes, &mut asset, op, now)?;
        outcome.warnings.extend(out.warnings);
        stale |= affects;
    }
    ensure_minimum(&tx, &asset)?;
    if stale {
        mark_match_stale(&tx, now)?;
    }
    tx.commit()?;
    log_rename(&original, &asset);
    tracing::info!(asset = %asset.record.asset_tag, steps = ops.len(), "畫面編修資產");
    Ok(outcome)
}

/// 提交後才記錄更名（交易回復時不留下沒有發生的更名紀錄）。
fn log_rename(before: &Asset, after: &Asset) {
    if before.record.asset_tag != after.record.asset_tag {
        tracing::info!(
            from = %before.record.asset_tag,
            to = %after.record.asset_tag,
            "資產更名"
        );
    }
}

/// 更名（12 design D1）：新編號去除前後空白、不可空白、不可與原編號相同；已被其他資產
/// （含 retired，不分大小寫）使用時拒絕。排除自身，因此只改大小寫可通過。回傳更名後的資產。
fn rename_in(conn: &Connection, asset: &Asset, new_tag: &str) -> Result<Asset, CoreError> {
    let old = &asset.record.asset_tag;
    let new_tag = new_tag.trim();
    if new_tag.is_empty() {
        return Err(invalid("資產編號不可空白"));
    }
    if new_tag == old {
        return Err(invalid(format!("新編號與原編號相同：{old}")));
    }
    if let Some((existing, status)) = tag_in_use(conn, new_tag, Some(asset.id))? {
        return Err(invalid(tag_in_use_message(&existing, status)));
    }
    AssetRepo::new(conn).rename(asset.id, new_tag)?;
    let mut renamed = asset.clone();
    renamed.record.asset_tag = new_tag.to_string();
    Ok(renamed)
}

/// 更改 active 資產的編號（12 design D1、D2）。元件、版本歷史、未辨識產品、命中與處理紀錄
/// 以資產 id 關聯而保留；比對結果與編號無關，因此不記錄「比對待補」。回傳實際寫入的新編號
/// （已去除前後空白）。
pub fn rename_asset(conn: &mut Connection, tag: &str, new_tag: &str) -> Result<String, CoreError> {
    let tx = conn.transaction()?;
    let asset = active_asset(&tx, tag)?;
    let renamed = rename_in(&tx, &asset, new_tag)?;
    tx.commit()?;
    log_rename(&asset, &renamed);
    Ok(renamed.record.asset_tag)
}

/// 修改 active 資產的欄位；`asset_tag` 不使用（`input.asset_tag` 被忽略，更名見 [`rename_asset`]）。
pub fn update_asset(
    conn: &mut Connection,
    classes: &AssetClasses,
    tag: &str,
    input: &RecordInput<'_>,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    let tx = conn.transaction()?;
    let stale = update_fields(&tx, classes, &active_asset(&tx, tag)?, input, now)?;
    if stale {
        mark_match_stale(&tx, now)?;
    }
    tx.commit()?;
    Ok(EditOutcome::default())
}

/// 對 active 資產新增目錄產品元件；同產品已存在時拒絕。
pub fn add_component(
    conn: &mut Connection,
    catalog: &Catalog,
    tag: &str,
    component: &NewComponent,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    in_tx(conn, now, |tx| {
        add_to(tx, catalog, &active_asset(tx, tag)?, component, now)
    })
}

/// 修改清冊元件的版本（舊版本記入版本歷史）；套件元件拒絕。
pub fn set_component_version(
    conn: &mut Connection,
    catalog: &Catalog,
    component_id: i64,
    version_raw: &str,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    in_tx(conn, now, |tx| {
        let asset = asset_of_component(tx, component_id)?;
        set_version_of(tx, catalog, &asset, component_id, version_raw, now)
    })
}

/// 移除清冊元件（軟刪除，命中與處理紀錄保留）；資產須仍保留一個清冊元件或未辨識產品。
pub fn remove_component(
    conn: &mut Connection,
    component_id: i64,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    in_tx(conn, now, |tx| {
        let asset = asset_of_component(tx, component_id)?;
        let out = remove_from(tx, &asset, component_id, now)?;
        ensure_minimum(tx, &asset)?;
        Ok(out)
    })
}

/// 把一筆未辨識產品改為目錄產品元件（沿用原版本字串），並移除該筆未辨識產品。
pub fn resolve_unresolved(
    conn: &mut Connection,
    catalog: &Catalog,
    tag: &str,
    unresolved_id: i64,
    product_key: &str,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    in_tx(conn, now, |tx| {
        let asset = active_asset(tx, tag)?;
        resolve_in(tx, catalog, &asset, unresolved_id, product_key, now)
    })
}

/// 刪除一筆未辨識產品；資產須仍保留一個清冊元件或未辨識產品。
pub fn delete_unresolved(
    conn: &mut Connection,
    tag: &str,
    unresolved_id: i64,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    in_tx(conn, now, |tx| {
        let asset = active_asset(tx, tag)?;
        let out = delete_in(tx, &asset, unresolved_id)?;
        ensure_minimum(tx, &asset)?;
        Ok(out)
    })
}
