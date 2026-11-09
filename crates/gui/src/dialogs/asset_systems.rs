//! 「新增資產說明」開啟的資產說明對話框（16 spec GUI asset system management、design D8；
//! 19 design D6）：列出代碼、名稱與歸屬的資產數（含已退役），可新增、編輯與刪除。「編輯」
//! 在同一列修改代碼與名稱後一次儲存；代碼改變且有資產歸屬時，對話框內容換成確認畫面（受影響
//! 資產數、ISMS 編號與資產編號的變更、[`RECODE_NOTE`]），按「確定修改」才寫入。仍有資產
//! 歸屬時「刪除」停用並以提示說明資產數。欄位錯誤顯示在欄位下方，對話框不關閉。

use cvewatch_core::assets::systems::{SystemError, SystemField};
use cvewatch_core::assets::systems_edit::{SystemEdit, SystemEditPlan, PLAN_CHANGED};
use cvewatch_core::assets::SystemCode;
use cvewatch_frontend::workspace::systems::{
    affected_text, delete_blocked, field_error, field_errors, SystemEdited, SystemSaveError,
    RECODE_NOTE,
};
use cvewatch_frontend::Report;
use eframe::egui;

use super::{escape_pressed, AssetDialog};
use crate::app::GuiApp;
use crate::screens::ALERT;
use crate::text::one_line;

/// 對話框標題。
pub const TITLE: &str = "資產說明";
/// 新增列的欄位標籤（與資產清單的「名稱」欄區分）。
pub const NEW_CODE_LABEL: &str = "新資產說明代碼";
pub const NEW_NAME_LABEL: &str = "新資產說明名稱";
/// 編輯列的按鈕。
pub const SAVE_EDIT: &str = "儲存";
pub const CANCEL_EDIT: &str = "取消編輯";
/// 確認畫面的按鈕。
pub const CONFIRM_EDIT: &str = "確定修改";
pub const BACK_TO_EDIT: &str = "返回";
/// 確認畫面變更清單的最大高度。
const CHANGES_MAX_HEIGHT: f32 = 240.0;

/// 列上的「編輯」按鈕。
pub fn edit_button(code: &SystemCode) -> String {
    format!("編輯 {code}")
}

/// 編輯列代碼欄的標籤。
pub fn code_label(code: &SystemCode) -> String {
    format!("{code} 的代碼")
}

/// 編輯列名稱欄的標籤。
pub fn rename_label(code: &SystemCode) -> String {
    format!("{code} 的名稱")
}

/// 確認畫面的標題。
pub fn confirm_title(plan: &SystemEditPlan) -> String {
    format!(
        "將資產說明代碼 {} 改為 {}？",
        plan.before.code, plan.after.code
    )
}

/// 對話框狀態。
#[derive(Default)]
pub struct SystemsDialog {
    pub new_code: String,
    pub new_name: String,
    /// 新增列的欄位錯誤
    pub new_errors: Vec<SystemError>,
    /// 正在編輯的資產說明（原代碼與輸入中的代碼、名稱）
    pub editing: Option<SystemEdit>,
    pub edit_errors: Vec<SystemError>,
    /// 代碼改變且有資產歸屬時的確認畫面
    pub confirm: Option<SystemEditPlan>,
}

enum Action {
    Create,
    StartEdit(SystemCode, String),
    SaveEdit,
    CancelEdit,
    ConfirmEdit,
    BackToEdit,
    Delete(SystemCode),
    Close,
}

impl GuiApp {
    /// 開啟資產說明對話框（工具列的「新增資產說明」）。
    pub fn start_systems_dialog(&mut self) {
        self.asset_dialog = Some(AssetDialog::Systems(Box::default()));
    }

    pub(super) fn show_systems_dialog(&mut self, ctx: &egui::Context, mut d: Box<SystemsDialog>) {
        let mut action = None;
        egui::Modal::new(egui::Id::unique("dialog-asset-systems")).show(ctx, |ui| {
            ui.heading(TITLE);
            if let Some(plan) = &d.confirm {
                action = confirm_view(ui, plan);
                return;
            }
            let listed = self.systems_table(ui, &mut d);
            ui.separator();
            let added = new_row(ui, &mut d);
            ui.separator();
            let closed = ui.button("關閉").clicked().then_some(Action::Close);
            action = listed.or(added).or(closed);
        });
        if action.is_none() && escape_pressed(ctx) {
            // 確認畫面：Esc 回到編輯列；編輯列存在時 Esc 不關閉對話框
            if d.confirm.is_some() {
                action = Some(Action::BackToEdit);
            } else if d.editing.is_none() {
                action = Some(Action::Close);
            }
        }
        let keep = match action {
            None => true,
            Some(Action::Close) => false,
            Some(a) => self.apply_systems_action(&mut d, a),
        };
        if keep {
            self.asset_dialog = Some(AssetDialog::Systems(d));
        }
    }

