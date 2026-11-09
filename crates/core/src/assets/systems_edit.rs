//! 修改資產說明（19 design D1～D4）：代碼與名稱可一起修改。
//!
//! 代碼改變時，在同一個交易內把所有歸屬的資產（含 retired）改為歸屬新代碼、ISMS 編號的系統
//! 代碼改為新代碼，資產編號等於舊 ISMS 編號（不分大小寫）的資產改為新 ISMS 編號（比照資產
//! 更名，元件、命中與處理紀錄以資產 id 關聯而保留，不記錄「比對待補」）。任何一個新編號已被
//! 其他資產使用、或同一批中重複時整批拒絕。[`plan_system_edit`] 只讀，供畫面與 CLI 預覽；
//! [`edit_system`] 在交易內重新計算計畫後套用，不信任呼叫端的快照；傳入確認時的計畫而重算
//! 結果不同時不寫入（[`SystemEditError::Changed`]）。

use std::collections::HashMap;

use rusqlite::{params, Connection};

use super::edit::{tag_in_use, tag_in_use_message};
use super::model::AssetStatus;
use super::plan::isms_in_use_message;
use super::record_admin::isms_system_code;
use super::repo::AssetRepo;
use super::systems::{
    join, name_key, validate_fields, AssetSystem, SystemCode, SystemDirectory, SystemError,
    SystemField, SystemRepo,
};
use crate::error::CoreError;

/// 一次修改：原代碼、新代碼（使用者輸入，尚未正規化）與名稱。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemEdit {
    pub code: SystemCode,
    pub new_code: String,
    pub name: String,
}

/// 代碼改變時受影響的一個資產。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecodedAsset {
    pub id: i64,
    /// 修改前的資產編號
    pub asset_tag: String,
    pub status: AssetStatus,
    /// ISMS 編號的 (舊, 新)；沒有 ISMS 編號，或其系統代碼不是原代碼時為 None
    pub isms_id: Option<(String, String)>,
    /// 資產編號等於舊 ISMS 編號時的新編號
    pub new_tag: Option<String>,
}

/// 修改的計畫（預覽與實際套用的結果相同）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemEditPlan {
    pub before: AssetSystem,
    pub after: AssetSystem,
    /// 歸屬的資產（依資產編號排序）；代碼未變時為空
    pub assets: Vec<RecodedAsset>,
}

impl SystemEditPlan {
    pub fn code_changed(&self) -> bool {
        self.before.code != self.after.code
    }

    /// 受影響資產中已退役的數量。
    pub fn retired(&self) -> usize {
        self.assets
            .iter()
            .filter(|a| a.status == AssetStatus::Retired)
            .count()
    }

    /// ISMS 編號的 (舊, 新)。
    pub fn isms_changes(&self) -> Vec<(&str, &str)> {
        self.assets
            .iter()
            .filter_map(|a| a.isms_id.as_ref())
            .map(|(old, new)| (old.as_str(), new.as_str()))
            .collect()
    }

    /// 資產編號的 (舊, 新)。
    pub fn tag_changes(&self) -> Vec<(&str, &str)> {
        self.assets
            .iter()
            .filter_map(|a| a.new_tag.as_deref().map(|n| (a.asset_tag.as_str(), n)))
            .collect()
    }
}

/// 修改失敗：欄位錯誤（畫面標在欄位旁；資產編號衝突歸在代碼欄）、交易內重算的計畫與確認時
/// 不同（附上新計畫，畫面重新確認），或其他錯誤。
#[derive(Debug)]
pub enum SystemEditError {
    Fields(Vec<SystemError>),
    Changed(Box<SystemEditPlan>),
    Failed(CoreError),
}

/// 確認後資料已變動時的說明。
pub const PLAN_CHANGED: &str = "受影響的資產在確認後有變動，未寫入；請重新確認";

impl From<CoreError> for SystemEditError {
    fn from(e: CoreError) -> Self {
        SystemEditError::Failed(e)
    }
}

impl From<rusqlite::Error> for SystemEditError {
    fn from(e: rusqlite::Error) -> Self {
        SystemEditError::Failed(e.into())
    }
}

impl From<SystemEditError> for CoreError {
    fn from(e: SystemEditError) -> Self {
        match e {
            SystemEditError::Fields(errors) => join(errors),
            SystemEditError::Changed(_) => CoreError::Invalid(PLAN_CHANGED.to_string()),
            SystemEditError::Failed(e) => e,
        }
    }
}

