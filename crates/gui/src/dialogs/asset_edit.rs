//! 「新增資產元件」與「編輯」共用的對話框（design D11）：上半部資產欄位（類別、曝露度、機密性、
//! 完整性、可用性為下拉選單，即時顯示資產價值，13 design D8；編輯時資產編號可修改＝更名，
//! 與原編號不同時欄位下方提示影響，12 design D4），下半部元件與未辨識產品
//! （[`super::asset_components`]）。
//!
//! 每個欄位只有左側一個標籤，必填欄位標「*」；類別以「代碼 名稱」、曝露度以雙語呈現
//! （design 11 D3）。C/I/A 的第一項「依類別（N）」= 空白（儲存時帶入子類別範本值）。
//! 驗證錯誤以介面用語紅字顯示在欄位下方，失敗時保留對話框與輸入內容；本地驗證失敗不寫狀態列，
//! 以「取消」或 Esc 關閉時清除對話框留在狀態列的寫入失敗訊息（design 11 D5）。
//! ISMS 管理欄位（資產說明、單位、法規遵循性、個資、防護等級）在 [`super::asset_admin`]，
//! 識別欄位與 IP 在 [`super::asset_identity`]（20 design D11）。

use std::collections::HashMap;
use std::sync::LazyLock;

use cvewatch_core::assets::classes::{MAX_RATING, MIN_RATING};
use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::validate::{RATING_FIELDS, REQUIRED_FIELDS};
use cvewatch_core::assets::{AssetClasses, Exposure, SystemDirectory};
use cvewatch_core::catalog::Catalog;
use cvewatch_frontend::asset_form::{field_label, AssetForm, FormErrors, FormMode};
use cvewatch_frontend::display::{class_label, exposure_label};
use cvewatch_frontend::Level;
use eframe::egui;

use super::asset_admin::{admin_fields, system_choices};
use super::asset_components::{add_component, components, unresolved};
use super::asset_identity::identity_fields;
use super::product_picker::PickerState;
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
/// 下拉選單的最大高度：內建 18 個子類別不必捲動即可看到全部。
const COMBO_HEIGHT: f32 = 480.0;

/// 對話框狀態。
pub struct AssetEdit {
    pub form: AssetForm,
    /// 開啟時的內容（判斷是否有未儲存的修改）
    initial: AssetForm,
    pub errors: FormErrors,
    /// 寫入失敗的訊息（core 拒絕或中途失敗）
    pub failure: Option<String>,
    /// 「加入元件」選擇器的狀態（搜尋字、反白、是否開啟）
    pub add_picker: PickerState,
    /// 各未辨識產品（`asset_unresolved.id`）「改為…」選擇器的狀態
    pub unresolved_pickers: HashMap<i64, PickerState>,
    /// 下一幀讓這個元件列的版本欄取得焦點（剛加入的元件）
    pub focus_version: Option<usize>,
    /// 下一幀讓這個 IP 列的 IP 欄位取得焦點（剛按「新增 IP」）
    pub focus_ip: Option<usize>,
    /// 子類別選單（開啟時依設定檔建立一次，不在每一幀重新組字）
    class_options: Vec<Choice>,
    /// 資產說明選單（開啟時依資料建立一次）
    pub(super) system_options: Vec<Choice>,
}

