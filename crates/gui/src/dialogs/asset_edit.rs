//! 「新增資產」與「編輯」共用的對話框（design D11）：上半部資產欄位（`asset_tag` 只在新增時
//! 可編輯；category、曝露度、重要性為下拉選單），下半部元件表格（套件元件唯讀）、未辨識產品的
//! 處置與可搜尋的「加入元件」。驗證錯誤以紅字顯示在欄位下方，失敗時保留對話框與輸入內容。

use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::{Category, Exposure, MAX_CRITICALITY, MIN_CRITICALITY};
use cvewatch_frontend::asset_form::{
    product_choices, search_products, AssetForm, ComponentDraft, FormErrors, FormMode,
    ProductChoice, UnresolvedAction,
};
use cvewatch_frontend::Level;
use eframe::egui;

use super::{escape_pressed, AssetDialog};
use crate::app::GuiApp;
use crate::screens::ALERT;
use crate::text::one_line;

/// 套件元件的說明（唯讀）。
pub const PACKAGE_NOTE: &str = "由套件清單管理";

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
}

impl AssetEdit {
    pub fn new(form: AssetForm) -> AssetEdit {
        AssetEdit {
            initial: form.clone(),
            form,
            errors: FormErrors::default(),
            failure: None,
            search: String::new(),
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
            action = body(ui, &mut edit, &products);
        });
        if action.is_none() && !popup_open && !edit.dirty() && escape_pressed(ctx) {
            action = Some(Action::Cancel);
        }
        match action {
            None => self.asset_dialog = Some(AssetDialog::Edit(edit)),
            Some(Action::Cancel) => {}
            Some(Action::Save) => self.save_asset(edit),
        }
    }

    /// 儲存：本地驗證失敗時只顯示錯誤；寫入失敗時保留對話框與輸入內容；成功時關閉並選取該資產。
    fn save_asset(&mut self, mut edit: Box<AssetEdit>) {
        edit.failure = None;
        if let Err(errors) = edit.form.validate() {
            edit.errors = errors;
            self.asset_dialog = Some(AssetDialog::Edit(edit));
            return self.report(cvewatch_frontend::Report::error("請修正標示的欄位"));
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

fn body(ui: &mut egui::Ui, edit: &mut AssetEdit, products: &Products) -> Option<Action> {
    let title = match edit.form.mode {
        FormMode::New => "新增資產".to_string(),
        FormMode::Edit => format!("編輯 {}", one_line(&edit.form.asset_tag)),
    };
    ui.heading(title);
    egui::ScrollArea::vertical()
        .max_height(560.0)
        .show(ui, |ui| {
            fields(ui, edit);
            ui.separator();
            components(ui, edit);
            unresolved(ui, edit, &products.all);
            add_component(ui, edit, &products.matching);
        });
    ui.separator();
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

fn text_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    enabled: bool,
    errors: &FormErrors,
    field: StandardField,
) {
    let l = ui.label(label);
    ui.add_enabled(
        enabled,
        egui::TextEdit::singleline(value).desired_width(FIELD_WIDTH),
    )
    .labelled_by(l.id);
    ui.end_row();
    if errors.field(field).is_some() {
        ui.label("");
        field_error(ui, errors, field);
        ui.end_row();
    }
}

fn choice_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    options: &[&str],
    errors: &FormErrors,
    field: StandardField,
) {
    ui.label(label);
    let shown = if value.is_empty() {
        "（請選擇）".to_string()
    } else {
        value.clone()
    };
    egui::ComboBox::from_label(format!("選擇{label}"))
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for o in options {
                if ui.selectable_label(value.as_str() == *o, *o).clicked() {
                    *value = o.to_string();
                }
            }
        });
    ui.end_row();
    if errors.field(field).is_some() {
        ui.label("");
        field_error(ui, errors, field);
        ui.end_row();
    }
}

/// 下拉選單的選項：類別、曝露度、重要性。
struct FieldOptions {
    categories: Vec<&'static str>,
    exposures: Vec<&'static str>,
    levels: Vec<String>,
}

impl FieldOptions {
    fn new() -> FieldOptions {
        FieldOptions {
            categories: Category::ALL.iter().map(|c| c.as_str()).collect(),
            exposures: Exposure::ALL.iter().map(|e| e.as_str()).collect(),
            levels: (MIN_CRITICALITY..=MAX_CRITICALITY)
                .map(|n| n.to_string())
                .collect(),
        }
    }
}