/// 驗證修改（19 design D2）：原代碼須存在；新代碼與名稱依新增的規則檢查，但以原代碼排除
/// 自身。新代碼正規化後等於原代碼時視為只改名稱。
pub fn validate_edit(
    edit: &SystemEdit,
    dir: &SystemDirectory,
) -> Result<AssetSystem, Vec<SystemError>> {
    let mut errors = Vec::new();
    if dir.get(&edit.code).is_none() {
        errors.push(SystemError::new(
            SystemField::Code,
            format!("資產說明 {} 不存在", edit.code),
        ));
    }
    let system = validate_fields(
        &edit.new_code,
        &edit.name,
        Some(&edit.code),
        dir,
        &mut errors,
    );
    match system {
        Some(system) if errors.is_empty() => Ok(system),
        _ => Err(errors),
    }
}

/// ISMS 編號的系統代碼為 `old` 時，換成 `new` 後的編號（其餘保留使用者寫法，
/// `NET-hw-001` → `NWS-hw-001`）。系統代碼的判斷與 membership 規則相同（[`isms_system_code`]：
/// 第一個 `-` 之前、全形視同半形、不分大小寫）；其他編號為 None。
fn recode(isms_id: &str, old: &SystemCode, new: &SystemCode) -> Option<String> {
    if isms_system_code(isms_id).as_ref() != Some(old) {
        return None;
    }
    Some(match isms_id.split_once('-') {
        Some((_, rest)) => format!("{new}-{rest}"),
        None => new.to_string(),
    })
}

struct Member {
    id: i64,
    tag: String,
    status: AssetStatus,
    isms_id: Option<String>,
}

