//! 備註對話框：確認後才寫入；「取消」或 Esc 不變更。

use cvewatch_core::matching::pending_ops::Action;
use cvewatch_frontend::workspace::change_detail;
use eframe::egui;

use super::{escape_pressed, NotePurpose, Outcome};

/// 對話框標題，例如「設為忽略（判定不受影響，需特定設定）：3 筆命中」。
pub fn title(purpose: &NotePurpose) -> String {
    match purpose {
        NotePurpose::Triage(d) => format!(
            "設為{}{}：{} 筆命中",
            d.status.label(),
            change_detail(d.reason, d.justification),
            d.targets.len()
        ),
        NotePurpose::Pending {
            cve,
            action: Action::Apply(target),
        } => format!("{cve} 套用到 {target}"),
        NotePurpose::Pending {
            cve,
            action: Action::Dismiss,
        } => format!("{cve} 標為不相關（移出等待區）"),
    }
}

pub fn show(
    ctx: &egui::Context,
    purpose: &NotePurpose,
    text: &mut String,
    focused: &mut bool,
) -> Outcome {
    let mut out = Outcome::Keep;
    egui::Modal::new(egui::Id::unique("dialog-note")).show(ctx, |ui| {
        ui.heading(title(purpose));
        ui.add_space(6.0);
        ui.label("備註（寫入時間軸）");
        let edit = ui.add(
            egui::TextEdit::multiline(text)
                .id_salt("note-text")
                .hint_text("例如：已更新韌體")
                .desired_rows(3)
                .desired_width(420.0),
        );
        if !*focused {
            edit.request_focus();
            *focused = true;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("確認").clicked() {
                out = Outcome::Submit(purpose.clone(), text.clone());
            }
            if ui.button("取消").clicked() {
                out = Outcome::Close;
            }
        });
    });
    // 點對話框外不關閉（避免誤觸丟失備註）；Esc 取消
    if out == Outcome::Keep && escape_pressed(ctx) {
        return Outcome::Close;
    }
    out
}