    /// 清單：代碼、名稱（編輯中為輸入框）、資產數、操作。
    fn systems_table(&self, ui: &mut egui::Ui, d: &mut SystemsDialog) -> Option<Action> {
        let mut action = None;
        if self.ws.data.systems.is_empty() {
            ui.label("尚無資產說明。");
            return None;
        }
        egui::Grid::new("asset-systems")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                for name in ["代碼", "名稱", "資產數", ""] {
                    ui.strong(name);
                }
                ui.end_row();
                for s in self.ws.data.systems.all() {
                    let used = self.ws.system_usage(&s.code);
                    match &mut d.editing {
                        Some(row) if row.code == s.code => {
                            edit_cells(ui, row, used, &d.edit_errors, &mut action);
                        }
                        _ => {
                            ui.label(s.code.as_str());
                            ui.label(one_line(&s.name));
                            ui.label(used.to_string());
                            ui.horizontal(|ui| {
                                if ui.button(edit_button(&s.code)).clicked() {
                                    action =
                                        Some(Action::StartEdit(s.code.clone(), s.name.clone()));
                                }
                                let delete = ui
                                    .add_enabled(
                                        used == 0,
                                        egui::Button::new(format!("刪除 {}", s.code)),
                                    )
                                    .on_disabled_hover_text(delete_blocked(used));
                                if delete.clicked() {
                                    action = Some(Action::Delete(s.code.clone()));
                                }
                            });
                        }
                    }
                    ui.end_row();
                }
            });
        action
    }

    /// 執行操作；回傳對話框是否保留。
    fn apply_systems_action(&mut self, d: &mut SystemsDialog, action: Action) -> bool {
        let open = self.open_ids();
        match action {
            Action::Close => return false,
            Action::StartEdit(code, name) => {
                d.editing = Some(SystemEdit {
                    new_code: code.to_string(),
                    code,
                    name,
                });
                d.edit_errors.clear();
                d.confirm = None;
            }
            Action::CancelEdit => {
                d.editing = None;
                d.edit_errors.clear();
                d.confirm = None;
            }
            Action::SaveEdit => self.save_system_edit(d, &open),
            Action::ConfirmEdit => {
                if let Some(plan) = d.confirm.take() {
                    self.write_system_edit(d, Some(&plan), &open);
                }
            }
            Action::BackToEdit => d.confirm = None,
            Action::Create => match self.ws.create_system(&d.new_code, &d.new_name, &open) {
                Ok(r) => {
                    d.new_code.clear();
                    d.new_name.clear();
                    d.new_errors.clear();
                    self.apply_refreshed(r);
                }
                Err(e) => self.system_error(e, &mut d.new_errors),
            },
            Action::Delete(code) => {
                let r = self.ws.delete_system(&code, &open);
                self.apply_refreshed(r);
            }
        }
        true
    }

    /// 「儲存」：只改名稱時直接寫入；否則先預覽，沒有資產歸屬時寫入，有資產時顯示確認畫面。
    fn save_system_edit(&mut self, d: &mut SystemsDialog, open: &[i64]) {
        let Some(edit) = &d.editing else { return };
        if SystemCode::parse(&edit.new_code).as_ref() == Some(&edit.code) {
            return self.write_system_edit(d, None, open);
        }
        match self.ws.preview_system_edit(edit) {
            Ok(plan) if plan.assets.is_empty() => self.write_system_edit(d, Some(&plan), open),
            Ok(plan) => {
                d.edit_errors.clear();
                d.confirm = Some(plan);
            }
            Err(e) => self.system_error(e, &mut d.edit_errors),
        }
    }

    /// 寫入（core 在交易內重新計算；此時出現衝突則回到編輯列並標示錯誤；與確認的計畫不同時
    /// 以新計畫重新顯示確認）。
    fn write_system_edit(
        &mut self,
        d: &mut SystemsDialog,
        confirmed: Option<&SystemEditPlan>,
        open: &[i64],
    ) {
        let Some(edit) = &d.editing else { return };
        match self.ws.edit_system(edit, confirmed, open) {
            Ok(SystemEdited { refreshed, renamed }) => {
                d.editing = None;
                d.edit_errors.clear();
                self.follow_renamed(&renamed);
                self.apply_refreshed(refreshed);
            }
            Err(SystemSaveError::Changed(plan)) => {
                d.edit_errors.clear();
                d.confirm = Some(*plan);
                self.report(Report::warning(PLAN_CHANGED));
            }
            Err(e) => self.system_error(e, &mut d.edit_errors),
        }
    }

    /// 選取中的資產被改了編號時，改選新編號。
    fn follow_renamed(&mut self, renamed: &[(String, String)]) {
        let Some(selected) = &self.assets_selected else {
            return;
        };
        if let Some((_, to)) = renamed.iter().find(|(from, _)| from == selected) {
            self.assets_selected = Some(to.clone());
        }
    }

    fn system_error(&mut self, e: SystemSaveError, errors: &mut Vec<SystemError>) {
        match e {
            SystemSaveError::Fields(fields) => *errors = fields,
            // 只有帶確認計畫的寫入會回報，由 write_system_edit 處理
            SystemSaveError::Changed(_) => errors.clear(),
            SystemSaveError::Failed(report) => {
                errors.clear();
                self.report(report);
            }
        }
    }
}

