//! 資產畫面：工具列（新增資產、匯入清冊、下載範本、匯出清冊、顯示已退役）、資產清單；
//! 選取後顯示元件（含版本歷史）、該資產的命中與未辨識產品，以及編輯、刪除、恢復、永久刪除。

use cvewatch_core::assets::{AssetStatus, Component, ComponentKind};
use cvewatch_core::matching::query::FindingRow;
use cvewatch_frontend::data::{AssetEntry, AssetListFilter};
use cvewatch_frontend::Report;
use eframe::egui;

use super::heading;
use crate::app::{GuiApp, NO_COMPANION};
use crate::text::{one_line, short_time};

/// 退役資產在清單中的標示。
pub const RETIRED_MARK: &str = "（已退役）";

/// 詳情的操作。
enum DetailAction {
    Edit(String),
    Retire(String),
    Restore(String),
    Purge(String),
}

impl GuiApp {
    pub(super) fn show_assets(&mut self, ui: &mut egui::Ui) {
        self.assets_toolbar(ui);
        ui.separator();
        let picked = self.asset_list(ui);
        if picked.is_some() {
            self.assets_selected = picked;
        }
        let selected = self
            .assets_selected
            .as_deref()
            .and_then(|t| self.ws.data.any_asset(t))
            .filter(|a| self.assets_show_retired || a.asset.status == AssetStatus::Active);
        let action = match selected {
            Some(entry) => self.asset_detail(ui, entry),
            None => {
                ui.label("請從左側選取資產。");
                unresolved_all(ui, self);
                None
            }
        };
        match action {
            Some(DetailAction::Edit(tag)) => self.start_edit_asset(&tag),
            Some(DetailAction::Retire(tag)) => self.start_retire_asset(&tag),
            Some(DetailAction::Restore(tag)) => self.restore_asset(&tag),
            Some(DetailAction::Purge(tag)) => self.start_purge_asset(&tag),
            None => {}
        }
    }

