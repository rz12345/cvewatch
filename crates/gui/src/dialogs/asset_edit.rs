//! 「新增資產」與「編輯」共用的對話框（design D11）：上半部資產欄位（category、曝露度、重要性為
//! 下拉選單；編輯時資產編號可修改＝更名，與原編號不同時欄位下方提示影響，12 design D4），
//! 下半部元件與未辨識產品（[`super::asset_components`]）。
//!
//! 每個欄位只有左側一個標籤，必填欄位標「*」；類別與曝露度的選項雙語（design 11 D3）。
//! 驗證錯誤以介面用語紅字顯示在欄位下方，失敗時保留對話框與輸入內容；本地驗證失敗不寫狀態列，
//! 以「取消」或 Esc 關閉時清除對話框留在狀態列的寫入失敗訊息（design 11 D5）。

use std::sync::LazyLock;

use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::validate::REQUIRED_FIELDS;
use cvewatch_core::assets::{Category, Exposure, MAX_CRITICALITY, MIN_CRITICALITY};
use cvewatch_core::catalog::Catalog;
use cvewatch_frontend::asset_form::{
    field_label, product_choices, search_products, AssetForm, FormErrors, FormMode, ProductChoice,
};
use cvewatch_frontend::display::{category_label, exposure_label};
use cvewatch_frontend::Level;
use eframe::egui;

use super::asset_components::{add_component, components, unresolved};
use super::{escape_pressed, AssetDialog};
use crate::app::GuiApp;
use crate::screens::ALERT;
use crate::text::one_line;

pub use super::asset_components::PACKAGE_NOTE;

/// 更名提示（12 design D4）：外部檔案的舊編號不會對應到更名後的資產。
pub fn rename_note(old_tag: &str) -> String {
    format!(
        "更名後，舊清冊與已匯出的 VEX 中的「{}」不會對應到此資產；儲存後建議重新匯出清冊",
        one_line(old_tag)
    )
}

/// 有欄位錯誤時對話框底部的摘要。
pub const FIX_FIELDS: &str = "請修正標示的欄位";

const FIELD_WIDTH: f32 = 260.0;

/// 對話框狀態。
pub struct AssetEdit {
    pub form: AssetForm,
    /// 開啟時的內容（判斷是否有未儲存的修改）
    initial: AssetForm,
    pub errors: FormErrors,
    /// 寫入失敗的訊息（core 拒絕或中途失敗）
    pub failure: Option<String>,
    /// 「加入元件」的搜尋字串
    pub search: String,
    /// 「加入元件」選單在上一幀是開啟的（剛開啟的那一幀讓搜尋框取得焦點）
    pub picker_open: bool,
    /// 下一幀讓這個元件列的版本欄取得焦點（剛加入的元件）
    pub focus_version: Option<usize>,
}

impl AssetEdit {
    pub fn new(form: AssetForm) -> AssetEdit {
        AssetEdit {
            initial: form.clone(),
            form,
            errors: FormErrors::default(),
            failure: None,
            search: String::new(),
            picker_open: false,
            focus_version: None,
        }
    }

    /// 有尚未儲存的修改。
    pub fn dirty(&self) -> bool {
        self.form != self.initial
    }
}

enum Action {
    Save,
    Cancel,
}

/// 目錄產品選單：「加入元件」的搜尋結果與「改為目錄產品」的完整清單。
struct Products {
    matching: Vec<ProductChoice>,
    all: Vec<ProductChoice>,
}

impl GuiApp {
    /// 「新增資產」。
    pub fn start_new_asset(&mut self) {
        self.asset_dialog = Some(AssetDialog::Edit(Box::new(
            AssetEdit::new(AssetForm::new()),
        )));
    }

    /// 編輯所選的使用中資產。
    pub fn start_edit_asset(&mut self, tag: &str) {
        match self.ws.data.asset(tag) {
            Some(entry) => {
                let form = AssetForm::from_entry(entry);
                self.asset_dialog = Some(AssetDialog::Edit(Box::new(AssetEdit::new(form))));
            }
            None => self.report(cvewatch_frontend::Report::warning(format!(
                "找不到使用中的資產 {tag}，請重新載入"
            ))),
        }
    }

