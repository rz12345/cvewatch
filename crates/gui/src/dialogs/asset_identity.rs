//! 資產對話框的「識別與網路」區塊（20 spec GUI asset identity input、design D11）：
//! 廠牌、型號、序號、財產編號、主機名稱、管理介面位址，以及 IP 清單（每列「IP」「用途」與
//! 「移除」，下方「新增 IP」）。錯誤以介面用語顯示在欄位或 IP 列下方；完全空白的 IP 列儲存時
//! 忽略（frontend 的 `AssetForm::ip_inputs`）。

use cvewatch_core::assets::columns::StandardField;
use eframe::egui;

use super::asset_edit::{text_field, AssetEdit};
use crate::screens::ALERT;

/// 區塊標題。
pub const SECTION_TITLE: &str = "識別與網路";
/// 加入一列空白 IP。
pub const ADD_IP: &str = "新增 IP";
/// 移除一列 IP。
pub const REMOVE_IP: &str = "移除";
/// 用途欄位的標籤。
pub const PURPOSE_LABEL: &str = "用途";

const IP_WIDTH: f32 = 160.0;
const PURPOSE_WIDTH: f32 = 120.0;

/// IP 列的標籤（「IP 1」「IP 2」…；畫面與輔助技術用）。
pub fn ip_label(index: usize) -> String {
    format!("IP {}", index + 1)
}

pub(super) fn identity_fields(ui: &mut egui::Ui, edit: &mut AssetEdit) {
    let errors = edit.errors.clone();
    ui.add_space(4.0);
    ui.strong(SECTION_TITLE);
    let f = &mut edit.form;
    egui::Grid::new("asset-identity-fields")
        .num_columns(2)
        .show(ui, |ui| {
            use StandardField as F;
            text_field(ui, &mut f.brand, &errors, F::Brand);
            text_field(ui, &mut f.model, &errors, F::Model);
            text_field(ui, &mut f.serial_number, &errors, F::SerialNumber);
            text_field(ui, &mut f.property_number, &errors, F::PropertyNumber);
            text_field(ui, &mut f.hostname, &errors, F::Hostname);
            text_field(ui, &mut f.management_url, &errors, F::ManagementUrl);
        });
    ui.add_space(4.0);
    let focus = edit.focus_ip.take();
    let mut remove = None;
    for (i, draft) in f.ips.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let l = ui.label(ip_label(i));
            let address = ui
                .add(
                    egui::TextEdit::singleline(&mut draft.address)
                        .id_salt(("asset-ip", i))
                        .desired_width(IP_WIDTH),
                )
                .labelled_by(l.id);
            if focus == Some(i) {
                address.request_focus();
            }
            let p = ui.label(PURPOSE_LABEL);
            ui.add(
                egui::TextEdit::singleline(&mut draft.purpose)
                    .id_salt(("asset-ip-purpose", i))
                    .desired_width(PURPOSE_WIDTH),
            )
            .labelled_by(p.id);
            if ui.button(REMOVE_IP).clicked() {
                remove = Some(i);
            }
        });
        for msg in errors.ip_row(i) {
            ui.colored_label(ALERT, msg);
        }
    }
    if let Some(i) = remove {
        f.remove_ip(i);
        // 錯誤以列索引定位，移除後不再對應
        edit.errors.ips.clear();
    }
    if ui.button(ADD_IP).clicked() {
        edit.focus_ip = Some(f.add_ip());
    }
}
