//! 資產編輯對話框的下半部：元件表格（套件元件唯讀）、未辨識產品的處置與可搜尋的「加入元件」。
//!
//! 產品以目錄的顯示名稱呈現（版本欄標籤例如「PAN-OS 版本」）。未辨識產品列依「原文與版本 →
//! 處置選單 → 刪除」排列。加入元件的焦點流程（design 11 D6）：選單開啟時搜尋框取得焦點，
//! 選擇產品後焦點移到該元件的版本欄。兩處都使用共用的產品選擇器（[`super::product_picker`]）。
//!
//! 元件表格依目錄角色分組（21 design D5）：每組先有一列粗體組名，接著該組各列；主機套件在最後的
//! 「套件」組。新加入的元件仍加在草稿最後，只在顯示時排入其組；版本欄的 id、欄位錯誤、焦點與
//! 移除都沿用草稿的原索引，所以分組不影響加入元件後的焦點與錯誤對應。
//!
//! 每個清冊元件列的「EoL 覆寫…」開啟軟體 EoL 覆寫的小對話框（[`super::asset_lifecycle`]，
//! 22 design D11），按鈕後顯示目前的覆寫。

use cvewatch_core::catalog::Catalog;
use cvewatch_frontend::asset_form::{ComponentDraft, UnresolvedAction};
use cvewatch_frontend::component_groups::group_drafts;
use cvewatch_frontend::display::{product_name, version_text, PENDING_NOTE, PENDING_VERSION};
use eframe::egui;

use super::asset_edit::AssetEdit;
use super::asset_lifecycle::{override_summary, ComponentEol, EOL_OVERRIDE_BUTTON};
use super::product_picker::{product_picker, Picked, PickerButton, PickerMode};
use crate::screens::ALERT;
use crate::text::one_line;

/// 套件元件的說明（唯讀）。
pub const PACKAGE_NOTE: &str = "由套件清單管理";

/// 「加入元件」選單的標籤。
pub const ADD_COMPONENT: &str = "加入元件";

/// 未辨識產品「改為…」的提示。
pub const MAP_HINT: &str = "改為目錄產品（沿用原版本字串）";

const VERSION_WIDTH: f32 = 180.0;

pub(super) fn components(ui: &mut egui::Ui, edit: &mut AssetEdit, catalog: &Catalog) {
    ui.label(egui::RichText::new("元件 *（至少一個）").strong());
    if edit.form.components.is_empty() {
        ui.label("尚無元件，請由下方「加入元件」選擇產品。");
    }
    let errors = edit.errors.clone();
    let focus = edit.focus_version.take();
    let mut dropped = None;
    let mut eol = None;
    let groups = group_drafts(catalog, &edit.form.components);
    egui::Grid::new("asset-components")
        .num_columns(4)
        // 組名列會打亂條紋的交替，改以組名區隔（21 design D5）
        .striped(false)
        .show(ui, |ui| {
            for (group, indices) in groups {
                // 組名列只是標籤，不加入 Tab 焦點順序（21 design D5）
                ui.label(egui::RichText::new(group.label()).strong());
                ui.label("");
                ui.label("");
                ui.label("");
                ui.end_row();
                for i in indices {
                    let d = &mut edit.form.components[i];
                    let name = one_line(&d.name(catalog));
                    match component_row(ui, d, i, &name, focus == Some(i)) {
                        Some(RowAction::Drop) => dropped = Some(i),
                        Some(RowAction::EditEol) => eol = Some(i),
                        None => {}
                    }
                    ui.end_row();
                    for msg in errors.component(i) {
                        ui.label("");
                        ui.colored_label(ALERT, msg);
                        ui.end_row();
                    }
                }
            }
        });
    if let Some(i) = eol {
        edit.eol_dialog = ComponentEol::open(edit, i, catalog);
    }
    if let Some(i) = dropped {
        // 尚未儲存的新元件直接拿掉（錯誤的列索引隨之失效）
        edit.form.components.remove(i);
        edit.errors.components.clear();
    }
}

/// 元件列的操作。
enum RowAction {
    /// 拿掉尚未儲存的新元件
    Drop,
    /// 開啟 EoL 覆寫小對話框
    EditEol,
}

