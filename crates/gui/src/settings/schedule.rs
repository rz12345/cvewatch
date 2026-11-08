//! 排程與門檻分頁：同步間隔、摘要時間（HH:MM）、緊急門檻（0–10）、摘要無新事項仍寄出；
//! 儲存後排程有變更時提示重新安裝排程。

use cvewatch_frontend::settings::form::Errors;
use cvewatch_frontend::settings::{FieldId, JobKind};
use eframe::egui;

use super::{text_field, toggle_field};
use crate::app::{GuiApp, NO_COMPANION};

/// 排程變更的提示。
pub const SCHEDULE_CHANGED: &str = "同步間隔或摘要時間已變更，需重新安裝排程才會生效";

impl GuiApp {
    pub(super) fn show_schedule(&mut self, ui: &mut egui::Ui, errors: &Errors) {
        egui::Grid::new("settings-schedule")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                let page = &mut self.settings;
                text_field(ui, page, FieldId::SyncHours, errors);
                text_field(ui, page, FieldId::DigestAt, errors);
                text_field(ui, page, FieldId::UrgentCvss, errors);
                toggle_field(ui, page, FieldId::SendEmpty);
            });
        ui.add_space(8.0);
        if self.settings.schedule_pending {
            ui.colored_label(egui::Color32::from_rgb(0xc0, 0x80, 0x00), SCHEDULE_CHANGED);
        }
        let running = self.settings.running(JobKind::InstallSchedule);
        let enabled = self.companion.is_some() && !running;
        let button = ui
            .add_enabled(enabled, egui::Button::new("重新安裝排程"))
            .on_disabled_hover_text(if running {
                "重新安裝排程中…"
            } else {
                NO_COMPANION
            });
        if button.clicked() {
            self.reinstall_schedule();
        }
    }
}
