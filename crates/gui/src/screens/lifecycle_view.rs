//! 資產詳情的「生命週期」區塊（22 spec GUI lifecycle in asset detail、design D11）：硬體 EoL、
//! 每個清冊元件的軟體 EoL（含版本分支）與保固、維護合約、軟體授權到期，每項一列（項目、日期、
//! 狀態與天數、來源）。沒有日期的項目顯示原因；來源為網址時以連結樣式呈現，點選後經 frontend 的
//! Opener 開啟（與管理介面位址相同，不用 `egui::Hyperlink`）。

use cvewatch_core::assets::dates::format_date;
use cvewatch_core::lifecycle::{AssetLifecycle, ItemState, LifecycleItem, Source};
use cvewatch_frontend::display::BLANK;
use eframe::egui;

use super::{heading, ALERT};
use crate::text::one_line;

/// 區塊標題。
pub const LIFECYCLE_HEADING: &str = "生命週期";

/// 區塊標題與提醒天數：「生命週期（提醒 90 天）」。
pub fn lifecycle_heading(window: u32) -> String {
    format!("{LIFECYCLE_HEADING}（提醒 {window} 天）")
}

/// 日期欄：日期；沒有日期時為原因（「未填」「無法對應版本分支（6.4）」）或「未公布 EoL」。
pub fn date_cell(item: &LifecycleItem) -> String {
    match &item.state {
        ItemState::Dated { date, .. } => format_date(*date),
        ItemState::EolUndated { .. } => BLANK.to_string(),
        ItemState::NotAnnounced { .. } => "未公布 EoL".to_string(),
        ItemState::Missing(reason) => reason.text(),
    }
}

/// 區塊本體；回傳被點選的來源網址。
pub fn lifecycle_section(
    ui: &mut egui::Ui,
    lifecycle: &AssetLifecycle,
    window: u32,
) -> Option<String> {
    heading(ui, lifecycle_heading(window));
    let mut clicked = None;
    egui::Grid::new("asset-lifecycle")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            for item in &lifecycle.items {
                if let Some(url) = item_row(ui, item) {
                    clicked = Some(url);
                }
                ui.end_row();
            }
        });
    clicked
}

/// 一列：項目、日期（或原因）、狀態與天數（已 EoL／已到期為警示色）、來源。
fn item_row(ui: &mut egui::Ui, item: &LifecycleItem) -> Option<String> {
    ui.label(one_line(&item.subject()));
    ui.label(one_line(&date_cell(item)));
    let status = item.status_text().unwrap_or_default();
    if item.status.is_past() {
        ui.colored_label(ALERT, status);
    } else {
        ui.label(status);
    }
    source_cell(ui, item.source())
}

/// 來源：網址以連結樣式呈現（滑鼠提示為網址），其他為文字；資產欄位沒有來源。
fn source_cell(ui: &mut egui::Ui, source: Option<&Source>) -> Option<String> {
    let Some(source) = source else {
        ui.label("");
        return None;
    };
    let text = one_line(&source.text().unwrap_or_default());
    match source.url() {
        Some(url) => ui
            .link(text)
            .on_hover_text(url)
            .clicked()
            .then(|| url.to_string()),
        None => {
            ui.label(text);
            None
        }
    }
}
