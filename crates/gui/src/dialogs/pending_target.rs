//! 等待區：選擇套用的資產（資產有多個產品元件時另列 `<資產>:<產品>`），接著輸入備註。

use cvewatch_core::matching::pending_ops::Action;
use eframe::egui;

use super::{escape_pressed, Dialog, NotePurpose, Outcome};

pub fn show(ctx: &egui::Context, cve: &str, options: &[String]) -> Outcome {
    let mut out = Outcome::Keep;
    let modal = egui::Modal::new(egui::Id::unique("dialog-pending-target")).show(ctx, |ui| {
        ui.heading(format!("{cve} 套用到資產"));
        ui.label("資產有多個產品元件時，可選「資產:產品」指定元件");
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for target in options {
                    if ui.button(target).clicked() {
                        out = Outcome::Next(Dialog::note(NotePurpose::Pending {
                            cve: cve.to_string(),
                            action: Action::Apply(target.clone()),
                        }));
                    }
                }
            });
        ui.separator();
        if ui.button("取消").clicked() {
            out = Outcome::Close;
        }
    });
    if out == Outcome::Keep && (modal.should_close() || escape_pressed(ctx)) {
        return Outcome::Close;
    }
    out
}
