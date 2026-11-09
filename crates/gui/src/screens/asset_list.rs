//! 資產畫面的工具列與左側資產清單（design 11 D4、D8）。
//!
//! 工具列分三組：新增資產、資產說明…｜清冊檔案（匯入、範本、匯出）｜顯示已退役。清單依資產說明
//! 分組（16 design D8；可收合，標題列出資產數與未處理命中數），組內為三欄表格（編號、名稱、
//! 未處理），整列可點選；名稱過長時截斷並以提示顯示全文。

use cvewatch_core::assets::AssetStatus;
use cvewatch_frontend::data::{AssetEntry, AssetListFilter};
use cvewatch_frontend::display::shown_name;
use cvewatch_frontend::groups::AssetGroup;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

use super::ALERT;
use crate::app::{GuiApp, NO_COMPANION};
use crate::text::one_line;

/// 退役資產在清單中的標示。
pub const RETIRED_MARK: &str = "（已退役）";

/// 拖放匯入的提示（「匯入清冊」的滑鼠提示與空清單）。
pub const DROP_HINT: &str = "也可以把 .xlsx 或 .csv 清冊拖進視窗";

/// 「由 ISMS 清冊產生草稿」按鈕。
pub const ISMS_DRAFT_BUTTON: &str = "由 ISMS 清冊產生草稿";

/// 清單的欄位標題。
pub const LIST_COLUMNS: [&str; 3] = ["編號", "名稱", "未處理"];

const ROW_HEIGHT: f32 = 22.0;
/// 未處理數欄的寬度（固定，名稱欄取得其餘寬度）。
const COUNT_WIDTH: f32 = 56.0;

impl GuiApp {
    pub(super) fn assets_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("新增資產").clicked() {
                self.start_new_asset();
            }
            if ui.button("資產說明…").clicked() {
                self.start_systems_dialog();
            }
            ui.separator();
            let enabled = self.companion.is_some();
            if ui
                .add_enabled(enabled, egui::Button::new("匯入清冊"))
                .on_hover_text(DROP_HINT)
                .on_disabled_hover_text(NO_COMPANION)
                .clicked()
            {
                self.pick_inventory();
            }
            if ui.button("下載範本").clicked() {
                self.download_template();
            }
            if ui.button("匯出清冊").clicked() {
                self.export_inventory();
            }
            if ui
                .button(ISMS_DRAFT_BUTTON)
                .on_hover_text("ISMS 清冊沒有版本：產生草稿後補上版本，再以「匯入清冊」匯入")
                .clicked()
            {
                self.generate_isms_draft();
            }
            ui.separator();
            ui.checkbox(&mut self.assets_show_retired, "顯示已退役");
        });
    }

    /// 左側清單；回傳被點選的資產編號。
    pub(super) fn asset_list(&self, ui: &mut egui::Ui) -> Option<String> {
        let filter = AssetListFilter {
            include_retired: self.assets_show_retired,
            class: None,
        };
        let mut picked = None;
        egui::Panel::left("asset-list")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| {
                let groups = self.ws.data.asset_groups(&filter);
                if groups.is_empty() {
                    ui.label(format!(
                        "尚無資產。請按「新增資產」或「匯入清冊」，{DROP_HINT}。"
                    ));
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt("asset-groups")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for group in &groups {
                            let hit = group_section(ui, group, self.assets_selected.as_deref());
                            picked = hit.or(picked.take());
                        }
                    });
            });
        picked
    }
}

/// 組的收合狀態 id（測試可用來收合）。
pub fn group_id(key: &str) -> egui::Id {
    egui::Id::unique(("asset-group", key))
}

/// 組的未處理命中標示。
pub fn group_open_text(open: usize) -> String {
    format!("未處理 {open}")
}

/// 一組：可收合的標題（「NET 網路服務（2）」與醒目的未處理數）與組內表格。
fn group_section(
    ui: &mut egui::Ui,
    group: &AssetGroup<'_>,
    selected: Option<&str>,
) -> Option<String> {
    let mut picked = None;
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        group_id(group.key()),
        true,
    )
    .show_header(ui, |ui| {
        ui.strong(group.heading());
        if group.open_findings > 0 {
            ui.colored_label(ALERT, group_open_text(group.open_findings));
        }
    })
    .body(|ui| {
        picked = list_table(ui, group.key(), &group.assets, selected);
    });
    picked
}

fn list_table(
    ui: &mut egui::Ui,
    key: &str,
    list: &[&AssetEntry],
    selected: Option<&str>,
) -> Option<String> {
    let mut picked = None;
    // 退役列用佈景主題的弱化文字色（深色主題下 `Color32::GRAY` 比一般文字更亮）
    let weak = ui.visuals().weak_text_color();
    TableBuilder::new(ui)
        .id_salt(("asset-list-table", key))
        .vscroll(false)
        .striped(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .auto_shrink([false, false])
        .column(Column::auto().at_least(70.0))
        .column(Column::remainder().at_least(60.0).clip(true))
        .column(Column::exact(COUNT_WIDTH))
        .header(ROW_HEIGHT, |mut header| {
            for name in LIST_COLUMNS {
                header.col(|ui| {
                    ui.strong(name);
                });
            }
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, list.len(), |mut row| {
                let a = list[row.index()];
                let tag = &a.asset.record.asset_tag;
                let on = selected == Some(tag.as_str());
                row.set_selected(on);
                list_row(&mut row, a, weak, on);
                if row.response().clicked() {
                    picked = Some(tag.clone());
                }
            });
        });
    picked
}

/// 一列：編號、名稱（與編號相同時留空；退役加標示）、未處理數（靠右，大於 0 時醒目；
/// 選取列的底色已醒目，數字改用粗體不加警示色以免對比不足）。
fn list_row(
    row: &mut egui_extras::TableRow<'_, '_>,
    a: &AssetEntry,
    weak: egui::Color32,
    selected: bool,
) {
    let r = &a.asset.record;
    let retired = a.asset.status == AssetStatus::Retired;
    let paint = |text: String| {
        let t = egui::RichText::new(text);
        if retired {
            t.color(weak)
        } else {
            t
        }
    };
    let name = shown_name(r).map(one_line).unwrap_or_default();
    let name = if retired {
        format!("{name}{RETIRED_MARK}")
    } else {
        name
    };
    row.col(|ui| {
        ui.add(egui::Label::new(paint(one_line(&r.asset_tag))).selectable(false));
    });
    row.col(|ui| {
        ui.add(egui::Label::new(paint(name)).selectable(false).truncate());
    });
    row.col(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let count = egui::RichText::new(a.open_findings.to_string());
            let count = match (retired, a.open_findings > 0) {
                (true, _) => count.color(weak),
                (false, true) if selected => count.strong(),
                (false, true) => count.color(ALERT).strong(),
                (false, false) => count,
            };
            ui.add(egui::Label::new(count).selectable(false));
        });
    });
}
