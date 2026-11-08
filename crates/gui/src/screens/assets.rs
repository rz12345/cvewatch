//! 資產畫面：資產清單；選取後顯示元件、該資產的命中與未辨識產品。「匯入清冊」在此。

use cvewatch_core::assets::ComponentKind;
use cvewatch_frontend::data::AssetEntry;
use eframe::egui;

use super::heading;
use crate::app::{GuiApp, NO_COMPANION};
use crate::text::one_line;

impl GuiApp {
    pub(super) fn show_assets(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let enabled = self.companion.is_some();
            if ui
                .add_enabled(enabled, egui::Button::new("匯入清冊"))
                .on_disabled_hover_text(NO_COMPANION)
                .clicked()
            {
                self.pick_inventory();
            }
            ui.label("也可以把 .xlsx 或 .csv 清冊拖進視窗");
        });
        ui.separator();
        let mut picked = None;
        egui::Panel::left("asset-list")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("asset-list-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for a in &self.ws.data.assets {
                            let tag = &a.asset.record.asset_tag;
                            let on = self.assets_selected.as_deref() == Some(tag.as_str());
                            let text = one_line(&format!(
                                "{tag}  {}（未處理 {}）",
                                a.asset.record.name, a.open_findings
                            ));
                            if ui.selectable_label(on, text).clicked() {
                                picked = Some(tag.clone());
                            }
                        }
                        if self.ws.data.assets.is_empty() {
                            ui.label("尚無資產。請匯入清冊。");
                        }
                    });
            });
        if picked.is_some() {
            self.assets_selected = picked;
        }
        let selected = self
            .assets_selected
            .as_deref()
            .and_then(|t| self.ws.data.asset(t));
        match selected {
            Some(entry) => self.asset_detail(ui, entry),
            None => {
                ui.label("請從左側選取資產。");
                unresolved_all(ui, self);
            }
        }
    }

    fn asset_detail(&self, ui: &mut egui::Ui, a: &AssetEntry) {
        let r = &a.asset.record;
        let findings = self.ws.data.asset_findings(&r.asset_tag);
        egui::ScrollArea::vertical()
            .id_salt("asset-detail")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                heading(ui, one_line(&format!("{}  {}", r.asset_tag, r.name)));
                ui.label(format!(
                    "未處理命中 {} 筆｜全部命中 {} 筆",
                    a.open_findings,
                    findings.len()
                ));
                ui.add_space(6.0);
                heading(ui, format!("元件（{}）", a.components.len()));
                for c in &a.components {
                    ui.label(component_text(c));
                }
                ui.add_space(6.0);
                heading(ui, "命中");
                egui::Grid::new("asset-findings")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        for f in &findings {
                            ui.label(f.priority.as_str());
                            ui.label(one_line(&f.cve_id));
                            ui.label(one_line(&format!("{} {}", f.component, f.version)));
                            ui.label(f.status.label());
                            ui.end_row();
                        }
                    });
                if !a.unresolved.is_empty() {
                    ui.add_space(6.0);
                    heading(ui, "未辨識產品");
                    for u in &a.unresolved {
                        ui.label(format!("{} {}", one_line(&u.product_raw), u.version_raw));
                    }
                }
            });
    }
}

fn component_text(c: &cvewatch_core::assets::Component) -> String {
    let name = match &c.kind {
        ComponentKind::Product { product_key, .. } => product_key,
        ComponentKind::Package { purl, .. } => purl,
    };
    one_line(&format!("{name} {}", c.version_raw))
}

/// 沒有選取資產時：列出全部未辨識產品。
fn unresolved_all(ui: &mut egui::Ui, app: &GuiApp) {
    if app.ws.data.unresolved.is_empty() {
        return;
    }
    ui.add_space(8.0);
    heading(ui, "未辨識產品");
    for u in &app.ws.data.unresolved {
        ui.label(format!(
            "{}：{} {}",
            u.asset_tag,
            one_line(&u.product_raw),
            u.version_raw
        ));
    }
}
