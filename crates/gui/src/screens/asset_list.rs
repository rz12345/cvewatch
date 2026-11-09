//! 資產畫面的工具列與左側資產清單（design 11 D4、D8）。
//!
//! 工具列分三組：新增資產說明、新增資產元件｜清冊檔案（匯入、範本、匯出、從 ISMS 清冊匯入）｜顯示已退役。
//! 清單依資產說明分組（16 design D8；可收合，標題列出資產數與未處理命中數），組內為三欄表格（編號、
//! 名稱、未處理），整列可點選、快速點兩下開啟編輯；名稱過長時截斷並以提示顯示全文。
//! 清單上方的搜尋框（20 design D11）依編號、名稱、主機名稱、IP、序號、財產編號篩選；搜尋中
//! 各組以另一個 id 一律展開，清除搜尋後恢復原本的收合狀態。

use cvewatch_core::assets::AssetStatus;
use cvewatch_core::lifecycle::{LifecycleFilter, LifecycleFlags};
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

/// 有版本待補元件的資產在清單中的標示。
pub const PENDING_MARK: &str = "（版本待補）";

/// 「只顯示版本待補」篩選。
pub const PENDING_FILTER: &str = "只顯示版本待補";

/// 已 EoL（硬體或任一清冊元件）的標示（22 design D11；警示色）。
pub const EOL_MARK: &str = "（已 EoL）";

/// 即將 EoL 的標示。
pub const EOL_SOON_MARK: &str = "（即將 EoL）";

/// 保固、維護合約或軟體授權已到期的標示（警示色）。
pub const EXPIRED_MARK: &str = "（已到期）";

/// 保固、維護合約或軟體授權即將到期的標示。
pub const EXPIRING_MARK: &str = "（即將到期）";

/// 生命週期篩選下拉選單的標籤。
pub const LIFECYCLE_FILTER_LABEL: &str = "生命週期";

/// 生命週期篩選「全部」（不篩選）。
pub const LIFECYCLE_ALL: &str = "全部";

/// 使用中資產名稱後的生命週期標示（依 design D11 的順序）與是否以警示色顯示。
pub fn lifecycle_marks(flags: LifecycleFlags) -> Vec<(&'static str, bool)> {
    [
        (flags.eol, EOL_MARK, true),
        (flags.eol_soon, EOL_SOON_MARK, false),
        (flags.expired, EXPIRED_MARK, true),
        (flags.expiring, EXPIRING_MARK, false),
    ]
    .into_iter()
    .filter(|(on, _, _)| *on)
    .map(|(_, mark, alert)| (mark, alert))
    .collect()
}

/// 生命週期篩選的顯示文字（None 為「全部」）。
pub fn lifecycle_filter_text(filter: Option<LifecycleFilter>) -> &'static str {
    filter.map_or(LIFECYCLE_ALL, LifecycleFilter::label)
}

/// 「新增資產元件」按鈕（開啟新增資產的對話框）。
pub const NEW_ASSET_BUTTON: &str = "新增資產元件";

/// 「新增資產說明」按鈕（開啟資產說明對話框）。
pub const SYSTEMS_BUTTON: &str = "新增資產說明";

/// 「從 ISMS 清冊匯入」按鈕。
pub const ISMS_IMPORT_BUTTON: &str = "從 ISMS 清冊匯入";

/// 搜尋框的提示文字。
pub const SEARCH_HINT: &str = "搜尋編號、名稱、主機名稱、IP、序號、財產編號";

/// 搜尋框左側的標籤。
pub const SEARCH_LABEL: &str = "搜尋";

/// 清除搜尋字的按鈕。
pub const CLEAR_SEARCH: &str = "清除";

/// 沒有符合搜尋字的資產。
pub fn no_match_text(query: &str) -> String {
    format!("沒有符合「{}」的資產", one_line(query.trim()))
}

/// 清單的欄位標題。
pub const LIST_COLUMNS: [&str; 3] = ["編號", "名稱", "未處理"];

const ROW_HEIGHT: f32 = 22.0;
/// 未處理數欄的寬度（固定，名稱欄取得其餘寬度）。
const COUNT_WIDTH: f32 = 56.0;
/// 清單預設佔資產畫面寬度的比例（其餘給右側詳情）；使用者拖曳分隔線後依拖曳的寬度。
const LIST_WIDTH_RATIO: f32 = 2.0 / 3.0;

impl GuiApp {
    pub(super) fn assets_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button(SYSTEMS_BUTTON).clicked() {
                self.start_systems_dialog();
            }
            if ui.button(NEW_ASSET_BUTTON).clicked() {
                self.start_new_asset();
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
                .add_enabled(enabled, egui::Button::new(ISMS_IMPORT_BUTTON))
                .on_hover_text(
                    "先預覽再匯入；描述中的產品成為版本待補元件，匯入後在編輯對話框補上版本",
                )
                .on_disabled_hover_text(NO_COMPANION)
                .clicked()
            {
                self.pick_isms_workbook();
            }
            ui.separator();
            ui.checkbox(&mut self.assets_show_retired, "顯示已退役");
            ui.checkbox(&mut self.assets_pending_only, PENDING_FILTER);
            lifecycle_combo(ui, &mut self.assets_lifecycle);
        });
    }

    /// 左側清單（頂端為搜尋框）；回傳被點選的資產。
    pub(super) fn asset_list(&mut self, ui: &mut egui::Ui) -> Option<ListPick> {
        let mut picked = None;
        let default_width = ui.available_width() * LIST_WIDTH_RATIO;
        egui::Panel::left("asset-list")
            .resizable(true)
            .default_size(default_width)
            .show(ui, |ui| {
                search_box(ui, &mut self.assets_search);
                let filter = AssetListFilter {
                    include_retired: self.assets_show_retired,
                    pending_only: self.assets_pending_only,
                    class: None,
                    search: self.assets_search.clone(),
                    lifecycle: self.assets_lifecycle,
                };
                let searching = !filter.search.trim().is_empty();
                let groups = self.ws.data.asset_groups(&filter);
                if groups.is_empty() && searching {
                    ui.label(no_match_text(&filter.search));
                    return;
                }
                if groups.is_empty() {
                    ui.label(format!(
                        "尚無資產。請按「{NEW_ASSET_BUTTON}」或「匯入清冊」，{DROP_HINT}。"
                    ));
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt("asset-groups")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for group in &groups {
                            let selected = self.assets_selected.as_deref();
                            let hit = group_section(ui, group, selected, searching);
                            picked = hit.or(picked.take());
                        }
                    });
            });
        picked
    }
}

