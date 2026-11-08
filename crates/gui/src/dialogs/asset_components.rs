//! 資產編輯對話框的下半部：元件表格（套件元件唯讀）、未辨識產品的處置與可搜尋的「加入元件」。
//!
//! 產品以目錄的顯示名稱呈現（版本欄標籤例如「PAN-OS 版本」）。未辨識產品列依「原文與版本 →
//! 處置選單 → 刪除」排列。加入元件的焦點流程（design 11 D6）：選單開啟時搜尋框取得焦點，
//! 選擇產品後焦點移到該元件的版本欄。

use cvewatch_core::catalog::Catalog;
use cvewatch_frontend::asset_form::{ComponentDraft, ProductChoice, UnresolvedAction};
use cvewatch_frontend::display::product_name;
use eframe::egui;

use super::asset_edit::AssetEdit;
use crate::screens::ALERT;
use crate::text::one_line;

/// 套件元件的說明（唯讀）。
pub const PACKAGE_NOTE: &str = "由套件清單管理";

/// 「加入元件」選單的標籤。
pub const ADD_COMPONENT: &str = "加入元件";

const VERSION_WIDTH: f32 = 180.0;

pub(super) fn components(ui: &mut egui::Ui, edit: &mut AssetEdit, catalog: &Catalog) {
    ui.label(egui::RichText::new("元件 *（至少一個）").strong());
    if edit.form.components.is_empty() {
        ui.label("尚無元件，請由下方「加入元件」選擇產品。");
    }
    let errors = edit.errors.clone();
    let focus = edit.focus_version.take();
    let mut dropped = None;
    egui::Grid::new("asset-components")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            for (i, d) in edit.form.components.iter_mut().enumerate() {
                let name = one_line(&d.name(catalog));
                if component_row(ui, d, i, &name, focus == Some(i)) {
                    dropped = Some(i);
                }
                ui.end_row();
                for msg in errors.component(i) {
                    ui.label("");
                    ui.colored_label(ALERT, msg);
                    ui.end_row();
                }
            }
        });
    if let Some(i) = dropped {
        // 尚未儲存的新元件直接拿掉（錯誤的列索引隨之失效）
        edit.form.components.remove(i);
        edit.errors.components.clear();
    }
}

/// 第 `row` 列元件；回傳 true 表示要拿掉這個尚未儲存的新元件。`focus` 時版本欄取得輸入焦點。
fn component_row(
    ui: &mut egui::Ui,
    d: &mut ComponentDraft,
    row: usize,
    name: &str,
    focus: bool,
) -> bool {
    if d.package {
        ui.label(name);
        ui.label(one_line(&d.version));
        ui.weak(PACKAGE_NOTE);
        return false;
    }
    let l = ui
        .label(format!("{name} 版本"))
        .on_hover_text(format!("產品 key：{}", d.product_key));
    if d.removed {
        ui.weak(format!("{}（儲存時移除）", one_line(&d.version)));
        if ui.button(format!("復原 {name}")).clicked() {
            d.removed = false;
        }
        return false;
    }
    let field = ui
        .add(
            egui::TextEdit::singleline(&mut d.version)
                // 以列索引區分：重複加入同一產品（驗證會擋下）時兩列仍是不同的欄位
                .id_salt(("component-version", row))
                .desired_width(VERSION_WIDTH)
                // Grid 的欄寬沿用前一幀：只設 desired_width 時第一次排版的窄欄會一直保留
                .min_size(egui::vec2(VERSION_WIDTH, 0.0)),
        )
        .labelled_by(l.id);
    if focus {
        field.request_focus();
    }
    if ui.button(format!("移除 {name}")).clicked() {
        if d.id.is_none() {
            return true;
        }
        d.removed = true;
    }
    false
}

pub(super) fn unresolved(
    ui: &mut egui::Ui,
    edit: &mut AssetEdit,
    products: &[ProductChoice],
    catalog: &Catalog,
) {
    if edit.form.unresolved.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.label(egui::RichText::new("未辨識產品").strong());
    let errors = edit.errors.clone();
    for (i, u) in edit.form.unresolved.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let raw = one_line(&u.product_raw);
            let l = ui.label(format!("{raw} {}", one_line(&u.version_raw)));
            let current = match &u.action {
                UnresolvedAction::Keep => "保留".to_string(),
                UnresolvedAction::MapTo(key) => format!("改為 {}", product_name(catalog, key)),
                UnresolvedAction::Delete => "儲存時刪除".to_string(),
            };
            let combo = egui::ComboBox::from_id_salt(("unresolved-action", u.id))
                .selected_text(current)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(false, "保留").clicked() {
                        u.action = UnresolvedAction::Keep;
                    }
                    for p in products {
                        if ui
                            .selectable_label(false, format!("改為 {}", p.label()))
                            .clicked()
                        {
                            u.action = UnresolvedAction::MapTo(p.key.clone());
                        }
                    }
                });
            combo
                .response
                .labelled_by(l.id)
                .on_hover_text("改為目錄產品（沿用原版本字串）");
            if ui.button(format!("刪除「{raw}」")).clicked() {
                u.action = UnresolvedAction::Delete;
            }
        });
        for msg in errors.unresolved_row(i) {
            ui.colored_label(ALERT, msg);
        }
    }
}

pub(super) fn add_component(ui: &mut egui::Ui, edit: &mut AssetEdit, choices: &[ProductChoice]) {
    ui.add_space(6.0);
    let mut picked = None;
    let just_opened = !edit.picker_open;
    ui.horizontal(|ui| {
        let l = ui.label(ADD_COMPONENT);
        let combo = egui::ComboBox::from_id_salt("add-component")
            .selected_text("選擇目錄產品…")
            .height(320.0)
            .show_ui(ui, |ui| {
                let s = ui.label("搜尋產品");
                let search = ui
                    .add(egui::TextEdit::singleline(&mut edit.search).desired_width(220.0))
                    .labelled_by(s.id);
                if just_opened {
                    search.request_focus();
                }
                for c in choices {
                    if ui.selectable_label(false, c.label()).clicked() {
                        picked = Some(c.key.clone());
                    }
                }
                if choices.is_empty() {
                    ui.label("沒有相符的產品");
                }
            });
        combo.response.labelled_by(l.id);
        edit.picker_open = combo.inner.is_some();
    });
    if let Some(key) = picked {
        edit.form.add_product(&key);
        edit.search.clear();
        edit.focus_version = Some(edit.form.components.len() - 1);
    }
}