fn error_text(ui: &mut egui::Ui, errors: &[SystemError], field: SystemField) {
    if let Some(msg) = field_errors(errors, field) {
        ui.colored_label(ALERT, msg);
    }
}

/// 編輯中的列：代碼欄、名稱欄（各自顯示錯誤）、資產數、「儲存」「取消編輯」。
fn edit_cells(
    ui: &mut egui::Ui,
    row: &mut SystemEdit,
    used: u32,
    errors: &[SystemError],
    action: &mut Option<Action>,
) {
    ui.vertical(|ui| {
        let l = ui.weak(code_label(&row.code));
        ui.add(egui::TextEdit::singleline(&mut row.new_code).desired_width(80.0))
            .labelled_by(l.id);
        error_text(ui, errors, SystemField::Code);
    });
    ui.vertical(|ui| {
        let l = ui.weak(rename_label(&row.code));
        ui.add(egui::TextEdit::singleline(&mut row.name).desired_width(200.0))
            .labelled_by(l.id);
        error_text(ui, errors, SystemField::Name);
    });
    ui.label(used.to_string());
    ui.horizontal(|ui| {
        if ui.button(SAVE_EDIT).clicked() {
            *action = Some(Action::SaveEdit);
        }
        if ui.button(CANCEL_EDIT).clicked() {
            *action = Some(Action::CancelEdit);
        }
    });
}

/// 確認畫面：受影響資產數、編號變更與提醒。
fn confirm_view(ui: &mut egui::Ui, plan: &SystemEditPlan) -> Option<Action> {
    ui.strong(confirm_title(plan));
    ui.label(format!(
        "資產說明 {} → {}",
        plan.before.label(),
        plan.after.label()
    ));
    ui.label(affected_text(plan));
    egui::ScrollArea::vertical()
        .max_height(CHANGES_MAX_HEIGHT)
        .show(ui, |ui| {
            change_list(ui, "ISMS 編號：", &plan.isms_changes());
            change_list(ui, "資產編號：", &plan.tag_changes());
        });
    ui.colored_label(ALERT, RECODE_NOTE);
    ui.separator();
    let mut action = None;
    ui.horizontal(|ui| {
        if ui.button(CONFIRM_EDIT).clicked() {
            action = Some(Action::ConfirmEdit);
        }
        if ui.button(BACK_TO_EDIT).clicked() {
            action = Some(Action::BackToEdit);
        }
    });
    action
}

fn change_list(ui: &mut egui::Ui, heading: &str, changes: &[(&str, &str)]) {
    if changes.is_empty() {
        return;
    }
    ui.strong(heading);
    for (from, to) in changes {
        ui.label(format!("  {from} → {to}"));
    }
}

/// 新增列：代碼、名稱、「新增」。
fn new_row(ui: &mut egui::Ui, d: &mut SystemsDialog) -> Option<Action> {
    let mut action = None;
    ui.strong("新增資產說明");
    egui::Grid::new("asset-systems-new")
        .num_columns(2)
        .show(ui, |ui| {
            let l = ui.label(NEW_CODE_LABEL);
            ui.add(egui::TextEdit::singleline(&mut d.new_code).desired_width(80.0))
                .labelled_by(l.id);
            ui.end_row();
            if field_error(&d.new_errors, SystemField::Code).is_some() {
                ui.label("");
                error_text(ui, &d.new_errors, SystemField::Code);
                ui.end_row();
            }
            let l = ui.label(NEW_NAME_LABEL);
            ui.add(egui::TextEdit::singleline(&mut d.new_name).desired_width(200.0))
                .labelled_by(l.id);
            ui.end_row();
            if field_error(&d.new_errors, SystemField::Name).is_some() {
                ui.label("");
                error_text(ui, &d.new_errors, SystemField::Name);
                ui.end_row();
            }
        });
    if ui.button("新增").clicked() {
        action = Some(Action::Create);
    }
    action
}