    pub(super) fn show_asset_edit(&mut self, ctx: &egui::Context, mut edit: Box<AssetEdit>) {
        let catalog = &self.ws.settings.catalog;
        let products = Products {
            matching: search_products(catalog, &edit.search),
            all: product_choices(catalog),
        };
        // Esc 先用來關閉下拉選單；有未儲存的修改時不以 Esc 關閉（避免丟失輸入，請按「取消」）
        let popup_open = egui::Popup::is_any_open(ctx);
        let mut action = None;
        egui::Modal::new(egui::Id::unique("dialog-asset-edit")).show(ctx, |ui| {
            action = body(ui, &mut edit, &products, catalog);
        });
        if action.is_none() && !popup_open && !edit.dirty() && escape_pressed(ctx) {
            action = Some(Action::Cancel);
        }
        match action {
            None => self.asset_dialog = Some(AssetDialog::Edit(edit)),
            Some(Action::Cancel) => self.clear_dialog_failure(edit.failure.as_deref()),
            Some(Action::Save) => self.save_asset(edit),
        }
    }

    /// 關閉對話框時，狀態列若仍是這個對話框的寫入失敗訊息就清除。
    fn clear_dialog_failure(&mut self, failure: Option<&str>) {
        let left_behind = self
            .status
            .as_ref()
            .is_some_and(|r| Some(r.message.as_str()) == failure);
        if left_behind {
            self.status = None;
        }
    }

    /// 儲存：本地驗證失敗時只在對話框內顯示錯誤（不寫狀態列）；寫入失敗時保留對話框與輸入內容；
    /// 成功時關閉並選取該資產。
    fn save_asset(&mut self, mut edit: Box<AssetEdit>) {
        // 上一次的寫入失敗已不適用（對話框與狀態列都清除），本次結果另行顯示
        self.clear_dialog_failure(edit.failure.take().as_deref());
        if let Err(errors) = self.ws.validate_asset_form(&edit.form) {
            edit.errors = errors;
            self.asset_dialog = Some(AssetDialog::Edit(edit));
            return;
        }
        edit.errors = FormErrors::default();
        let open = self.open_ids();
        let refreshed = self.ws.save_asset_form(&edit.form, &open);
        let tag = edit.form.asset_tag.trim().to_string();
        let failed = refreshed.report.level == Level::Error;
        if failed {
            // 沒有寫入任何變更（同一交易）：保留對話框與輸入內容
            edit.failure = Some(refreshed.report.message.clone());
            self.asset_dialog = Some(AssetDialog::Edit(edit));
        } else {
            self.assets_selected = Some(tag);
        }
        self.after_asset_write(refreshed);
    }
}

fn body(
    ui: &mut egui::Ui,
    edit: &mut AssetEdit,
    products: &Products,
    catalog: &Catalog,
) -> Option<Action> {
    let title = match &edit.form.mode {
        FormMode::New => "新增資產".to_string(),
        FormMode::Edit { original_tag } => format!("編輯 {}", one_line(original_tag)),
    };
    ui.heading(title);
    egui::ScrollArea::vertical()
        .max_height(560.0)
        .show(ui, |ui| {
            fields(ui, edit);
            ui.separator();
            components(ui, edit, catalog);
            unresolved(ui, edit, &products.all, catalog);
            add_component(ui, edit, &products.matching);
        });
    ui.separator();
    let marked = !edit.errors.fields.is_empty()
        || !edit.errors.components.is_empty()
        || !edit.errors.unresolved.is_empty();
    if marked {
        ui.colored_label(ALERT, FIX_FIELDS);
    }
    for msg in edit.errors.general.iter().chain(edit.failure.iter()) {
        ui.colored_label(ALERT, msg);
    }
    let mut action = None;
    ui.horizontal(|ui| {
        if ui.button("儲存").clicked() {
            action = Some(Action::Save);
        }
        if ui.button("取消").clicked() {
            action = Some(Action::Cancel);
        }
    });
    action
}

/// 欄位下方的紅字錯誤。
fn field_error(ui: &mut egui::Ui, errors: &FormErrors, field: StandardField) {
    if let Some(msg) = errors.field(field) {
        ui.colored_label(ALERT, msg);
    }
}

