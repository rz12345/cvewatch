//! 資產畫面：工具列與左側清單（[`super::asset_list`]）；選取後顯示元件（含版本歷史）、
//! 該資產的命中與未辨識產品，以及編輯、退役、恢復、永久刪除。
//!
//! 產品以目錄的顯示名稱呈現（產品 key 只在滑鼠提示中），類別與曝露度雙語（design 11 D1）。

use cvewatch_core::assets::{AssetStatus, ComponentKind};
use cvewatch_core::catalog::Catalog;
use cvewatch_core::matching::query::FindingRow;
use cvewatch_frontend::data::AssetEntry;
use cvewatch_frontend::display::{
    class_label, component_name, exposure_label, product_name, shown_name,
};
use cvewatch_frontend::Report;
use eframe::egui;

use super::asset_list::RETIRED_MARK;
use super::{heading, ALERT};
use crate::app::GuiApp;
use crate::text::{one_line, short_time};

/// 使用中資產的退役按鈕（確認後標為退役，可恢復）。
pub const RETIRE_BUTTON: &str = "退役…";

/// 沒有命中時的命中區塊。
pub const NO_FINDINGS: &str = "沒有命中";

/// 已退役資產的說明。
pub const RETIRED_NOTE: &str = "已退役：不參與比對，命中與處理紀錄保留";

/// 一般操作與危險操作之間的間隔。
const DANGER_GAP: f32 = 24.0;

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
        let action = match self.selected_asset() {
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

    /// 目前選取且顯示中的資產（未勾選「顯示已退役」時不含退役資產）。
    fn selected_asset(&self) -> Option<&AssetEntry> {
        self.assets_selected
            .as_deref()
            .and_then(|t| self.ws.data.any_asset(t))
            .filter(|a| self.assets_show_retired || a.asset.status == AssetStatus::Active)
    }

    /// 資產清單的鍵盤操作（design 11 D6）：Enter 編輯、Delete 退役確認（只對使用中資產）。
    /// 由外框在沒有文字欄位取得焦點、沒有對話框時呼叫；有按鈕等元件取得鍵盤焦點時不作用，
    /// 讓 Enter 照常按下該元件。
    pub(crate) fn assets_keys(
        &mut self,
        ctx: &egui::Context,
        pressed: &dyn Fn(egui::Key, bool) -> bool,
    ) {
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let Some(tag) = self
            .selected_asset()
            .filter(|a| a.asset.status == AssetStatus::Active)
            .map(|a| a.asset.record.asset_tag.clone())
        else {
            return;
        };
        if pressed(egui::Key::Enter, false) {
            self.start_edit_asset(&tag);
        } else if pressed(egui::Key::Delete, false) {
            self.start_retire_asset(&tag);
        }
    }

    fn asset_detail(&self, ui: &mut egui::Ui, a: &AssetEntry) -> Option<DetailAction> {
        let r = &a.asset.record;
        let retired = a.asset.status == AssetStatus::Retired;
        let findings = self.ws.data.asset_findings(&r.asset_tag);
        let catalog = &self.ws.settings.catalog;
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt("asset-detail")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                heading(ui, detail_title(a));
                action = detail_buttons(ui, &r.asset_tag, retired);
                ui.label(format!(
                    "類別 {}｜曝露度 {}",
                    class_label(&r.category, &self.ws.settings.classes),
                    exposure_label(r.exposure)
                ));
                ui.label(format!(
                    "機密性 {}｜完整性 {}｜可用性 {}｜資產價值 {}",
                    r.cia.confidentiality,
                    r.cia.integrity,
                    r.cia.availability,
                    r.cia.value()
                ));
                ui.label(format!(
                    "ISMS 編號 {}",
                    r.isms_id.as_deref().map_or("—".to_string(), one_line)
                ));
                ui.label(format!(
                    "未處理命中 {} 筆｜全部命中 {} 筆",
                    a.open_findings,
                    findings.len()
                ));
                if retired {
                    ui.weak(RETIRED_NOTE);
                }
                ui.add_space(6.0);
                self.components_list(ui, a);
                ui.add_space(6.0);
                findings_grid(ui, catalog, &findings);
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

    /// 元件（顯示名稱與版本；滑鼠提示為產品 key）與其版本歷史。
    fn components_list(&self, ui: &mut egui::Ui, a: &AssetEntry) {
        let catalog = &self.ws.settings.catalog;
        heading(ui, format!("元件（{}）", a.components.len()));
        for c in &a.components {
            let text = one_line(&format!("{} {}", component_name(catalog, c), c.version_raw));
            let resp = ui.label(text);
            if let ComponentKind::Product { product_key, .. } = &c.kind {
                resp.on_hover_text(format!("產品 key：{product_key}"));
            }
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

/// 標題：編號與名稱（名稱與編號相同時只顯示一次），退役加標示。
fn detail_title(a: &AssetEntry) -> String {
    let r = &a.asset.record;
    let mark = if a.asset.status == AssetStatus::Retired {
        RETIRED_MARK
    } else {
        ""
    };
    let title = match shown_name(r) {
        Some(name) => format!("{}  {name}{mark}", r.asset_tag),
        None => format!("{}{mark}", r.asset_tag),
    };
    one_line(&title)
}

fn findings_grid(ui: &mut egui::Ui, catalog: &Catalog, findings: &[&FindingRow]) {
    heading(ui, "命中");
    if findings.is_empty() {
        ui.label(NO_FINDINGS);
        return;
    }
    egui::Grid::new("asset-findings")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            for f in findings {
                ui.label(f.priority.as_str());
                ui.label(one_line(&f.cve_id));
                let component = product_name(catalog, &f.component);
                ui.label(one_line(&format!("{component} {}", f.version)));
                ui.label(f.status.label());
                ui.end_row();
            }
        });
}

/// 警示色文字的按鈕（危險操作）。
fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.button(egui::RichText::new(text).color(ALERT))
}

/// 使用中資產：編輯、退役…；已退役：恢復、永久刪除（不提供編輯）。危險操作在後方並以間隔分開。
fn detail_buttons(ui: &mut egui::Ui, tag: &str, retired: bool) -> Option<DetailAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        if retired {
            if ui.button("恢復").clicked() {
                action = Some(DetailAction::Restore(tag.to_string()));
            }
            ui.add_space(DANGER_GAP);
            if danger_button(ui, "永久刪除").clicked() {
                action = Some(DetailAction::Purge(tag.to_string()));
            }
        } else {
            if ui.button("編輯").on_hover_text("Enter").clicked() {
                action = Some(DetailAction::Edit(tag.to_string()));
            }
            ui.add_space(DANGER_GAP);
            if danger_button(ui, RETIRE_BUTTON)
                .on_hover_text("標為退役：保留紀錄、不再比對、可恢復（Delete）")
                .clicked()
            {
                action = Some(DetailAction::Retire(tag.to_string()));
            }
        }
    });
    action
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
