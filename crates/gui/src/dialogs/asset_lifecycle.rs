//! 資產對話框的「生命週期」區塊與元件的「EoL 覆寫…」小對話框（22 spec GUI lifecycle fields
//! input、design D11）。
//!
//! 區塊：保固到期、維護合約到期、軟體授權到期與硬體 EoL 覆寫（日期與來源），日期寫法與清冊相同
//! （`YYYY-MM-DD`、`YYYY/MM/DD`），錯誤以紅字顯示在欄位下方（解析與驗證在 frontend 的
//! `asset_form::lifecycle`）。小對話框編輯一個清冊元件的軟體 EoL 覆寫（日期與來源）：「套用」時先
//! 檢查並把錯誤標在欄位下方，「清除覆寫」清空兩欄（儲存後改由目錄或 endoflife.date 決定），
//! 「取消」或 Esc 不改變元件。覆寫在按下資產對話框的「儲存」時才寫入。

use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::catalog::Catalog;
use cvewatch_frontend::asset_form::{field_label, EolDraft, FormErrors};
use eframe::egui;

use super::asset_edit::AssetEdit;
use super::escape_pressed;
use crate::screens::ALERT;
use crate::text::one_line;

/// 區塊標題。
pub const SECTION_TITLE: &str = "生命週期";
/// 日期寫法的說明。
pub const DATE_HINT: &str = "日期寫法：YYYY-MM-DD 或 YYYY/MM/DD；硬體 EoL 覆寫需填來源";
/// 清除硬體 EoL 覆寫（日期與來源）。
pub const CLEAR_HARDWARE: &str = "清除硬體覆寫";
/// 元件列開啟小對話框的按鈕。
pub const EOL_OVERRIDE_BUTTON: &str = "EoL 覆寫…";
/// 小對話框的日期欄。
pub const COMPONENT_EOL_DATE: &str = "軟體 EoL 日期";
/// 小對話框的來源欄。
pub const COMPONENT_EOL_SOURCE: &str = "EoL 覆寫來源";
/// 小對話框：檢查後寫回元件。
pub const EOL_APPLY: &str = "套用";
/// 小對話框：清空覆寫並關閉。
pub const EOL_CLEAR: &str = "清除覆寫";

const FIELD_WIDTH: f32 = 140.0;

/// 小對話框的標題。
pub fn component_eol_title(name: &str) -> String {
    format!("{} 的軟體 EoL 覆寫", one_line(name))
}

/// 元件列在按鈕後顯示的目前覆寫（「EoL 2028-12-31（延長支援合約）」）；沒有覆寫時為 None。
pub fn override_summary(eol: &EolDraft) -> Option<String> {
    let date = eol.date.trim();
    let source = eol.source.trim();
    match (date.is_empty(), source.is_empty()) {
        (true, true) => None,
        (false, true) => Some(format!("EoL {date}")),
        _ => Some(format!("EoL {date}（{source}）")),
    }
}

/// 編輯中的元件 EoL 覆寫（小對話框的狀態；套用前不影響表單）。
pub struct ComponentEol {
    /// 元件在表單中的索引
    pub index: usize,
    /// 產品顯示名稱
    pub name: String,
    pub draft: EolDraft,
    /// 上一次「套用」的錯誤（欄位、訊息）
    pub error: Option<(StandardField, String)>,
}

impl ComponentEol {
    pub fn open(edit: &AssetEdit, index: usize, catalog: &Catalog) -> Option<ComponentEol> {
        let d = edit.form.components.get(index)?;
        Some(ComponentEol {
            index,
            name: d.name(catalog),
            draft: d.eol.clone(),
            error: None,
        })
    }
}

/// 「生命週期」區塊（資產對話框捲動區的最後、「加入元件」之後：上方三欄已接近捲動區高度，
/// 放在上方會把元件區推出畫面）。兩列：三個到期日；硬體 EoL 覆寫與來源。錯誤在欄位下方。
pub(super) fn lifecycle_fields(ui: &mut egui::Ui, edit: &mut AssetEdit) {
    let errors = edit.errors.clone();
    ui.separator();
    ui.strong(SECTION_TITLE);
    let l = &mut edit.form.lifecycle;
    egui::Grid::new("asset-lifecycle-fields")
        .num_columns(6)
        .show(ui, |ui| {
            use StandardField as F;
            let dates = [
                (F::WarrantyEnd, &mut l.warranty_end),
                (F::SupportEnd, &mut l.support_end),
                (F::LicenseEnd, &mut l.license_end),
            ];
            field_row(ui, dates, &errors);
            let hardware = [
                (F::Eol, &mut l.hardware_date),
                (F::Source, &mut l.hardware_source),
            ];
            field_row(ui, hardware, &errors);
        });
    ui.horizontal(|ui| {
        let has_override =
            !l.hardware_date.trim().is_empty() || !l.hardware_source.trim().is_empty();
        if has_override && ui.button(CLEAR_HARDWARE).clicked() {
            l.clear_hardware_override();
        }
        ui.weak(DATE_HINT);
    });
}

