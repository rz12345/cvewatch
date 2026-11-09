//! 資產說明的新增、改名與刪除（16 spec GUI asset system management、design D5）。
//!
//! 寫入前以畫面快照與 core 相同的規則檢查，錯誤以 [`SystemError`] 依欄位回報（對話框標在
//! 欄位旁）；core 仍會再檢查一次。資產說明不影響比對，寫入後只重新載入。

use cvewatch_core::assets::systems::{self, SystemError, SystemField};
use cvewatch_core::assets::SystemCode;

use super::reload::Refreshed;
use super::Workspace;
use crate::findings::ListView;
use crate::report::Report;

/// 寫入失敗的結果：欄位錯誤（對話框保留）或整體錯誤。
#[derive(Debug, Clone)]
pub enum SystemSaveError {
    Fields(Vec<SystemError>),
    Failed(Report),
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

    /// 修改資產說明名稱。
    pub fn rename_system(
        &mut self,
        code: &SystemCode,
        name: &str,
        open: &[i64],
    ) -> Result<Refreshed, SystemSaveError> {
        let system = systems::validate_rename(code, name, &self.data.systems)
            .map_err(SystemSaveError::Fields)?;
        systems::rename_system(&self.conn, code, name)
            .map_err(|e| SystemSaveError::Failed(Report::failure("修改資產說明失敗", &e)))?;
        Ok(self.after_write(format!("已將資產說明改為 {}", system.label()), open))
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
