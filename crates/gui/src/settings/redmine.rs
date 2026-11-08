//! Redmine 分頁：網址（必須 https）、API key、專案／tracker／優先權／指派對象（載入清單後以
//! 「名稱 (#ID)」挑選，無法載入時改為數字輸入）、狀態變更註記。

use cvewatch_frontend::settings::form::Errors;
use cvewatch_frontend::settings::{FieldId, JobKind};
use eframe::egui;

use super::{error_row, secret_field, text_field, toggle_field, ERROR_COLOR};
use crate::app::GuiApp;

const PICK_FIELDS: [FieldId; 5] = [
    FieldId::Project,
    FieldId::Tracker,
    FieldId::PriorityP1,
    FieldId::PriorityP2,
    FieldId::Assignee,
];

impl GuiApp {
    pub(super) fn show_redmine(&mut self, ui: &mut egui::Ui, errors: &Errors) {
        let mut picked = None;
        egui::Grid::new("settings-redmine")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                let page = &mut self.settings;
                toggle_field(ui, page, FieldId::RedmineEnabled);
                text_field(ui, page, FieldId::BaseUrl, errors);
                secret_field(ui, page, FieldId::ApiKey);
                for f in PICK_FIELDS {
                    if let Some(id) = self.pick_field(ui, f, errors) {
                        picked = Some((f, id));
                    }
                }
                toggle_field(ui, &mut self.settings, FieldId::NoteOnTriage);
            });
        match picked {
            Some((FieldId::Project, id)) => self.project_picked(id),
            Some((f, id)) => self.settings.set_text(f, &id.to_string()),
            None => {}
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let running = self.settings.running(JobKind::LoadLists)
                || self.settings.running(JobKind::LoadProject);
            if ui
                .add_enabled(!running, egui::Button::new("從 Redmine 載入"))
                .clicked()
            {
                self.load_lists(None);
            }
            if running {
                ui.label("載入中…");
            }
        });
        if let Some(e) = &self.settings.lists_error {
            ui.colored_label(
                ERROR_COLOR,
                format!("無法載入 Redmine 清單：{e}；請直接輸入數字 ID"),
            );
        }
    }

    /// 挑選欄位：有清單時為下拉選單（回傳選取的 ID），否則為數字輸入。
    fn pick_field(&mut self, ui: &mut egui::Ui, f: FieldId, errors: &Errors) -> Option<u64> {
        // 與 TUI 相同：有清單（且非空）時挑選，否則手動輸入數字 ID
        let Some(items) = self.settings.pick_items(f).filter(|i| !i.is_empty()) else {
            text_field(ui, &mut self.settings, f, errors);
            return None;
        };
        let label = ui.label(f.label());
        let current = self.settings.form.text(f).trim().parse::<u64>().ok();
        let mut picked = None;
        let combo = egui::ComboBox::from_id_salt(("pick", f.label()))
            .selected_text(self.settings.pick_label(f))
            .width(280.0)
            .show_ui(ui, |ui| {
                for item in &items {
                    let selected = current == Some(item.id);
                    if ui.selectable_label(selected, item.label()).clicked() && !selected {
                        picked = Some(item.id);
                    }
                }
            });
        combo.response.labelled_by(label.id);
        ui.end_row();
        error_row(ui, f, errors);
        picked
    }
}
