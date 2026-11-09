//! 「資產說明…」對話框（16 spec GUI asset system management、design D8）：列出代碼、名稱與
//! 歸屬的資產數（含已退役），可新增、改名與刪除。代碼只在新增時可輸入；仍有資產歸屬時
//! 「刪除」停用並以提示說明資產數。欄位錯誤顯示在欄位下方，對話框不關閉。

use cvewatch_core::assets::systems::{SystemError, SystemField};
use cvewatch_core::assets::SystemCode;
use cvewatch_frontend::workspace::systems::{delete_blocked, field_error, SystemSaveError};
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

/// 改名欄位的標籤。
pub fn rename_label(code: &SystemCode) -> String {
    format!("{code} 的名稱")
}

/// 對話框狀態。
#[derive(Default)]
pub struct SystemsDialog {
    pub new_code: String,
    pub new_name: String,
    /// 新增列的欄位錯誤
    pub new_errors: Vec<SystemError>,
    /// 正在改名的資產說明與輸入中的名稱
    pub renaming: Option<(SystemCode, String)>,
    pub rename_errors: Vec<SystemError>,
}

enum Action {
    Create,
    StartRename(SystemCode, String),
    SaveRename,
    CancelRename,
    Delete(SystemCode),
    Close,
}

impl GuiApp {
    /// 開啟「資產說明…」。
    pub fn start_systems_dialog(&mut self) {
        self.asset_dialog = Some(AssetDialog::Systems(Box::default()));
    }

    pub(super) fn show_systems_dialog(&mut self, ctx: &egui::Context, mut d: Box<SystemsDialog>) {
        let mut action = None;
        egui::Modal::new(egui::Id::unique("dialog-asset-systems")).show(ctx, |ui| {
            ui.heading(TITLE);
            let listed = self.systems_table(ui, &mut d);
            ui.separator();
            let added = new_row(ui, &mut d);
            ui.separator();
            let closed = ui.button("關閉").clicked().then_some(Action::Close);
            action = listed.or(added).or(closed);
        });
        if action.is_none() && d.renaming.is_none() && escape_pressed(ctx) {
            action = Some(Action::Close);
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

    /// 清單：代碼、名稱（改名中為輸入框）、資產數、操作。
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
                    ui.label(s.code.as_str());
                    match &mut d.renaming {
                        Some((code, text)) if *code == s.code => {
                            rename_cell(ui, code, text, &d.rename_errors, &mut action);
                        }
                        _ => {
                            ui.label(one_line(&s.name));
                            ui.label(used.to_string());
                            ui.horizontal(|ui| {
                                if ui.button(format!("改名 {}", s.code)).clicked() {
                                    action =
                                        Some(Action::StartRename(s.code.clone(), s.name.clone()));
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
            Action::StartRename(code, name) => {
                d.renaming = Some((code, name));
                d.rename_errors.clear();
            }
            Action::CancelRename => {
                d.renaming = None;
                d.rename_errors.clear();
            }
            Action::SaveRename => {
                if let Some((code, name)) = d.renaming.clone() {
                    match self.ws.rename_system(&code, &name, &open) {
                        Ok(r) => {
                            d.renaming = None;
                            d.rename_errors.clear();
                            self.apply_refreshed(r);
                        }
                        Err(e) => self.system_error(e, &mut d.rename_errors),
                    }
                }
            }
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

    fn system_error(&mut self, e: SystemSaveError, errors: &mut Vec<SystemError>) {
        match e {
            SystemSaveError::Fields(fields) => *errors = fields,
            SystemSaveError::Failed(report) => {
                errors.clear();
                self.report(report);
            }
        }
    }
}

fn error_text(ui: &mut egui::Ui, errors: &[SystemError], field: SystemField) {
    if let Some(msg) = field_error(errors, field) {
        ui.colored_label(ALERT, msg);
    }
}

fn rename_cell(
    ui: &mut egui::Ui,
    code: &SystemCode,
    text: &mut String,
    errors: &[SystemError],
    action: &mut Option<Action>,
) {
    ui.vertical(|ui| {
        let l = ui.weak(rename_label(code));
        ui.add(egui::TextEdit::singleline(text).desired_width(200.0))
            .labelled_by(l.id);
        error_text(ui, errors, SystemField::Name);
    });
    ui.label("");
    ui.horizontal(|ui| {
        if ui.button("儲存名稱").clicked() {
            *action = Some(Action::SaveRename);
        }
        if ui.button("取消改名").clicked() {
            *action = Some(Action::CancelRename);
        }
    });
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
