//! 外框：頂端警示列（明文傳輸、啟動警告、有新資料）、工具列（同步）、側邊導覽、狀態列、
//! 快捷鍵（design D10），以及每一幀的處理順序（design D3）。

use std::time::Duration;

use cvewatch_core::matching::triage::Status;
use cvewatch_frontend::workspace::NEW_DATA_POLL;
use cvewatch_frontend::Level;
use eframe::egui;

use crate::app::{GuiApp, Screen, NO_COMPANION};
use crate::text::report_text;

/// 有背景工作時的喚醒間隔。
pub const BUSY_POLL: Duration = Duration::from_millis(200);

/// 明文傳輸警示列文字。
pub const INSECURE_BANNER: &str =
    "明文傳輸已啟用（設定檔 allow_insecure_transport = true）：通知可能以未加密的連線送出";

const RED: egui::Color32 = egui::Color32::from_rgb(0xb0, 0x20, 0x20);
const YELLOW: egui::Color32 = egui::Color32::from_rgb(0xf0, 0xc0, 0x30);

impl GuiApp {
    /// 一幀：關閉要求與拖放 → 輪詢 → 快捷鍵 → 繪製 → 對話框。
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.handle_close_request(&ctx);
        self.handle_drops(&ctx);
        self.poll();
        self.handle_shortcuts(&ctx);
        self.top_bars(ui);
        self.status_bar(ui);
        self.navigation(ui);
        if self.screen == Screen::Findings {
            self.show_detail_panel(ui);
        }
        egui::CentralPanel::default().show(ui, |ui| self.show_screen(ui));
        self.show_dialogs(&ctx);
        ctx.request_repaint_after(if self.busy() {
            BUSY_POLL
        } else {
            NEW_DATA_POLL
        });
    }

    pub(crate) fn modal_open(&self) -> bool {
        self.dialog.is_some()
            || self.import.is_some()
            || self.leave.is_some()
            || self.asset_dialog.is_some()
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.modal_open() {
            return;
        }
        let ctrl = egui::Modifiers::COMMAND;
        for s in Screen::ALL {
            if ctx.input_mut(|i| i.consume_key(ctrl, s.key())) {
                self.go(s);
            }
        }
        if ctx.input_mut(|i| i.consume_key(ctrl, egui::Key::F)) {
            self.go(Screen::Findings);
            // 離開設定被未儲存確認攔下時不留下聚焦要求
            self.ws.findings.view.focus_search = self.screen == Screen::Findings;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            self.reload();
        }
        // 單鍵只在沒有文字欄位取得焦點時生效（避免輸入備註或搜尋時誤觸）
        if ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx) {
            return;
        }
        // 修飾鍵需完全相符（`consume_key` 會忽略多按的 Shift，`r` 與 `R` 必須分開）
        let pressed = |key: egui::Key, shift: bool| {
            ctx.input_mut(|i| {
                let m = i.modifiers;
                let exact = m.shift == shift && !m.ctrl && !m.command && !m.alt && !m.mac_cmd;
                exact && i.consume_key(m, key)
            })
        };
        if pressed(egui::Key::S, true) {
            self.start_sync();
        }
        if pressed(egui::Key::R, true) {
            self.reload();
        }
        match self.screen {
            Screen::Findings => self.findings_keys(&pressed),
            Screen::Pending => {
                if let Some(cve) = self.pending_selected.clone() {
                    if pressed(egui::Key::A, false) {
                        self.start_pending_apply(cve);
                    } else if pressed(egui::Key::D, false) {
                        self.start_pending_dismiss(cve);
                    } else if pressed(egui::Key::Y, false) {
                        self.copy(&cve);
                    }
                }
            }
            _ => {}
        }
    }

    fn findings_keys(&mut self, pressed: &dyn Fn(egui::Key, bool) -> bool) {
        if pressed(egui::Key::R, false) {
            self.start_triage(Status::Resolved);
        } else if pressed(egui::Key::I, false) {
            self.start_triage(Status::Ignored);
        } else if pressed(egui::Key::U, false) {
            self.start_triage(Status::Open);
        } else if pressed(egui::Key::Y, false) {
            self.copy_current_cve();
        } else if pressed(egui::Key::E, true) {
            self.export_vex();
        } else if pressed(egui::Key::Escape, false) {
            self.detail = None;
        } else if pressed(egui::Key::Enter, false) || pressed(egui::Key::O, false) {
            if let Some(id) = self.current_visible() {
                self.open_detail(id);
            }
        } else if pressed(egui::Key::ArrowDown, false) {
            self.move_cursor(true);
        } else if pressed(egui::Key::ArrowUp, false) {
            self.move_cursor(false);
        }
    }

    fn top_bars(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top-bars").show(ui, |ui| {
            if self.ws.config.notify.allow_insecure_transport {
                banner(ui, RED, egui::Color32::WHITE, INSECURE_BANNER);
            }
            let mut dismiss = None;
            for (i, w) in self.warnings.iter().enumerate() {
                egui::Frame::new()
                    .fill(YELLOW)
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(egui::Color32::BLACK, format!("警告：{w}"));
                            if ui.small_button("關閉警告").clicked() {
                                dismiss = Some(i);
                            }
                        });
                    });
            }
            if let Some(i) = dismiss {
                self.warnings.remove(i);
            }
            self.toolbar(ui);
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let running = self.ws.sync.is_some();
            let enabled = self.companion.is_some() && !running;
            let sync = ui
                .add_enabled(enabled, egui::Button::new("同步"))
                .on_disabled_hover_text(if running {
                    "背景 sync 執行中"
                } else {
                    NO_COMPANION
                });
            if sync.clicked() {
                self.start_sync();
            }
            if running {
                // 不用 spinner：它每一幀都要求重繪；結束由 200 ms 的輪詢偵測
                ui.label("背景 sync 執行中…");
            }
            if ui.button("重新載入").clicked() {
                self.reload();
            }
            if self.ws.new_data {
                ui.colored_label(
                    egui::Color32::from_rgb(0x20, 0x70, 0xd0),
                    "有新資料：按「重新載入」更新畫面",
                );
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status-bar").show(ui, |ui| {
            match &self.status {
                Some(r) => {
                    let text = report_text(r);
                    match r.level {
                        Level::Error => ui.colored_label(RED, text),
                        Level::Warning => {
                            ui.colored_label(egui::Color32::from_rgb(0xc0, 0x80, 0x00), text)
                        }
                        Level::Info => ui.label(text),
                    };
                }
                None => {
                    ui.label(format!(
                        "資料載入於 {}",
                        crate::text::full_time(
                            &self.ws.data.loaded_at.to_rfc3339(),
                            self.ws.services.tz
                        )
                    ));
                }
            };
        });
    }

    fn navigation(&mut self, ui: &mut egui::Ui) {
        let mut target = None;
        egui::Panel::left("navigation")
            .resizable(false)
            .exact_size(140.0)
            .show(ui, |ui| {
                for (i, s) in Screen::ALL.iter().enumerate() {
                    let r = ui
                        .selectable_label(self.screen == *s, s.title())
                        .on_hover_text(format!("Ctrl+{}", i + 1));
                    if r.clicked() {
                        target = Some(*s);
                    }
                }
            });
        if let Some(s) = target {
            self.go(s);
        }
    }
}

fn banner(ui: &mut egui::Ui, fill: egui::Color32, text: egui::Color32, msg: &str) {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(4.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.colored_label(text, msg);
        });
}
