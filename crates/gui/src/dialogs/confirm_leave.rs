//! 未儲存變更確認（離開設定畫面、關閉視窗）：儲存／放棄／取消（design D9）。

use eframe::egui;

use crate::app::{GuiApp, Leave};
use crate::dialogs::Dialog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Save,
    Discard,
    Cancel,
}

impl GuiApp {
    /// 視窗關閉要求：有未儲存的變更時取消關閉並詢問。
    pub fn handle_close_request(&mut self, ctx: &egui::Context) {
        let requested = ctx.input(|i| i.viewport().close_requested());
        if !requested || self.allow_close || !self.has_unsaved() {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        self.leave = Some(Leave::Quit);
    }

    pub(crate) fn show_confirm_leave(&mut self, ctx: &egui::Context) {
        let Some(then) = self.leave else {
            return;
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("dialog-confirm-leave")).show(ctx, |ui| {
            ui.heading("有未儲存的變更");
            ui.label(self.unsaved_summary());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("儲存").clicked() {
                    choice = Some(Choice::Save);
                }
                if ui.button("放棄").clicked() {
                    choice = Some(Choice::Discard);
                }
                if ui.button("取消").clicked() {
                    choice = Some(Choice::Cancel);
                }
            });
        });
        if choice.is_none() && super::escape_pressed(ctx) {
            choice = Some(Choice::Cancel);
        }
        match choice {
            None => {}
            Some(Choice::Cancel) => self.leave = None,
            Some(Choice::Discard) => {
                self.leave = None;
                self.discard_unsaved();
                self.proceed(ctx, then);
            }
            Some(Choice::Save) => {
                self.leave = None;
                if self.save_unsaved() {
                    self.proceed(ctx, then);
                }
            }
        }
    }

    fn unsaved_summary(&self) -> String {
        let mut parts = Vec::new();
        if self.settings.dirty() {
            parts.push("設定有未儲存的變更");
        }
        if self.dialog.as_ref().is_some_and(Dialog::has_unsaved_note) {
            parts.push("備註尚未送出");
        }
        format!("{}。要儲存嗎？", parts.join("，"))
    }

    fn discard_unsaved(&mut self) {
        if self.settings.dirty() {
            self.settings.discard();
            self.report(cvewatch_frontend::Report::info("已放棄未儲存的設定變更"));
        }
        if self.dialog.as_ref().is_some_and(Dialog::has_unsaved_note) {
            self.dialog = None;
        }
    }

    /// 儲存設定並送出備註；任何一項失敗時回傳 false（不離開，備註對話框保留）。
    fn save_unsaved(&mut self) -> bool {
        if self.settings.dirty() && !self.save_settings() {
            return false;
        }
        match self.dialog.take() {
            Some(Dialog::Note { purpose, text, .. }) if !text.trim().is_empty() => {
                if self.submit(purpose.clone(), text.clone()) {
                    true
                } else {
                    self.dialog = Some(Dialog::Note {
                        purpose,
                        text,
                        focused: true,
                    });
                    false
                }
            }
            other => {
                self.dialog = other;
                true
            }
        }
    }

    fn proceed(&mut self, ctx: &egui::Context, then: Leave) {
        match then {
            Leave::Screen(s) => self.enter(s),
            Leave::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