fn fields(ui: &mut egui::Ui, edit: &mut AssetEdit) {
    let new = edit.form.mode == FormMode::New;
    let errors = edit.errors.clone();
    let options = FieldOptions::new();
    let f = &mut edit.form;
    egui::Grid::new("asset-fields")
        .num_columns(2)
        .show(ui, |ui| {
            use StandardField as F;
            text_field(ui, "資產編號", &mut f.asset_tag, new, &errors, F::AssetTag);
            text_field(ui, "名稱", &mut f.name, true, &errors, F::Name);
            choice_fields(ui, f, &options, &errors);
            text_field(ui, "位置", &mut f.location, true, &errors, F::Location);
            text_field(ui, "負責人", &mut f.owner, true, &errors, F::Owner);
            text_field(ui, "備註", &mut f.notes, true, &errors, F::Notes);
        });
}

fn choice_fields(ui: &mut egui::Ui, f: &mut AssetForm, o: &FieldOptions, errors: &FormErrors) {
    let levels: Vec<&str> = o.levels.iter().map(String::as_str).collect();
    choice_field(
        ui,
        "類別",
        &mut f.category,
        &o.categories,
        errors,
        StandardField::Category,
    );
    choice_field(
        ui,
        "曝露度",
        &mut f.exposure,
        &o.exposures,
        errors,
        StandardField::Exposure,
    );
    choice_field(
        ui,
        "重要性",
        &mut f.criticality,
        &levels,
        errors,
        StandardField::Criticality,
    );
}

fn components(ui: &mut egui::Ui, edit: &mut AssetEdit) {
    ui.label(egui::RichText::new("元件").strong());
    if edit.form.components.is_empty() {
        ui.label("尚無元件，請由下方「加入元件」選擇產品。");
    }
    let errors = edit.errors.clone();
    let mut dropped = None;
    egui::Grid::new("asset-components")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            for (i, d) in edit.form.components.iter_mut().enumerate() {
                if component_row(ui, d) {
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

/// 一列元件；回傳 true 表示要拿掉這個尚未儲存的新元件。
fn component_row(ui: &mut egui::Ui, d: &mut ComponentDraft) -> bool {
    let name = one_line(&d.product_key);
    if d.package {
        ui.label(name);
        ui.label(one_line(&d.version));
        ui.weak(PACKAGE_NOTE);
        return false;
    }
    let l = ui.label(format!("{name} 版本"));
    if d.removed {
        ui.weak(format!("{}（儲存時移除）", one_line(&d.version)));
        if ui.button(format!("復原 {name}")).clicked() {
            d.removed = false;
        }
        return false;
    }
    ui.add(
        egui::TextEdit::singleline(&mut d.version)
            .id_salt(("component-version", &d.product_key))
            .desired_width(180.0),
    )
    .labelled_by(l.id);
    if ui.button(format!("移除 {name}")).clicked() {
        if d.id.is_none() {
            return true;
        }
        d.removed = true;
    }
    false
}

fn unresolved(ui: &mut egui::Ui, edit: &mut AssetEdit, products: &[ProductChoice]) {
    if edit.form.unresolved.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.label(egui::RichText::new("未辨識產品").strong());
    let errors = edit.errors.clone();
    for (i, u) in edit.form.unresolved.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let raw = one_line(&u.product_raw);
            let current = match &u.action {
                UnresolvedAction::Keep => "保留".to_string(),
                UnresolvedAction::MapTo(key) => format!("改為 {key}"),
                UnresolvedAction::Delete => "儲存時刪除".to_string(),
            };
            egui::ComboBox::from_label(format!("{raw} {} 處置", one_line(&u.version_raw)))
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
                })
                .response
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

fn add_component(ui: &mut egui::Ui, edit: &mut AssetEdit, choices: &[ProductChoice]) {
    ui.add_space(6.0);
    let mut picked = None;
    egui::ComboBox::from_label("加入元件")
        .selected_text("選擇目錄產品…")
        .height(320.0)
        .show_ui(ui, |ui| {
            let l = ui.label("搜尋產品");
            ui.add(egui::TextEdit::singleline(&mut edit.search).desired_width(220.0))
                .labelled_by(l.id);
            for c in choices {
                if ui.selectable_label(false, c.label()).clicked() {
                    picked = Some(c.key.clone());
                }
            }
            if choices.is_empty() {
                ui.label("沒有相符的產品");
            }
        });
    if let Some(key) = picked {
        edit.form.add_product(&key);
        edit.search.clear();
    }
}
