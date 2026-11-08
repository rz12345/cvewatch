//! 命中詳情面板（段落文字由 `cvewatch_frontend::display` 產生，與 TUI 相同）：CVE 描述、
//! 各來源 CVSS／EPSS／KEV、證據與版本區間、來源矛盾、原廠修正版本、時間軸、連結（只允許 https）
//! 與複製 CVE ID。

use eframe::egui;

use super::heading;
use crate::app::{GuiApp, OpenDetail};

/// 詳情面板的操作。
enum Action {
    Open(String),
    Copy(String),
    Close,
}

impl GuiApp {
    /// 右側詳情面板（命中畫面且有開啟中的詳情時）。
    pub fn show_detail_panel(&mut self, ui: &mut egui::Ui) {
        let Some(detail) = &self.detail else {
            return;
        };
        let mut action = None;
        egui::Panel::right("finding-detail")
            .resizable(true)
            .default_size(480.0)
            .show(ui, |ui| {
                action = detail_body(ui, detail);
            });
        match action {
            Some(Action::Open(url)) => self.open_link(&url),
            Some(Action::Copy(cve)) => self.copy(&cve),
            Some(Action::Close) => self.detail = None,
            None => {}
        }
    }
}

fn detail_body(ui: &mut egui::Ui, detail: &OpenDetail) -> Option<Action> {
    let data = &detail.data;
    let mut action = None;
    let cve = &data.finding.row.cve_id;
    ui.horizontal(|ui| {
        heading(ui, format!("命中 #{}", data.finding.row.id));
        if ui.button("複製 CVE ID").clicked() {
            action = Some(Action::Copy(cve.clone()));
        }
        if ui.button("關閉詳情").clicked() {
            action = Some(Action::Close);
        }
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("detail-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, s) in detail.sections.iter().enumerate() {
                if let Some(t) = s.title {
                    ui.add_space(6.0);
                    heading(ui, t);
                }
                for (j, line) in s.lines.iter().enumerate() {
                    let text = egui::RichText::new(crate::text::one_line(line));
                    let text = if i == 0 && j == 0 {
                        text.strong()
                    } else {
                        text
                    };
                    ui.add(egui::Label::new(text).wrap());
                }
            }
            ui.add_space(6.0);
            heading(ui, "連結");
            ui.horizontal_wrapped(|ui| {
                // 連結已由 Workspace 依連結規則（只允許 https）篩選
                for l in &data.links {
                    if ui.link(&l.label).on_hover_text(&l.url).clicked() {
                        action = Some(Action::Open(l.url.clone()));
                    }
                }
            });
        });
    action
}
