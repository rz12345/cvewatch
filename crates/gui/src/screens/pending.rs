//! 等待區：原因、進入天數、逾期、KEV 置頂（順序由 core 決定）；套用到資產或標為不相關，
//! 連結與複製 CVE ID。

use chrono::{DateTime, Utc};
use cvewatch_frontend::display::pending_days;
use cvewatch_frontend::links::Link;
use eframe::egui;

use super::{heading, ALERT};
use crate::app::GuiApp;
use crate::text::one_line;

enum Action {
    Apply(String),
    Dismiss(String),
    Open(String),
    Copy(String),
}

/// 選取中 CVE 的連結（選取改變或重新載入後才重新查詢資料庫，不在每一幀查詢）。
pub struct PendingLinks {
    cve: String,
    loaded_at: DateTime<Utc>,
    links: Result<Vec<Link>, String>,
}

impl GuiApp {
    pub(super) fn show_pending(&mut self, ui: &mut egui::Ui) {
        if self.ws.data.pending.is_empty() {
            ui.label("等待區沒有 CVE。");
            return;
        }
        // 與 TUI 相同：天數以資料載入時間計算
        let now = self.ws.data.loaded_at;
        let mut picked = None;
        egui::ScrollArea::vertical()
            .id_salt("pending-list")
            .max_height(ui.available_height() * 0.55)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("pending-grid")
                    .num_columns(5)
                    .striped(true)
                    .spacing([14.0, 4.0])
                    .show(ui, |ui| {
                        for h in ["CVE", "原因", "天數", "KEV／逾期", "標題"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for p in &self.ws.data.pending {
                            let on = self.pending_selected.as_deref() == Some(p.cve_id.as_str());
                            if ui.selectable_label(on, &p.cve_id).clicked() {
                                picked = Some(p.cve_id.clone());
                            }
                            ui.label(p.reason.label());
                            ui.label(
                                pending_days(p, now).map_or("-".into(), |d| format!("{d} 天")),
                            );
                            let mut flags = Vec::new();
                            if let Some(k) = &p.kev_date {
                                flags.push(format!("KEV {k}"));
                            }
                            if p.stale {
                                flags.push("逾期".into());
                            }
                            if flags.is_empty() {
                                ui.label("");
                            } else {
                                ui.colored_label(ALERT, flags.join("、"));
                            }
                            ui.label(p.title.as_deref().map(one_line).unwrap_or_default());
                            ui.end_row();
                        }
                    });
            });
        if picked.is_some() {
            self.pending_selected = picked;
        }
        ui.separator();
        let action = self.pending_actions(ui);
        match action {
            Some(Action::Apply(cve)) => self.start_pending_apply(cve),
            Some(Action::Dismiss(cve)) => self.start_pending_dismiss(cve),
            Some(Action::Open(url)) => self.open_link(&url),
            Some(Action::Copy(cve)) => self.copy(&cve),
            None => {}
        }
    }

    fn pending_actions(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let Some(cve) = self.pending_selected.clone() else {
            ui.label("請選取一筆 CVE。");
            return None;
        };
        if !self.ws.data.pending.iter().any(|p| p.cve_id == cve) {
            self.pending_selected = None;
            return None;
        }
        let mut action = None;
        heading(ui, &cve);
        ui.horizontal(|ui| {
            if ui.button("套用到資產").clicked() {
                action = Some(Action::Apply(cve.clone()));
            }
            if ui.button("標為不相關").clicked() {
                action = Some(Action::Dismiss(cve.clone()));
            }
            if ui.button("複製 CVE ID").clicked() {
                action = Some(Action::Copy(cve.clone()));
            }
        });
        match &self.links_for(&cve).links {
            Ok(links) => {
                ui.horizontal_wrapped(|ui| {
                    for l in links.iter() {
                        if ui.link(&l.label).on_hover_text(&l.url).clicked() {
                            action = Some(Action::Open(l.url.clone()));
                        }
                    }
                });
            }
            Err(e) => {
                ui.label(e);
            }
        }
        action
    }

    fn links_for(&mut self, cve: &str) -> &PendingLinks {
        let loaded_at = self.ws.data.loaded_at;
        let fresh = self
            .pending_links
            .as_ref()
            .is_some_and(|l| l.cve == cve && l.loaded_at == loaded_at);
        if !fresh {
            self.pending_links = Some(PendingLinks {
                cve: cve.to_string(),
                loaded_at,
                links: self.ws.cve_links(cve).map_err(|r| r.message),
            });
        }
        self.pending_links.as_ref().expect("cached links")
    }
}
