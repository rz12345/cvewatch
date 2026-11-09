//! 資產說明的新增、編輯（代碼與名稱）與刪除（16 spec GUI asset system management、design D5；
//! 19 design D5）。
//!
//! 新增時先以畫面快照與 core 相同的規則檢查；編輯（預覽與寫入）直接由 core 對照資料庫檢查
//! （19 code review：快照可能過時），寫入時在交易內重新計算計畫。錯誤以 [`SystemError`] 依欄位
//! 回報（對話框標在欄位旁）。資產說明不影響比對，寫入後只重新載入。

use cvewatch_core::assets::systems::{self, SystemError, SystemField};
use cvewatch_core::assets::systems_edit::{self, SystemEdit, SystemEditError, SystemEditPlan};
use cvewatch_core::assets::SystemCode;

use super::reload::Refreshed;
use super::Workspace;
use crate::findings::ListView;
use crate::report::Report;

/// 修改資產說明代碼後的提醒（確認畫面與 CLI 共用，19 design D5）。
pub const RECODE_NOTE: &str = "舊清冊、已匯出的 VEX 與尚未更新的 ISMS 清冊中的舊代碼與舊編號不會對應；儲存後建議重新匯出清冊（cvewatch asset export），並更新 ISMS 清冊";

/// 寫入失敗的結果：欄位錯誤（對話框保留）、確認後受影響的資產有變動（附新計畫，重新確認）
/// 或整體錯誤。
#[derive(Debug, Clone)]
pub enum SystemSaveError {
    Fields(Vec<SystemError>),
    Changed(Box<SystemEditPlan>),
    Failed(Report),
}

impl SystemSaveError {
    fn from_edit(what: &str, e: SystemEditError) -> SystemSaveError {
        match e {
            SystemEditError::Fields(errors) => SystemSaveError::Fields(errors),
            SystemEditError::Changed(plan) => SystemSaveError::Changed(plan),
            SystemEditError::Failed(e) => SystemSaveError::Failed(Report::failure(what, &e)),
        }
    }
}

/// 編輯資產說明的結果：重新載入與被改了編號的資產（舊, 新），供畫面更新選取。
#[derive(Debug, Clone)]
#[must_use = "需要顯示結果並更新開啟中的詳情"]
pub struct SystemEdited {
    pub refreshed: Refreshed,
    pub renamed: Vec<(String, String)>,
}

impl<V: ListView> Workspace<V> {
    /// 歸屬某資產說明的資產數（含已退役）。
    pub fn system_usage(&self, code: &SystemCode) -> u32 {
        self.data.system_usage.get(code).copied().unwrap_or(0)
    }

    /// 新增資產說明。
    pub fn create_system(
        &mut self,
        code: &str,
        name: &str,
        open: &[i64],
    ) -> Result<Refreshed, SystemSaveError> {
        let system = systems::validate_new(code, name, &self.data.systems)
            .map_err(SystemSaveError::Fields)?;
        systems::create_system(&self.conn, code, name)
            .map_err(|e| SystemSaveError::Failed(Report::failure("新增資產說明失敗", &e)))?;
        Ok(self.after_write(format!("已新增資產說明 {}", system.label()), open))
    }

    /// 預覽編輯（不寫入）：由 core 對照資料庫檢查欄位，並計算受影響的資產與新編號衝突。
    pub fn preview_system_edit(
        &self,
        edit: &SystemEdit,
    ) -> Result<SystemEditPlan, SystemSaveError> {
        systems_edit::plan_system_edit(&self.conn, edit)
            .map_err(|e| SystemSaveError::from_edit("預覽資產說明修改失敗", e))
    }

    /// 編輯資產說明（代碼與名稱）；代碼改變時連動歸屬資產的 ISMS 編號與資產編號。
    /// `confirmed` 為使用者確認過的計畫：交易內重算結果不同時不寫入（[`SystemSaveError::Changed`]）。
    pub fn edit_system(
        &mut self,
        edit: &SystemEdit,
        confirmed: Option<&SystemEditPlan>,
        open: &[i64],
    ) -> Result<SystemEdited, SystemSaveError> {
        let plan = systems_edit::edit_system(&mut self.conn, edit, confirmed)
            .map_err(|e| SystemSaveError::from_edit("修改資產說明失敗", e))?;
        let renamed: Vec<(String, String)> = plan
            .tag_changes()
            .into_iter()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect();
        let refreshed = self.after_write(edited_message(&plan), open);
        Ok(SystemEdited { refreshed, renamed })
    }

    /// 刪除資產說明；仍有資產歸屬時不寫入並回報原因。
    pub fn delete_system(&mut self, code: &SystemCode, open: &[i64]) -> Refreshed {
        let used = self.system_usage(code);
        if used > 0 {
            return Refreshed::unchanged(Report::error(delete_blocked(used)), open);
        }
        match systems::delete_system(&self.conn, code) {
            Ok(()) => self.after_write(format!("已刪除資產說明 {code}"), open),
            Err(e) => Refreshed::unchanged(Report::failure("刪除資產說明失敗", &e), open),
        }
    }
}

/// 狀態列訊息：只改名「已將資產說明改為 NET 校園網路服務」；改代碼「已將資產說明代碼 NET
/// 改為 NWS（3 個資產；ISMS 編號 2 筆、資產編號 1 筆變更）」。
pub fn edited_message(plan: &SystemEditPlan) -> String {
    if !plan.code_changed() {
        return format!("已將資產說明改為 {}", plan.after.label());
    }
    let head = format!(
        "已將資產說明代碼 {} 改為 {}",
        plan.before.code, plan.after.code
    );
    if plan.assets.is_empty() {
        return head;
    }
    format!(
        "{head}（{} 個資產；ISMS 編號 {} 筆、資產編號 {} 筆變更）",
        plan.assets.len(),
        plan.isms_changes().len(),
        plan.tag_changes().len()
    )
}

/// 「受影響的資產：3 個（含已退役 1 個）」（確認畫面與 CLI 共用）。
pub fn affected_text(plan: &SystemEditPlan) -> String {
    let retired = plan.retired();
    if retired == 0 {
        format!("受影響的資產：{} 個", plan.assets.len())
    } else {
        format!(
            "受影響的資產：{} 個（含已退役 {retired} 個）",
            plan.assets.len()
        )
    }
}

/// 「刪除」停用時的說明。
pub fn delete_blocked(used: u32) -> String {
    format!("仍有 {used} 個資產歸屬此資產說明，無法刪除")
}

/// 欄位的錯誤訊息（對話框標在欄位旁）。
pub fn field_error(errors: &[SystemError], field: SystemField) -> Option<&str> {
    errors
        .iter()
        .find(|e| e.field == field)
        .map(|e| e.message.as_str())
}

/// 欄位的全部錯誤訊息（資產編號衝突可能有多筆，以換行連接）。
pub fn field_errors(errors: &[SystemError], field: SystemField) -> Option<String> {
    let lines: Vec<&str> = errors
        .iter()
        .filter(|e| e.field == field)
        .map(|e| e.message.as_str())
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}