/// 欄位標籤：介面名稱，必填欄位加「 *」。
fn label_text(field: StandardField) -> String {
    let name = field_label(field);
    if REQUIRED_FIELDS.contains(&field) {
        format!("{name} *")
    } else {
        name.to_string()
    }
}

fn text_field(ui: &mut egui::Ui, value: &mut String, errors: &FormErrors, field: StandardField) {
    let l = ui.label(label_text(field));
    ui.add(egui::TextEdit::singleline(value).desired_width(FIELD_WIDTH))
        .labelled_by(l.id);
    ui.end_row();
    if errors.field(field).is_some() {
        ui.label("");
        field_error(ui, errors, field);
        ui.end_row();
    }
}

/// 下拉選單：左側標籤以 `labelled_by` 關聯（選單右側不再重複標籤）。
fn choice_field(
    ui: &mut egui::Ui,
    value: &mut String,
    options: &[Choice],
    errors: &FormErrors,
    field: StandardField,
) {
    let l = ui.label(label_text(field));
    let shown = match options.iter().find(|o| o.value == value.trim()) {
        Some(o) => o.text.clone(),
        None if value.trim().is_empty() => "（請選擇）".to_string(),
        None => one_line(value),
    };
    let combo = egui::ComboBox::from_id_salt(("asset-field", field.as_str()))
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for o in options {
                if ui
                    .selectable_label(value.as_str() == o.value, &o.text)
                    .clicked()
                {
                    *value = o.value.clone();
                }
            }
        });
    combo.response.labelled_by(l.id);
    ui.end_row();
    if errors.field(field).is_some() {
        ui.label("");
        field_error(ui, errors, field);
        ui.end_row();
    }
}

/// 下拉選單的一個選項：儲存的值（英文）與顯示文字。
struct Choice {
    value: String,
    text: String,
}

/// 選項內容固定，只建立一次（不在每一幀重新組字）。
static FIELD_OPTIONS: LazyLock<FieldOptions> = LazyLock::new(FieldOptions::new);

/// 下拉選單的選項：類別、曝露度（雙語）、重要性。
struct FieldOptions {
    categories: Vec<Choice>,
    exposures: Vec<Choice>,
    levels: Vec<Choice>,
}

impl FieldOptions {
    fn new() -> FieldOptions {
        let choice = |value: &str, text: String| Choice {
            value: value.to_string(),
            text,
        };
        FieldOptions {
            categories: Category::ALL
                .iter()
                .map(|c| choice(c.as_str(), category_label(*c)))
                .collect(),
            exposures: Exposure::ALL
                .iter()
                .map(|e| choice(e.as_str(), exposure_label(*e)))
                .collect(),
            levels: (MIN_CRITICALITY..=MAX_CRITICALITY)
                .map(|n| choice(&n.to_string(), n.to_string()))
                .collect(),
        }
    }
}

fn fields(ui: &mut egui::Ui, edit: &mut AssetEdit) {
    let errors = edit.errors.clone();
    let options = &*FIELD_OPTIONS;
    let f = &mut edit.form;
    egui::Grid::new("asset-fields")
        .num_columns(2)
        .show(ui, |ui| {
            use StandardField as F;
            text_field(ui, &mut f.asset_tag, &errors, F::AssetTag);
            if let (Some(old), Some(_)) = (f.original_tag(), f.renamed_to()) {
                ui.label("");
                ui.scope(|ui| {
                    ui.set_max_width(FIELD_WIDTH);
                    let note = egui::RichText::new(rename_note(old)).color(ALERT);
                    ui.add(egui::Label::new(note).wrap());
                });
                ui.end_row();
            }
            text_field(ui, &mut f.name, &errors, F::Name);
            choice_field(
                ui,
                &mut f.category,
                &options.categories,
                &errors,
                F::Category,
            );
            choice_field(
                ui,
                &mut f.exposure,
                &options.exposures,
                &errors,
                F::Exposure,
            );
            choice_field(
                ui,
                &mut f.criticality,
                &options.levels,
                &errors,
                F::Criticality,
            );
            text_field(ui, &mut f.location, &errors, F::Location);
            text_field(ui, &mut f.owner, &errors, F::Owner);
            text_field(ui, &mut f.notes, &errors, F::Notes);
        });
}
