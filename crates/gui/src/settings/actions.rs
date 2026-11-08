//! 設定的動作（邏輯與說明文字在 `cvewatch_frontend::settings::SettingsState`，與 TUI 共用）：
//! 儲存、測試寄信／測試 Redmine（表單目前值）、載入 Redmine 清單、重新安裝排程、取回背景工作結果。

use cvewatch_frontend::settings::{ActionWords, FieldId, Finished, JobKind, Refused, Saved, Tab};
use cvewatch_frontend::Report;

use crate::app::{GuiApp, NO_COMPANION};

/// 視窗版的操作方式（插入共用的說明文字）。
const WORDS: ActionWords = ActionWords {
    save: "按「儲存」",
    reinstall: "請按「重新安裝排程」",
    pick: "可由下拉選單挑選",
};

impl GuiApp {
    /// 驗證 → 寫入憑證庫 → 寫回設定檔。成功（含沒有變更）時回傳 true。
    pub fn save_settings(&mut self) -> bool {
        let ws = &mut self.ws;
        let outcome = self
            .settings
            .state
            .save(&ws.services, &mut ws.config, &ws.config_path);
        match outcome {
            Saved::Done { .. } => self.settings.reset_buffers(),
            // 秘密已寫入並成為比較基準：清空秘密輸入
            Saved::ConfigFailed(_) => self.settings.clear_secret_buffers(),
            _ => {}
        }
        let text = outcome.message(&WORDS);
        self.report(if outcome.is_error() {
            Report::error(text)
        } else if outcome.ok() {
            Report::info(text)
        } else {
            Report::warning(text)
        });
        outcome.ok()
    }

    fn refused(&mut self, r: Refused) {
        if let Some(text) = r.message(&WORDS) {
            self.report(if r.is_error() {
                Report::error(text)
            } else {
                Report::warning(text)
            });
        }
    }

    /// 以目前（含未儲存）的表單值測試；不建立議題。
    pub fn start_test(&mut self, kind: JobKind) {
        let ws = &self.ws;
        match self
            .settings
            .state
            .start_test(&ws.services, &ws.config, kind)
        {
            Ok(()) => self.report(Report::info(kind.started())),
            Err(r) => self.refused(r),
        }
    }

    /// 載入專案與優先權（`project` 有值時載入該專案的 tracker 與成員）。
    pub fn load_lists(&mut self, project: Option<String>) {
        let ws = &self.ws;
        match self
            .settings
            .state
            .load_lists(&ws.services, &ws.config, project)
        {
            Ok(()) => self.report(Report::info(JobKind::LoadLists.started())),
            Err(r) => self.refused(r),
        }
    }

    /// 以已儲存的設定重新安裝排程（經 companion）。
    pub fn reinstall_schedule(&mut self) {
        if self.companion.is_none() {
            return self.report(Report::warning(NO_COMPANION));
        }
        match self.settings.state.reinstall_schedule(&self.ws.services) {
            Ok(()) => self.report(Report::info(JobKind::InstallSchedule.started())),
            Err(r) => self.refused(r),
        }
    }

    /// 取回已完成的背景工作。
    pub(crate) fn poll_settings_jobs(&mut self) {
        for f in self.settings.state.poll() {
            let text = f.message(&WORDS);
            self.report(if f.is_error() {
                Report::error(text)
            } else {
                Report::info(text)
            });
            if let Finished::Lists { follow_up } = f {
                if follow_up.is_some() {
                    self.load_lists(follow_up);
                }
            }
        }
    }

    /// 換專案：tracker 與成員清單需重新載入。
    pub fn project_picked(&mut self, id: u64) {
        self.settings.set_text(FieldId::Project, &id.to_string());
        self.settings.lists.trackers = None;
        self.settings.lists.members = None;
        if self.settings.tab == Tab::Redmine {
            self.load_lists(Some(id.to_string()));
        }
    }
}