    fn assets_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("新增資產").clicked() {
                self.start_new_asset();
            }
            let enabled = self.companion.is_some();
            if ui
                .add_enabled(enabled, egui::Button::new("匯入清冊"))
                .on_disabled_hover_text(NO_COMPANION)
                .clicked()
            {
                self.pick_inventory();
            }
            if ui.button("下載範本").clicked() {
                self.download_template();
            }
            if ui.button("匯出清冊").clicked() {
                self.export_inventory();
            }
            ui.checkbox(&mut self.assets_show_retired, "顯示已退役");
            ui.label("也可以把 .xlsx 或 .csv 清冊拖進視窗");
        });
    }

    /// 左側清單；回傳被點選的資產編號。
    fn asset_list(&self, ui: &mut egui::Ui) -> Option<String> {
        let mut picked = None;
        let filter = AssetListFilter {
            include_retired: self.assets_show_retired,
        };
        egui::Panel::left("asset-list")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("asset-list-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let list = self.ws.data.asset_list(&filter);
                        for a in &list {
                            let tag = &a.asset.record.asset_tag;
                            let on = self.assets_selected.as_deref() == Some(tag.as_str());
                            if ui.selectable_label(on, list_text(a)).clicked() {
                                picked = Some(tag.clone());
                            }
                        }
                        if list.is_empty() {
                            ui.label("尚無資產。請匯入清冊或按「新增資產」。");
                        }
                    });
            });
        picked
    }

    fn asset_detail(&self, ui: &mut egui::Ui, a: &AssetEntry) -> Option<DetailAction> {
        let r = &a.asset.record;
        let retired = a.asset.status == AssetStatus::Retired;
        let findings = self.ws.data.asset_findings(&r.asset_tag);
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt("asset-detail")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mark = if retired { RETIRED_MARK } else { "" };
                heading(ui, one_line(&format!("{}  {}{mark}", r.asset_tag, r.name)));
                action = detail_buttons(ui, &r.asset_tag, retired);
                ui.label(format!(
                    "{}｜曝露度 {}｜重要性 {}",
                    r.category.as_str(),
                    r.exposure.as_str(),
                    r.criticality
                ));
                ui.label(format!(
                    "未處理命中 {} 筆｜全部命中 {} 筆",
                    a.open_findings,
                    findings.len()
                ));
                ui.add_space(6.0);
                self.components_list(ui, a);
                ui.add_space(6.0);
                findings_grid(ui, &findings);
                if !a.unresolved.is_empty() {
                    ui.add_space(6.0);
                    heading(ui, "未辨識產品");
                    for u in &a.unresolved {
                        ui.label(format!("{} {}", one_line(&u.product_raw), u.version_raw));
                    }
                }
            });
        action
    }

    /// 元件與其版本歷史。
    fn components_list(&self, ui: &mut egui::Ui, a: &AssetEntry) {
        heading(ui, format!("元件（{}）", a.components.len()));
        for c in &a.components {
            ui.label(component_text(c));
            for h in a.history.get(&c.id).into_iter().flatten() {
                ui.weak(format!(
                    "    先前版本 {}（{} 變更）",
                    one_line(&h.version_raw),
                    short_time(&h.changed_at, self.ws.services.tz)
                ));
            }
        }
    }

    /// 「下載範本」：另存新檔對話框（預設文件資料夾）。
    pub fn download_template(&mut self) {
        let (dir, name) = self.ws.template_default();
        let r = match self.dialogs.save_xlsx("下載清冊範本", &dir, &name) {
            None => Report::info("已取消下載範本"),
            Some(path) => self.ws.write_template(&path),
        };
        self.report(r);
    }

    /// 「匯出清冊」：另存新檔對話框（預設文件資料夾、檔名含當天日期）。
    pub fn export_inventory(&mut self) {
        let (dir, name) = self.ws.inventory_export_default();
        let r = match self.dialogs.save_xlsx("匯出資產清冊", &dir, &name) {
            None => Report::info("已取消匯出清冊"),
            Some(path) => self.ws.export_inventory(&path),
        };
        self.report(r);
    }
}

fn findings_grid(ui: &mut egui::Ui, findings: &[&FindingRow]) {
    heading(ui, "命中");
    egui::Grid::new("asset-findings")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            for f in findings {
                ui.label(f.priority.as_str());
                ui.label(one_line(&f.cve_id));
                ui.label(one_line(&format!("{} {}", f.component, f.version)));
                ui.label(f.status.label());
                ui.end_row();
            }
        });
}

fn list_text(a: &AssetEntry) -> egui::RichText {
    let r = &a.asset.record;
    if a.asset.status == AssetStatus::Retired {
        return egui::RichText::new(one_line(&format!(
            "{}  {}{RETIRED_MARK}",
            r.asset_tag, r.name
        )))
        .color(egui::Color32::GRAY);
    }
    egui::RichText::new(one_line(&format!(
        "{}  {}（未處理 {}）",
        r.asset_tag, r.name, a.open_findings
    )))
}

/// 使用中資產：編輯、刪除；已退役：恢復、永久刪除（不提供編輯）。
fn detail_buttons(ui: &mut egui::Ui, tag: &str, retired: bool) -> Option<DetailAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        if retired {
            if ui.button("恢復").clicked() {
                action = Some(DetailAction::Restore(tag.to_string()));
            }
            if ui.button("永久刪除").clicked() {
                action = Some(DetailAction::Purge(tag.to_string()));
            }
        } else {
            if ui.button("編輯").clicked() {
                action = Some(DetailAction::Edit(tag.to_string()));
            }
            if ui
                .button("刪除")
                .on_hover_text("標為退役：保留紀錄、不再比對、可恢復")
                .clicked()
            {
                action = Some(DetailAction::Retire(tag.to_string()));
            }
        }
    });
    action
}

fn component_text(c: &Component) -> String {
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
