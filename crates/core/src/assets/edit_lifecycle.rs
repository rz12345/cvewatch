//! 資產的退役、恢復與永久刪除（20 design D9：由 [`super::edit`] 移出，原樣保留行為，
//! 由 `edit` 重新匯出）。

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use super::edit::{find_asset, in_tx, invalid, EditOutcome};
use super::model::{Asset, AssetStatus};
use super::repo::AssetRepo;
use crate::error::CoreError;

/// 永久刪除會一併移除的資料量（確認對話框用）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PurgePreview {
    /// 元件數（含先前已移除、仍保留紀錄的元件）
    pub components: usize,
    /// 命中數
    pub findings: usize,
}

/// 刪除 = 標為退役（與匯入 `--prune` 相同：保留命中與處理紀錄、不再參與比對）。
pub fn retire_asset(
    conn: &mut Connection,
    tag: &str,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    in_tx(conn, now, |tx| {
        let asset = find_asset(tx, tag)?;
        if asset.status == AssetStatus::Retired {
            return Err(invalid(format!("{tag} 已經是退役狀態")));
        }
        AssetRepo::new(tx).retire(asset.id, now)?;
        Ok(EditOutcome::default())
    })?;
    tracing::info!(asset = %tag, "畫面退役資產");
    Ok(EditOutcome::default())
}

/// 恢復已退役資產（design D8）。
pub fn restore_asset(
    conn: &mut Connection,
    tag: &str,
    now: DateTime<Utc>,
) -> Result<EditOutcome, CoreError> {
    in_tx(conn, now, |tx| {
        let asset = find_asset(tx, tag)?;
        if asset.status == AssetStatus::Active {
            return Err(invalid(format!("{tag} 未退役，不需要恢復")));
        }
        let repo = AssetRepo::new(tx);
        repo.restore(asset.id, now)?;
        repo.mark_components_updated(asset.id, now)?;
        Ok(EditOutcome::default())
    })?;
    tracing::info!(asset = %tag, "畫面恢復資產");
    Ok(EditOutcome::default())
}

fn retired_asset(conn: &Connection, tag: &str) -> Result<Asset, CoreError> {
    let asset = find_asset(conn, tag)?;
    if asset.status == AssetStatus::Active {
        return Err(invalid(format!(
            "{tag} 仍在使用中，需先標為退役才能永久刪除"
        )));
    }
    Ok(asset)
}

fn preview_of(conn: &Connection, asset_id: i64) -> Result<PurgePreview, CoreError> {
    let (components, findings): (i64, i64) = conn.query_row(
        "SELECT (SELECT count(*) FROM component WHERE asset_id = ?1),
                (SELECT count(*) FROM finding f JOIN component c ON c.id = f.component_id
                 WHERE c.asset_id = ?1)",
        params![asset_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(PurgePreview {
        components: usize::try_from(components).unwrap_or(0),
        findings: usize::try_from(findings).unwrap_or(0),
    })
}

/// 永久刪除前的預覽（唯讀）；只允許 retired 資產。
pub fn purge_preview(conn: &Connection, tag: &str) -> Result<PurgePreview, CoreError> {
    let asset = retired_asset(conn, tag)?;
    preview_of(conn, asset.id)
}

/// 永久刪除 retired 資產（design D7）：刪除 `asset` 一列，元件、版本歷史、未辨識產品、命中與
/// 處理紀錄依外鍵 `ON DELETE CASCADE` 一併移除。CVE、等待區、Redmine 議題對應與送出佇列不受影響
/// （佇列項目自帶內容，不回查命中）。
pub fn purge_asset(
    conn: &mut Connection,
    tag: &str,
    now: DateTime<Utc>,
) -> Result<PurgePreview, CoreError> {
    let preview = in_tx(conn, now, |tx| {
        let asset = retired_asset(tx, tag)?;
        let preview = preview_of(tx, asset.id)?;
        tx.execute("DELETE FROM asset WHERE id = ?1", params![asset.id])?;
        Ok(preview)
    })?;
    tracing::info!(
        asset = %tag,
        components = preview.components,
        findings = preview.findings,
        "永久刪除資產"
    );
    Ok(preview)
}
