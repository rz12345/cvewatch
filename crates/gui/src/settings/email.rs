//! Email 分頁：啟用、主機、連接埠、加密（只有 STARTTLS／TLS）、帳號、密碼、寄件者、多個收件者。

use cvewatch_core::notify::settings::SmtpSecurity;
use cvewatch_frontend::settings::form::Errors;
use cvewatch_frontend::settings::{FieldId, Form};
use eframe::egui;

use super::{error_row, secret_field, text_field, toggle_field, SettingsPage};

pub fn show(ui: &mut egui::Ui, page: &mut SettingsPage, errors: &Errors) {
    egui::Grid::new("settings-email")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            toggle_field(ui, page, FieldId::EmailEnabled);
            text_field(ui, page, FieldId::SmtpHost, errors);
            text_field(ui, page, FieldId::SmtpPort, errors);
            security(ui, page, errors);
            text_field(ui, page, FieldId::Username, errors);
            secret_field(ui, page, FieldId::Password);
            text_field(ui, page, FieldId::From, errors);
            recipients(ui, page, errors);
        });
}

/// 加密方式：只提供 STARTTLS 與 TLS（不提供無加密）。設定檔例外保留的無加密以文字標示。
fn security(ui: &mut egui::Ui, page: &mut SettingsPage, errors: &Errors) {
    ui.label(FieldId::Security.label());
    ui.horizontal(|ui| {
        let mut value = page.form.security;
        let a = ui.radio_value(&mut value, SmtpSecurity::Starttls, "STARTTLS");
        let b = ui.radio_value(&mut value, SmtpSecurity::Tls, "TLS");
        if a.changed() || b.changed() {
            page.form = Form {
                security: value,
                ..page.form.clone()
            };
        }
        if page.form.security == SmtpSecurity::None {
            ui.label("目前為設定檔例外保留的明文連線");
        }
    });
    ui.end_row();
    error_row(ui, FieldId::Security, errors);
}

fn recipients(ui: &mut egui::Ui, page: &mut SettingsPage, errors: &Errors) {
    ui.label(FieldId::To.label());
    let mut changed = false;
    let mut remove = None;
    let mut add = false;
    ui.vertical(|ui| {
        for (i, r) in page.recipients.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let label = ui.label(format!("{}.", i + 1));
                let edit = ui
                    .add(egui::TextEdit::singleline(r).desired_width(240.0))
                    .labelled_by(label.id);
                changed |= edit.changed();
                if ui.button(format!("刪除收件者 {}", i + 1)).clicked() {
                    remove = Some(i);
                }
            });
        }
        add = ui.button("新增收件者").clicked();
    });
    if let Some(i) = remove {
        page.recipients.remove(i);
        changed = true;
    }
    if add {
        page.recipients.push(String::new());
    }
    if changed {
        page.sync_recipients();
    }
    ui.end_row();
    error_row(ui, FieldId::To, errors);
}
