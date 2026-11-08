//! 六個畫面的內容（外框在 `shell`）。

pub mod assets;
pub mod dashboard;
pub mod detail;
pub mod findings;
pub mod pending;
pub mod sync_log;

use eframe::egui;

use crate::app::{GuiApp, Screen};

impl GuiApp {
    /// 目前畫面的內容（中央區域）。
    pub fn show_screen(&mut self, ui: &mut egui::Ui) {
        match self.screen {
            Screen::Dashboard => self.show_dashboard(ui),
            Screen::Findings => self.show_findings(ui),
            Screen::Assets => self.show_assets(ui),
            Screen::Pending => self.show_pending(ui),
            Screen::SyncLog => self.show_sync_log(ui),
            Screen::Settings => self.show_settings(ui),
        }
    }
}

/// 粗體標題。
pub fn heading(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text.into()).strong().size(15.0));
}

/// 失敗、逾期等需要注意的文字顏色。
pub const ALERT: egui::Color32 = egui::Color32::from_rgb(0xd0, 0x30, 0x30);
/// 成功的文字顏色。
pub const OK: egui::Color32 = egui::Color32::from_rgb(0x2e, 0x8b, 0x57);