/// 清單中被點選的資產；`open` 表示快速點兩下（開啟編輯）。
pub(super) struct ListPick {
    pub tag: String,
    pub open: bool,
}

/// 組的收合狀態 id（測試可用來收合）。
pub fn group_id(key: &str) -> egui::Id {
    egui::Id::unique(("asset-group", key))
}

/// 搜尋中組的收合狀態 id（與 [`group_id`] 分開：搜尋時一律展開，原本的收合狀態不受影響）。
pub fn search_group_id(key: &str) -> egui::Id {
    egui::Id::unique(("asset-search", key))
}

/// 工具列的生命週期篩選：「生命週期：全部／已 EoL／即將 EoL／保固或合約到期」。
fn lifecycle_combo(ui: &mut egui::Ui, value: &mut Option<LifecycleFilter>) {
    let l = ui.label(LIFECYCLE_FILTER_LABEL);
    let combo = egui::ComboBox::from_id_salt("assets-lifecycle-filter")
        .selected_text(lifecycle_filter_text(*value))
        .show_ui(ui, |ui| {
            let choices = std::iter::once(None).chain(LifecycleFilter::ALL.map(Some));
            for choice in choices {
                ui.selectable_value(value, choice, lifecycle_filter_text(choice));
            }
        });
    combo.response.labelled_by(l.id);
}

/// 搜尋框與「清除」按鈕；搜尋框取得焦點時清單的 Enter／Delete 不作用（`assets_keys`）。
fn search_box(ui: &mut egui::Ui, query: &mut String) {
    ui.horizontal(|ui| {
        let l = ui.label(SEARCH_LABEL);
        // 「清除」按鈕的寬度
        let clear_width = 48.0;
        let edit = egui::TextEdit::singleline(query)
            .id_salt("assets-search")
            .hint_text(SEARCH_HINT)
            .desired_width((ui.available_width() - clear_width).max(120.0));
        ui.add(edit).labelled_by(l.id);
        if ui
            .add_enabled(!query.is_empty(), egui::Button::new(CLEAR_SEARCH))
            .clicked()
        {
            query.clear();
        }
    });
    ui.add_space(4.0);
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
    searching: bool,
) -> Option<ListPick> {
    let mut picked = None;
    let id = if searching {
        search_group_id(group.key())
    } else {
        group_id(group.key())
    };
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
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
) -> Option<ListPick> {
    let mut picked = None;
    // 退役列用佈景主題的弱化文字色（深色主題下 `Color32::GRAY` 比一般文字更亮）
    let weak = ui.visuals().weak_text_color();
    TableBuilder::new(ui)
        .id_salt(("asset-list-table", key))
        .vscroll(false)
        .striped(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        // 寬度填滿面板，高度只取列數所需：不然展開的組會佔滿剩餘高度，下方留下大片空白
        .auto_shrink([false, true])
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
                let resp = row.response();
                if resp.clicked() || resp.double_clicked() {
                    picked = Some(ListPick {
                        tag: tag.clone(),
                        open: resp.double_clicked(),
                    });
                }
            });
        });
    picked
}

/// 一列：編號、名稱（與編號相同時留空；退役、版本待補與生命週期加標示）、未處理數（靠右，大於 0 時醒目；
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
    row.col(|ui| {
        ui.add(egui::Label::new(paint(one_line(&r.asset_tag))).selectable(false));
    });
    row.col(|ui| {
        let text = name_text(ui, a, weak, selected);
        ui.add(egui::Label::new(text).selectable(false).truncate());
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

/// 名稱欄：名稱（與編號相同時留空）與標示。退役資產整列弱化並加「（已退役）」；使用中資產依序加
/// 版本待補與生命週期標示，已 EoL／已到期以警示色顯示（其餘沿用列的文字色；選取列與未處理數
/// 相同改用粗體，以免對比不足）。
fn name_text(
    ui: &egui::Ui,
    a: &AssetEntry,
    weak: egui::Color32,
    selected: bool,
) -> egui::text::LayoutJob {
    let name = shown_name(&a.asset.record)
        .map(one_line)
        .unwrap_or_default();
    let mut job = egui::text::LayoutJob::default();
    let style = ui.style();
    let mut push = |text: egui::RichText| {
        text.append_to(
            &mut job,
            style,
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    };
    if a.asset.status == AssetStatus::Retired {
        push(egui::RichText::new(format!("{name}{RETIRED_MARK}")).color(weak));
        return job;
    }
    let pending = if a.has_pending() { PENDING_MARK } else { "" };
    push(egui::RichText::new(format!("{name}{pending}")));
    for (mark, alert) in lifecycle_marks(a.lifecycle.flags()) {
        let text = egui::RichText::new(mark);
        push(match (alert, selected) {
            (true, true) => text.strong(),
            (true, false) => text.color(ALERT),
            (false, _) => text,
        });
    }
    job
}
