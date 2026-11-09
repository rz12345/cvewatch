//! 資產對話框的「ISMS 管理」區塊（16 spec GUI asset class and rating input、design D8）：
//! 資產說明（「代碼 名稱」下拉選單，第一項「未歸屬」）、權責單位、保管人、使用單位、
//! 法規遵循性（「未評定」與 1–4）、是否含個人資料（「依類別（否）」「是」「否」），
//! 以及即時顯示的防護等級。選項內容固定，開啟程式後只建立一次（不在每一幀重新組字）。

use std::sync::LazyLock;

use cvewatch_core::assets::classes::{MAX_RATING, MIN_RATING};
use cvewatch_core::assets::columns::StandardField;
use cvewatch_core::assets::{AssetClasses, SystemCode, SystemDirectory};
use cvewatch_frontend::asset_form::{
    personal_data_label, rated_text, PERSONAL_DATA_NO, PERSONAL_DATA_YES, UNASSIGNED_SYSTEM,
    UNRATED,
};
use eframe::egui;

use super::asset_edit::{choice_field, text_field, AssetEdit, Choice};

/// 區塊標題。
pub const SECTION_TITLE: &str = "ISMS 管理";

/// 防護等級的標籤文字（未評定時為「未評定」）。
pub fn protection_text(level: Option<u8>) -> String {
    rated_text(level)
}

/// 法規遵循性選單（「未評定」與 1–4）。
static COMPLIANCE_OPTIONS: LazyLock<Vec<Choice>> = LazyLock::new(compliance_choices);

/// 是否含個人資料選單，依類別預設值分三種（預設「否」「是」、類別未選）。
static PERSONAL_OPTIONS: LazyLock<[Vec<Choice>; 3]> = LazyLock::new(|| {
    [
        personal_choices(Some(false)),
        personal_choices(Some(true)),
        personal_choices(None),
    ]
});

fn personal_options(default: Option<bool>) -> &'static [Choice] {
    let i = match default {
        Some(false) => 0,
        Some(true) => 1,
        None => 2,
    };
    &PERSONAL_OPTIONS[i]
}

/// 「依類別（否）」；類別未選時為「依類別」。
fn personal_by_class(default: Option<bool>) -> String {
    match default {
        Some(v) => format!("依類別（{}）", personal_data_label(v)),
        None => "依類別".to_string(),
    }
}

/// 資產說明選單：「未歸屬」與所有資產說明（依代碼排序）；目前值不在目錄中時另列。
pub(super) fn system_choices(systems: &SystemDirectory, current: &str) -> Vec<Choice> {
    let mut out = vec![Choice {
        value: String::new(),
        text: UNASSIGNED_SYSTEM.to_string(),
    }];
    out.extend(systems.all().iter().map(|s| Choice {
        value: s.code.to_string(),
        text: s.label(),
    }));
    let current = current.trim();
    if let Some(code) = SystemCode::parse(current) {
        if systems.get(&code).is_none() {
            out.push(Choice {
                value: current.to_string(),
                text: systems.label(current),
            });
        }
    }
    out
}

fn compliance_choices() -> Vec<Choice> {
    std::iter::once(Choice {
        value: String::new(),
        text: UNRATED.to_string(),
    })
    .chain((MIN_RATING..=MAX_RATING).map(|n| Choice {
        value: n.to_string(),
        text: n.to_string(),
    }))
    .collect()
}

fn personal_choices(default: Option<bool>) -> Vec<Choice> {
    vec![
        Choice {
            value: String::new(),
            text: personal_by_class(default),
        },
        Choice {
            value: PERSONAL_DATA_YES.to_string(),
            text: personal_data_label(true).to_string(),
        },
        Choice {
            value: PERSONAL_DATA_NO.to_string(),
            text: personal_data_label(false).to_string(),
        },
    ]
}

pub(super) fn admin_fields(
    ui: &mut egui::Ui,
    edit: &mut AssetEdit,
    classes: &AssetClasses,
    systems: &SystemDirectory,
) {
    let errors = edit.errors.clone();
    ui.add_space(4.0);
    ui.strong(SECTION_TITLE);
    let options = &edit.system_options;
    let f = &mut edit.form;
    egui::Grid::new("asset-admin-fields")
        .num_columns(2)
        .show(ui, |ui| {
            use StandardField as F;
            choice_field(ui, &mut f.system, options, &errors, F::System);
            if f.system.trim().is_empty() {
                if let Some(s) = f.effective_system(systems) {
                    ui.label("");
                    ui.weak(format!("由 ISMS 編號帶出：{}", s.label()));
                    ui.end_row();
                }
            }
            text_field(ui, &mut f.responsible_unit, &errors, F::ResponsibleUnit);
            text_field(ui, &mut f.custodian, &errors, F::Custodian);
            text_field(ui, &mut f.user_unit, &errors, F::UserUnit);
            choice_field(
                ui,
                &mut f.compliance,
                &COMPLIANCE_OPTIONS,
                &errors,
                F::Compliance,
            );
            ui.label("防護等級");
            ui.label(protection_text(f.protection_level(classes)));
            ui.end_row();
            let personal = personal_options(f.personal_data_default(classes));
            choice_field(ui, &mut f.personal_data, personal, &errors, F::PersonalData);
        });
}
