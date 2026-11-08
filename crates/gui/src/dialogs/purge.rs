//! 「刪除」（標為退役）確認與「永久刪除」確認（design D11）。
//!
//! 永久刪除先顯示將一併移除的元件數與命中數、建議先備份，輸入框內容與 `asset_tag` 完全相同
//! （區分大小寫、不去除空白）才啟用確認按鈕。

use cvewatch_core::assets::edit::PurgePreview;
use eframe::egui;

use super::{escape_pressed, AssetDialog};
use crate::app::GuiApp;
use crate::screens::ALERT;
use crate::text::one_line;

/// 退役的說明（保留紀錄、不再比對、可恢復）。
pub const RETIRE_NOTE: &str =
    "標為退役後：命中與處理紀錄都保留，不再參與比對；之後可在「顯示已退役」中恢復。";

/// 永久刪除前的備份建議。
pub const BACKUP_NOTE: &str = "此操作無法復原，建議先執行 `cvewatch db backup` 備份資料庫。";

/// 永久刪除確認的狀態。
pub struct PurgeConfirm {
    pub tag: String,
    pub preview: PurgePreview,
    /// 使用者輸入的資產編號
    pub typed: String,
}

impl PurgeConfirm {
    /// 輸入與資產編號完全相同。
    pub fn confirmed(&self) -> bool {
        self.typed == self.tag
    }
}

enum Choice {
    Confirm,
    Cancel,
}

impl GuiApp {
    /// 「刪除」：先確認。
    pub fn start_retire_asset(&mut self, tag: &str) {
        self.asset_dialog = Some(AssetDialog::Retire {
            tag: tag.to_string(),
        });
    }

    pub fn restore_asset(&mut self, tag: &str) {
        let open = self.open_ids();
        let r = self.ws.restore_asset(tag, &open);
        self.after_asset_write(r);
    }

    /// 「永久刪除」：讀取預覽後開啟確認。
    pub fn start_purge_asset(&mut self, tag: &str) {
        match self.ws.purge_preview(tag) {
            Ok(preview) => {
                self.asset_dialog = Some(AssetDialog::Purge(PurgeConfirm {
                    tag: tag.to_string(),
                    preview,
                    typed: String::new(),
                }))
            }
            Err(r) => self.report(r),
        }
    }

    pub(super) fn show_retire_confirm(&mut self, ctx: &egui::Context, tag: String) {
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("dialog-retire")).show(ctx, |ui| {
            ui.heading(format!("刪除 {}", one_line(&tag)));
            ui.label(RETIRE_NOTE);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("標為退役").clicked() {
                    choice = Some(Choice::Confirm);
                }
                if ui.button("取消").clicked() {
                    choice = Some(Choice::Cancel);
                }
            });
        });
        if choice.is_none() && escape_pressed(ctx) {
            choice = Some(Choice::Cancel);
        }
        match choice {
            None => self.asset_dialog = Some(AssetDialog::Retire { tag }),
            Some(Choice::Cancel) => {}
            Some(Choice::Confirm) => {
                let open = self.open_ids();
                let r = self.ws.retire_asset(&tag, &open);
                self.after_asset_write(r);
            }
        }
    }

    pub(super) fn show_purge_confirm(&mut self, ctx: &egui::Context, mut p: PurgeConfirm) {
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("dialog-purge")).show(ctx, |ui| {
            ui.heading(format!("永久刪除 {}", one_line(&p.tag)));
            ui.label(format!(
                "將一併移除元件 {} 個、命中 {} 筆與其處理紀錄、版本歷史與未辨識產品。",
                p.preview.components, p.preview.findings
            ));
            ui.colored_label(ALERT, BACKUP_NOTE);
            ui.add_space(6.0);
            let l = ui.label("輸入資產編號確認");
            ui.add(egui::TextEdit::singleline(&mut p.typed).desired_width(220.0))
                .labelled_by(l.id);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let ok = p.confirmed();
                if ui
                    .add_enabled(ok, egui::Button::new("永久刪除"))
                    .on_disabled_hover_text("請輸入完全相同的資產編號")
                    .clicked()
                {
                    choice = Some(Choice::Confirm);
                }
                if ui.button("取消").clicked() {
                    choice = Some(Choice::Cancel);
                }
            });
        });
        if choice.is_none() && escape_pressed(ctx) {
            choice = Some(Choice::Cancel);
        }
        match choice {
            None => self.asset_dialog = Some(AssetDialog::Purge(p)),
            Some(Choice::Cancel) => {}
            Some(Choice::Confirm) => {
                let open = self.open_ids();
                let r = self.ws.purge_asset(&p.tag, &open);
                if r.report.level != cvewatch_frontend::Level::Error {
                    self.assets_selected = None;
                }
                self.after_asset_write(r);
            }
        }
    }
}
