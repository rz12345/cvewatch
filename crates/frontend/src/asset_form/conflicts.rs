//! 以畫面快照先行檢查的重複：更名的新編號、ISMS 編號（20 design D10：由 [`super`] 移出）。
//! 讓錯誤在寫入前就標在欄位旁；core 仍在交易內再檢查一次。

use super::AssetForm;
use crate::data::Snapshot;

/// 第一個符合 `conflicts` 的其他資產（先找使用中、再找已退役；排除編輯中的 `original`），
/// 回傳欄位旁的錯誤「已被使用（FW-01）」「已被使用（已退役的 NAS-02）」。
pub(super) fn used_by(
    data: &Snapshot,
    original: Option<&str>,
    conflicts: impl Fn(&cvewatch_core::assets::AssetRecord) -> bool,
) -> Option<String> {
    let (other, retired) = data
        .assets
        .iter()
        .map(|a| (a, false))
        .chain(data.retired_assets.iter().map(|a| (a, true)))
        .find(|(a, _)| {
            let r = &a.asset.record;
            Some(r.asset_tag.as_str()) != original && conflicts(r)
        })?;
    let tag = &other.asset.record.asset_tag;
    Some(if retired {
        format!("已被使用（已退役的 {tag}）")
    } else {
        format!("已被使用（{tag}）")
    })
}

/// 更名的新編號已被其他資產（含已退役、不分大小寫）使用時，「資產編號」欄位的錯誤。
/// 以載入的資料先行檢查，讓錯誤標在欄位旁（12 code review）；core 仍在交易內再檢查一次。
pub fn rename_conflict(form: &AssetForm, data: &Snapshot) -> Option<String> {
    let new_tag = form.renamed_to()?;
    let original = form.original_tag()?;
    used_by(data, Some(original), |r| {
        r.asset_tag.eq_ignore_ascii_case(new_tag)
    })
}

/// ISMS 編號已被其他資產（含已退役、不分大小寫）使用時，「ISMS 編號」欄位的錯誤。
/// 與 [`rename_conflict`] 相同以載入的資料先行檢查；core 仍在交易內再檢查一次。
pub fn isms_conflict(form: &AssetForm, data: &Snapshot) -> Option<String> {
    let id = form.isms_id.trim();
    if id.is_empty() {
        return None;
    }
    used_by(data, form.original_tag(), |r| {
        r.isms_id
            .as_deref()
            .is_some_and(|x| x.eq_ignore_ascii_case(id))
    })
}