/// 一列欄位（標籤、輸入框交替）；有錯誤時下一列在對應輸入框下方顯示紅字。
fn field_row<const N: usize>(
    ui: &mut egui::Ui,
    fields: [(StandardField, &mut String); N],
    errors: &FormErrors,
) {
    let mut messages = Vec::with_capacity(N);
    for (field, value) in fields {
        let l = ui.label(field_label(field));
        ui.add(
            egui::TextEdit::singleline(value)
                .id_salt(("asset-lifecycle", field.as_str()))
                .desired_width(FIELD_WIDTH),
        )
        .labelled_by(l.id);
        messages.push(errors.field(field));
    }
    ui.end_row();
    if messages.iter().all(Option::is_none) {
        return;
    }
    for msg in messages {
        ui.label("");
        match msg {
            Some(m) => ui.colored_label(ALERT, m),
            None => ui.label(""),
        };
    }
    ui.end_row();
}

/// 小對話框這一幀的結果。
enum Outcome {
    Apply,
    Clear,
    Cancel,
}

/// 開啟中的元件 EoL 覆寫小對話框（在資產對話框之上）。
pub(super) fn component_eol_dialog(ctx: &egui::Context, edit: &mut AssetEdit) {
    let Some(mut state) = edit.eol_dialog.take() else {
        return;
    };
    let mut outcome = None;
    egui::Modal::new(egui::Id::unique("dialog-component-eol")).show(ctx, |ui| {
        outcome = eol_body(ui, &mut state);
    });
    if outcome.is_none() && escape_pressed(ctx) {
        outcome = Some(Outcome::Cancel);
    }
    match outcome {
        None => edit.eol_dialog = Some(state),
        Some(Outcome::Cancel) => {}
        Some(Outcome::Clear) => {
            if let Some(d) = edit.form.components.get_mut(state.index) {
                d.eol.clear();
            }
        }
        Some(Outcome::Apply) => match state.draft.parsed() {
            Err(e) => {
                state.error = Some(e);
                edit.eol_dialog = Some(state);
            }
            Ok(_) => {
                if let Some(d) = edit.form.components.get_mut(state.index) {
                    d.eol.date = state.draft.date.trim().to_string();
                    d.eol.source = state.draft.source.trim().to_string();
                }
            }
        },
    }
}

fn eol_body(ui: &mut egui::Ui, state: &mut ComponentEol) -> Option<Outcome> {
    ui.heading(component_eol_title(&state.name));
    egui::Grid::new("component-eol-fields")
        .num_columns(2)
        .show(ui, |ui| {
            let error = state.error.as_ref();
            eol_field(ui, COMPONENT_EOL_DATE, &mut state.draft.date, error, |f| {
                f == StandardField::Eol
            });
            eol_field(
                ui,
                COMPONENT_EOL_SOURCE,
                &mut state.draft.source,
                error,
                |f| f == StandardField::Source,
            );
        });
    ui.weak("覆寫優先於目錄與 endoflife.date；清除後改由它們決定");
    let mut outcome = None;
    ui.horizontal(|ui| {
        if ui.button(EOL_APPLY).clicked() {
            outcome = Some(Outcome::Apply);
        }
        if ui.button(EOL_CLEAR).clicked() {
            outcome = Some(Outcome::Clear);
        }
        if ui.button("取消").clicked() {
            outcome = Some(Outcome::Cancel);
        }
    });
    outcome
}

/// 小對話框的一個欄位與其下方的錯誤。
fn eol_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    error: Option<&(StandardField, String)>,
    is_field: impl Fn(StandardField) -> bool,
) {
    let l = ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(FIELD_WIDTH))
        .labelled_by(l.id);
    ui.end_row();
    if let Some((_, msg)) = error.filter(|(f, _)| is_field(*f)) {
        ui.label("");
        ui.colored_label(ALERT, msg);
        ui.end_row();
    }
}