/// 第 `row` 列元件（名稱、版本、EoL 覆寫、移除）。`focus` 時版本欄取得輸入焦點。
fn component_row(
    ui: &mut egui::Ui,
    d: &mut ComponentDraft,
    row: usize,
    name: &str,
    focus: bool,
) -> Option<RowAction> {
    if d.package {
        ui.label(name);
        ui.label(one_line(&d.version));
        ui.weak(PACKAGE_NOTE);
        ui.label("");
        return None;
    }
    let l = ui
        .label(format!("{name} 版本"))
        .on_hover_text(format!("產品 key：{}", d.product_key));
    if d.removed {
        ui.weak(format!("{}（儲存時移除）", one_line(&d.version)));
        ui.label("");
        if ui.button(format!("復原 {name}")).clicked() {
            d.removed = false;
        }
        return None;
    }
    let pending = d.is_untouched_pending();
    let field = ui
        .add(
            egui::TextEdit::singleline(&mut d.version)
                // 以列索引區分：重複加入同一產品（驗證會擋下）時兩列仍是不同的欄位
                .id_salt(("component-version", row))
                // 版本待補的元件版本欄空白，以提示文字標示（18 design D8）
                .hint_text(if pending { PENDING_VERSION } else { "" })
                .desired_width(VERSION_WIDTH)
                // Grid 的欄寬沿用前一幀：只設 desired_width 時第一次排版的窄欄會一直保留
                .min_size(egui::vec2(VERSION_WIDTH, 0.0)),
        )
        .labelled_by(l.id);
    if pending {
        field.on_hover_text(format!(
            "{PENDING_VERSION}{PENDING_NOTE}：填上版本後開始比對"
        ));
    } else if focus {
        field.request_focus();
    }
    let mut action = eol_cell(ui, d, name).then_some(RowAction::EditEol);
    if ui.button(format!("移除 {name}")).clicked() {
        if d.id.is_none() {
            action = Some(RowAction::Drop);
        } else {
            d.removed = true;
        }
    }
    action
}

/// 「EoL 覆寫…」按鈕與目前的覆寫（同一格）；回傳是否按下按鈕。
fn eol_cell(ui: &mut egui::Ui, d: &ComponentDraft, name: &str) -> bool {
    ui.horizontal(|ui| {
        let clicked = ui
            .button(EOL_OVERRIDE_BUTTON)
            .on_hover_text(format!("設定或清除 {name} 的軟體 EoL 覆寫（日期與來源）"))
            .clicked();
        if let Some(summary) = override_summary(&d.eol) {
            ui.weak(one_line(&summary));
        }
        clicked
    })
    .inner
}

pub(super) fn unresolved(ui: &mut egui::Ui, edit: &mut AssetEdit, catalog: &Catalog) {
    if edit.form.unresolved.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.label(egui::RichText::new("未辨識產品").strong());
    let errors = edit.errors.clone();
    for (i, u) in edit.form.unresolved.iter_mut().enumerate() {
        let state = edit.unresolved_pickers.entry(u.id).or_default();
        ui.horizontal(|ui| {
            let raw = one_line(&u.product_raw);
            let l = ui.label(format!("{raw} {}", one_line(version_text(&u.version_raw))));
            let current = match &u.action {
                UnresolvedAction::Keep => "保留".to_string(),
                UnresolvedAction::MapTo(key) => format!("改為 {}", product_name(catalog, key)),
                UnresolvedAction::Delete => "儲存時刪除".to_string(),
            };
            let mode = PickerMode::Unresolved {
                product_raw: &u.product_raw,
            };
            let button = PickerButton {
                id: ui.make_persistent_id(("unresolved-action", u.id)),
                text: &current,
                label: l.id,
                hover: Some(MAP_HINT),
            };
            let picked = product_picker(ui, button, state, catalog, mode);
            match picked {
                Some(Picked::Keep) => u.action = UnresolvedAction::Keep,
                Some(Picked::Product(key)) => u.action = UnresolvedAction::MapTo(key),
                None => {}
            }
            if ui.button(format!("刪除「{raw}」")).clicked() {
                u.action = UnresolvedAction::Delete;
            }
        });
        for msg in errors.unresolved_row(i) {
            ui.colored_label(ALERT, msg);
        }
    }
}

pub(super) fn add_component(ui: &mut egui::Ui, edit: &mut AssetEdit, catalog: &Catalog) {
    ui.add_space(6.0);
    let mut picked = None;
    ui.horizontal(|ui| {
        let l = ui.label(ADD_COMPONENT);
        let button = PickerButton {
            id: ui.make_persistent_id("add-component"),
            text: "選擇目錄產品…",
            label: l.id,
            hover: None,
        };
        picked = product_picker(ui, button, &mut edit.add_picker, catalog, PickerMode::Add);
    });
    if let Some(Picked::Product(key)) = picked {
        edit.form.add_product(&key);
        edit.focus_version = Some(edit.form.components.len() - 1);
    }
}
