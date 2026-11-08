//! 同步紀錄：最近的同步（新到舊）、來源、結果、筆數、錯誤摘要。

use eframe::egui;

use super::{ALERT, OK};
use crate::app::GuiApp;
use crate::text::{full_time, one_line};

impl GuiApp {
    pub(super) fn show_sync_log(&mut self, ui: &mut egui::Ui) {
        let runs = &self.ws.data.sync_runs;
        if runs.is_empty() {
            ui.label("尚無同步紀錄。");
            return;
        }
        let tz = self.ws.services.tz;
        egui::ScrollArea::vertical()
            .id_salt("sync-log")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("sync-log-grid")
                    .num_columns(6)
                    .striped(true)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for h in ["開始時間", "來源", "結果", "新增", "更新", "錯誤"]
                        {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for r in runs {
                            ui.label(full_time(&r.started_at, tz));
                            ui.label(&r.source);
                            match r.status.as_str() {
                                "success" => ui.colored_label(OK, "成功"),
                                "failed" => ui.colored_label(ALERT, "失敗"),
                                "running" if r.finished_at.is_none() => ui.label("執行中"),
                                other => ui.label(other),
                            };
                            ui.label(r.added.to_string());
                            ui.label(r.updated.to_string());
                            ui.label(r.error.as_deref().map(one_line).unwrap_or_default());
                            ui.end_row();
                        }
                    });
            });
    }
}