fn members(conn: &Connection, code: &SystemCode) -> Result<Vec<Member>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, asset_tag, status, isms_id FROM asset
         WHERE system_code = ?1 ORDER BY asset_tag COLLATE NOCASE, id",
    )?;
    let rows = stmt.query_map(params![code.as_str()], |r| {
        Ok(Member {
            id: r.get(0)?,
            tag: r.get(1)?,
            status: AssetStatus::parse(&r.get::<_, String>(2)?).unwrap_or(AssetStatus::Active),
            isms_id: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// 依 19 design D3 計算每個資產的變更。資產編號與舊 ISMS 編號相同（NFKC、不分大小寫）時，
/// 新資產編號為新 ISMS 編號（spec Changing a system code）。
fn recode_member(m: Member, old: &SystemCode, new: &SystemCode) -> RecodedAsset {
    // 系統代碼不是原代碼的 ISMS 編號（16 之前的舊資料）不更動
    let isms_id = m
        .isms_id
        .and_then(|id| recode(&id, old, new).map(|next| (id, next)));
    let new_tag = isms_id
        .as_ref()
        .filter(|(id, _)| name_key(id) == name_key(&m.tag))
        .map(|(_, next)| next.clone());
    RecodedAsset {
        id: m.id,
        asset_tag: m.tag,
        status: m.status,
        isms_id,
        new_tag,
    }
}

/// 同一批中重複的新值（不分 ASCII 大小寫，與 `COLLATE NOCASE` 相同）。
fn duplicates<'a>(values: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for v in values {
        let n = seen.entry(v.to_ascii_lowercase()).or_default();
        *n += 1;
        if *n == 2 {
            out.push(v);
        }
    }
    out
}

/// 新資產編號或 ISMS 編號已被其他資產（含 retired、不分大小寫）使用，或同一批中重複的錯誤
/// （全部收集）。
fn conflicts(conn: &Connection, assets: &[RecodedAsset]) -> Result<Vec<SystemError>, CoreError> {
    let repo = AssetRepo::new(conn);
    let code_error = |message: String| SystemError::new(SystemField::Code, message);
    let mut errors = Vec::new();
    for a in assets {
        if let Some(tag) = &a.new_tag {
            if let Some((_, status)) = tag_in_use(conn, tag, Some(a.id))? {
                errors.push(code_error(tag_in_use_message(tag, status)));
            }
        }
        if let Some((_, isms)) = &a.isms_id {
            if let Some((owner, status)) = repo.isms_id_in_use(isms, Some(a.id))? {
                let retired = status == AssetStatus::Retired;
                errors.push(code_error(isms_in_use_message(isms, &owner, retired)));
            }
        }
    }
    let tags = duplicates(assets.iter().filter_map(|a| a.new_tag.as_deref()));
    let isms = duplicates(
        assets
            .iter()
            .filter_map(|a| a.isms_id.as_ref().map(|i| i.1.as_str())),
    );
    errors.extend(
        tags.into_iter()
            .map(|t| code_error(format!("資產編號 {t} 在這次修改中重複"))),
    );
    errors.extend(
        isms.into_iter()
            .map(|i| code_error(format!("ISMS 編號 {i} 在這次修改中重複"))),
    );
    Ok(errors)
}

/// 預覽修改（只讀）。驗證失敗或新編號衝突時回傳 [`SystemEditError::Fields`]。
pub fn plan_system_edit(
    conn: &Connection,
    edit: &SystemEdit,
) -> Result<SystemEditPlan, SystemEditError> {
    let dir = SystemDirectory::load(conn)?;
    let after = validate_edit(edit, &dir).map_err(SystemEditError::Fields)?;
    let before = dir
        .get(&edit.code)
        .cloned()
        .ok_or_else(|| CoreError::Invalid(format!("資產說明 {} 不存在", edit.code)))?;
    if before.code == after.code {
        return Ok(SystemEditPlan {
            before,
            after,
            assets: Vec::new(),
        });
    }
    let assets: Vec<RecodedAsset> = members(conn, &before.code)?
        .into_iter()
        .map(|m| recode_member(m, &before.code, &after.code))
        .collect();
    let errors = conflicts(conn, &assets)?;
    if !errors.is_empty() {
        return Err(SystemEditError::Fields(errors));
    }
    Ok(SystemEditPlan {
        before,
        after,
        assets,
    })
}

/// 套用計畫（呼叫端在交易內）。代碼改變時延後外鍵檢查到 commit（19 design D4）。
fn apply(conn: &Connection, plan: &SystemEditPlan) -> Result<(), CoreError> {
    if !plan.code_changed() {
        return SystemRepo::new(conn).upsert(&plan.after);
    }
    let (old, new) = (plan.before.code.as_str(), plan.after.code.as_str());
    conn.pragma_update(None, "defer_foreign_keys", "ON")?;
    conn.execute(
        "UPDATE asset_system SET code = ?2, name = ?3 WHERE code = ?1",
        params![old, new, plan.after.name],
    )?;
    conn.execute(
        "UPDATE asset SET system_code = ?2 WHERE system_code = ?1",
        params![old, new],
    )?;
    let repo = AssetRepo::new(conn);
    for a in &plan.assets {
        if let Some((_, isms)) = &a.isms_id {
            repo.set_isms_id(a.id, isms)?;
        }
        if let Some(tag) = &a.new_tag {
            repo.rename(a.id, tag)?;
        }
    }
    Ok(())
}

/// 修改資產說明。交易內重新計算計畫並套用，回傳實際套用的計畫；任一步驟失敗時資料庫不變。
/// `confirmed` 為使用者確認過的計畫：重算結果不同時不寫入，回傳
/// [`SystemEditError::Changed`]（附新計畫）。
pub fn edit_system(
    conn: &mut Connection,
    edit: &SystemEdit,
    confirmed: Option<&SystemEditPlan>,
) -> Result<SystemEditPlan, SystemEditError> {
    let tx = conn.transaction()?;
    let plan = plan_system_edit(&tx, edit)?;
    if confirmed.is_some_and(|c| *c != plan) {
        return Err(SystemEditError::Changed(Box::new(plan)));
    }
    apply(&tx, &plan)?;
    tx.commit()?;
    log_edit(&plan);
    Ok(plan)
}

/// 提交後才記錄（交易回復時不留下沒有發生的變更）。
fn log_edit(plan: &SystemEditPlan) {
    if !plan.code_changed() {
        tracing::info!(code = %plan.after.code, "修改資產說明名稱");
        return;
    }
    tracing::info!(
        from = %plan.before.code,
        to = %plan.after.code,
        assets = plan.assets.len(),
        "修改資產說明代碼"
    );
    for (from, to) in plan.tag_changes() {
        tracing::info!(from = %from, to = %to, "資產更名");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recode_keeps_the_rest_of_the_id() {
        let net = SystemCode::parse("NET").unwrap();
        let nws = SystemCode::parse("NWS").unwrap();
        let r = |id: &str| recode(id, &net, &nws);
        assert_eq!(r("NET-hw-001").as_deref(), Some("NWS-hw-001"));
        assert_eq!(r("net-HW-001").as_deref(), Some("NWS-HW-001"));
        assert_eq!(r("ＮＥＴ-HW-001").as_deref(), Some("NWS-HW-001"));
        assert_eq!(r("SYS-HW-001"), None);
        assert_eq!(r("NETX-HW-001"), None, "系統代碼是第一個 - 之前的段落");
    }

    #[test]
    fn duplicates_ignore_ascii_case() {
        let values = ["NWS-HW-001", "nws-hw-001", "NWS-HW-002", "NWS-HW-001"];
        assert_eq!(duplicates(values.into_iter()), ["nws-hw-001"]);
    }
}