impl AssetEdit {
    pub fn new(form: AssetForm, classes: &AssetClasses, systems: &SystemDirectory) -> AssetEdit {
        AssetEdit {
            class_options: class_choices(classes, &form.category),
            system_options: system_choices(systems, &form.system),
            initial: form.clone(),
            form,
            errors: FormErrors::default(),
            failure: None,
            add_picker: PickerState::default(),
            unresolved_pickers: HashMap::new(),
            focus_version: None,
            focus_ip: None,
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

impl GuiApp {
    /// 「新增資產元件」。
    pub fn start_new_asset(&mut self) {
        self.asset_dialog = Some(AssetDialog::Edit(Box::new(AssetEdit::new(
            AssetForm::new(),
            &self.ws.settings.classes,
            &self.ws.data.systems,
        ))));
    }

    /// 編輯所選的使用中資產。
    pub fn start_edit_asset(&mut self, tag: &str) {
        match self.ws.data.asset(tag) {
            Some(entry) => {
                let form = AssetForm::from_entry(entry);
                let edit = AssetEdit::new(form, &self.ws.settings.classes, &self.ws.data.systems);
                self.asset_dialog = Some(AssetDialog::Edit(Box::new(edit)));
            }
            None => self.report(cvewatch_frontend::Report::warning(format!(
                "找不到使用中的資產 {tag}，請重新載入"
            ))),
        }
    }

    pub(super) fn show_asset_edit(&mut self, ctx: &egui::Context, mut edit: Box<AssetEdit>) {
        let catalog = &self.ws.settings.catalog;
        let classes = &self.ws.settings.classes;
        let systems = &self.ws.data.systems;
        // Esc 先用來關閉下拉選單；有未儲存的修改時不以 Esc 關閉（避免丟失輸入，請按「取消」）
        let popup_open = egui::Popup::is_any_open(ctx);
        let mut action = None;
        egui::Modal::new(egui::Id::unique("dialog-asset-edit")).show(ctx, |ui| {
            action = body(ui, &mut edit, catalog, classes, systems);
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
    catalog: &Catalog,
    classes: &AssetClasses,
    systems: &SystemDirectory,
) -> Option<Action> {
    let title = match &edit.form.mode {
        FormMode::New => "新增資產".to_string(),
        FormMode::Edit { original_tag } => format!("編輯 {}", one_line(original_tag)),
    };
    ui.heading(title);
    egui::ScrollArea::vertical()
        .max_height(560.0)
        .show(ui, |ui| {
            // 一般欄位、ISMS 管理與識別與網路三欄並排，元件區不必捲動才看得到
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| fields(ui, edit, classes));
                // 水平排列中的 separator 會撐滿捲動區的高度，改用間距
                ui.add_space(16.0);
                ui.vertical(|ui| admin_fields(ui, edit, classes, systems));
                ui.add_space(16.0);
                ui.vertical(|ui| identity_fields(ui, edit));
            });
            ui.separator();
            components(ui, edit, catalog);
            unresolved(ui, edit, catalog);
            add_component(ui, edit, catalog);
        });
    ui.separator();
    let marked = !edit.errors.fields.is_empty()
        || !edit.errors.components.is_empty()
        || !edit.errors.unresolved.is_empty()
        || !edit.errors.ips.is_empty();
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

pub(super) fn text_field(
    ui: &mut egui::Ui,
    value: &mut String,
    errors: &FormErrors,
    field: StandardField,
) {
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
pub(super) fn choice_field(
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
    combo_field(ui, l.id, value, None, options, field, shown);
    if errors.field(field).is_some() {
        ui.label("");
        field_error(ui, errors, field);
        ui.end_row();
    }
}

/// 下拉選單本體（選項的 `value` 寫回 `value`）；`first` 列在 `options` 之前。
fn combo_field(
    ui: &mut egui::Ui,
    label: egui::Id,
    value: &mut String,
    first: Option<&Choice>,
    options: &[Choice],
    field: StandardField,
    shown: String,
) {
    let combo = egui::ComboBox::from_id_salt(("asset-field", field.as_str()))
        .selected_text(shown)
        .height(COMBO_HEIGHT)
        .show_ui(ui, |ui| {
            for o in first.into_iter().chain(options) {
                if ui
                    .selectable_label(value.as_str() == o.value, &o.text)
                    .clicked()
                {
                    *value = o.value.clone();
                }
            }
        });
    combo.response.labelled_by(label);
    ui.end_row();
}

/// 「依類別（2）」；子類別沒有範本值或類別未選時為「依類別（無預設）」。
fn template_text(template: Option<u8>) -> String {
    match template {
        Some(n) => format!("依類別（{n}）"),
        None => "依類別（無預設）".to_string(),
    }
}

/// C/I/A 下拉選單：第一項「依類別（N）」（空白），其後 1–4。
fn rating_field(
    ui: &mut egui::Ui,
    form: &mut AssetForm,
    field: StandardField,
    template: Option<u8>,
    errors: &FormErrors,
) {
    let l = ui.label(label_text(field));
    let by_class = Choice {
        value: String::new(),
        text: template_text(template),
    };
    let ratings = &FIELD_OPTIONS.ratings;
    let Some(value) = form.rating_mut(field) else {
        return;
    };
    let shown = match std::iter::once(&by_class)
        .chain(ratings)
        .find(|o| o.value == value.trim())
    {
        Some(o) => o.text.clone(),
        None => one_line(value),
    };
    combo_field(ui, l.id, value, Some(&by_class), ratings, field, shown);
    if errors.field(field).is_some() {
        ui.label("");
        field_error(ui, errors, field);
        ui.end_row();
    }
}

/// 子類別選單：已定義子類別依代碼排序，以「代碼 名稱」呈現；目前值未定義時另列「HW12（未定義）」。
fn class_choices(classes: &AssetClasses, current: &str) -> Vec<Choice> {
    let mut out: Vec<Choice> = classes
        .all()
        .iter()
        .map(|c| Choice {
            value: c.code.to_string(),
            text: c.label(),
        })
        .collect();
    let current = current.trim();
    if let Some(code) = cvewatch_core::assets::ClassCode::parse(current) {
        if classes.get(&code).is_none() {
            out.push(Choice {
                value: current.to_string(),
                text: class_label(&code, classes),
            });
        }
    }
    out
}

/// 下拉選單的一個選項：儲存的值（英文）與顯示文字。
pub(super) struct Choice {
    pub(super) value: String,
    pub(super) text: String,
}

/// 選項內容固定，只建立一次（不在每一幀重新組字）。
static FIELD_OPTIONS: LazyLock<FieldOptions> = LazyLock::new(FieldOptions::new);

/// 固定的下拉選單選項：曝露度（雙語）、C/I/A 1–4（子類別選項依設定檔，於開啟對話框時建立）。
struct FieldOptions {
    exposures: Vec<Choice>,
    ratings: Vec<Choice>,
}

impl FieldOptions {
    fn new() -> FieldOptions {
        let choice = |value: &str, text: String| Choice {
            value: value.to_string(),
            text,
        };
        FieldOptions {
            exposures: Exposure::ALL
                .iter()
                .map(|e| choice(e.as_str(), exposure_label(*e)))
                .collect(),
            ratings: (MIN_RATING..=MAX_RATING)
                .map(|n| choice(&n.to_string(), n.to_string()))
                .collect(),
        }
    }
}

/// 即時資產價值的標籤（缺任何一項時為「—」）。
pub fn asset_value_text(value: Option<u8>) -> String {
    value.map_or_else(|| "—".to_string(), |v| v.to_string())
}

fn fields(ui: &mut egui::Ui, edit: &mut AssetEdit, classes: &AssetClasses) {
    let errors = edit.errors.clone();
    let options = &*FIELD_OPTIONS;
    let categories = &edit.class_options;
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
            choice_field(ui, &mut f.category, categories, &errors, F::Category);
            choice_field(
                ui,
                &mut f.exposure,
                &options.exposures,
                &errors,
                F::Exposure,
            );
            // 每一幀只解析一次目前的子類別
            let template = f.class(classes).and_then(|c| c.template);
            for (field, _) in RATING_FIELDS {
                let t = template.and_then(|t| t.rating(field));
                rating_field(ui, f, field, t, &errors);
            }
            ui.label("資產價值");
            ui.label(asset_value_text(f.asset_value_with(template)));
            ui.end_row();
            text_field(ui, &mut f.isms_id, &errors, F::IsmsId);
            text_field(ui, &mut f.location, &errors, F::Location);
            text_field(ui, &mut f.notes, &errors, F::Notes);
        });
}
