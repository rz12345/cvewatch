//! 忽略的原因，原因為「判定不受影響」時再選理由（CycloneDX VEX justification）。

use cvewatch_core::matching::triage::{IgnoreReason, Justification, Status};
use eframe::egui;

use super::{escape_pressed, Dialog, NotePurpose, Outcome, TriageDraft};

fn ignore_note(targets: &[i64], reason: IgnoreReason, j: Option<Justification>) -> Dialog {
    Dialog::note(NotePurpose::Triage(TriageDraft {
        targets: targets.to_vec(),
        status: Status::Ignored,
        reason: Some(reason),
        justification: j,
    }))
}

pub fn reason(ctx: &egui::Context, targets: &[i64]) -> Outcome {
    let mut out = Outcome::Keep;
    let modal = egui::Modal::new(egui::Id::unique("dialog-reason")).show(ctx, |ui| {
        ui.heading(format!("忽略 {} 筆命中：選擇原因", targets.len()));
        ui.add_space(6.0);
        for r in IgnoreReason::ALL {
            if ui.button(r.label()).clicked() {
                out = Outcome::Next(match r {
                    IgnoreReason::NotAffected => Dialog::Justification {
                        targets: targets.to_vec(),
                    },
                    _ => ignore_note(targets, *r, None),
                });
            }
        }
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

pub fn justification(ctx: &egui::Context, targets: &[i64]) -> Outcome {
    let mut out = Outcome::Keep;
    let modal = egui::Modal::new(egui::Id::unique("dialog-justification")).show(ctx, |ui| {
        ui.heading("判定不受影響：選擇理由");
        ui.add_space(6.0);
        for j in Justification::ALL {
            if ui.button(j.label()).clicked() {
                out = Outcome::Next(ignore_note(targets, IgnoreReason::NotAffected, Some(*j)));
            }
        }
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
