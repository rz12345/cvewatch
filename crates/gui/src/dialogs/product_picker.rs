//! 產品選擇器（design 14 D7）：按鈕＋`egui::Popup`，內含搜尋框與結果清單。
//!
//! 「加入元件」與未辨識產品的「改為…」共用。開啟時搜尋框取得焦點、第一筆反白；搜尋框有焦點時
//! ↑↓ 移動反白（並捲動到反白項目）、Enter 選擇（先於 TextEdit 取用按鍵）；Esc 關閉且不變更。
//! 搜尋字改變時反白回到第一筆。結果超過 [`PICKER_LIMIT`] 筆只顯示前幾筆並提示其餘筆數。
//! 搜尋、排序與上限在 [`cvewatch_frontend::product_search`]；清單在搜尋字改變時才重新計算。

use cvewatch_core::catalog::Catalog;
use cvewatch_frontend::product_search::{
    picker_list, unresolved_picker_list, PickerList, ProductChoice, PICKER_LIMIT,
};
use eframe::egui;

/// 搜尋框的標籤。
pub const SEARCH_LABEL: &str = "搜尋產品";
/// 未辨識模式的「保留」選項。
pub const KEEP: &str = "保留";
/// 未辨識模式中相似建議的小標。
pub const SUGGESTED: &str = "建議";

const SEARCH_WIDTH: f32 = 260.0;
const LIST_HEIGHT: f32 = 380.0;

/// 選擇結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    Keep,
    Product(String),
}

/// 選擇器的模式。
#[derive(Debug, Clone, Copy)]
pub enum PickerMode<'a> {
    /// 加入元件：選項為「顯示名稱（廠商）」
    Add,
    /// 未辨識產品的「改為…」：「保留」、相似建議與搜尋結果，選項為「改為 顯示名稱（廠商）」
    Unresolved { product_raw: &'a str },
}

/// 選擇器按鈕：`id` 區分各選擇器，`label` 為左側標籤（以 `labelled_by` 關聯），`hover` 為提示文字。
pub struct PickerButton<'a> {
    pub id: egui::Id,
    pub text: &'a str,
    pub label: egui::Id,
    pub hover: Option<&'a str>,
}

/// 一個選擇器的狀態。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PickerState {
    pub search: String,
    /// 反白項目的索引（依清單順序，含「保留」與建議）
    pub highlighted: usize,
    /// 上一幀是開啟的（剛開啟的那一幀讓搜尋框取得焦點）
    pub open: bool,
    /// 上次計算清單時的搜尋字與結果（搜尋字不變時沿用，不在每一幀重新搜尋）
    cached: Option<(String, PickerList)>,
}

impl PickerState {
    fn reset(&mut self) {
        *self = PickerState::default();
    }

    fn list(&mut self, catalog: &Catalog, mode: PickerMode<'_>) -> &PickerList {
        let stale = self.cached.as_ref().is_none_or(|(q, _)| *q != self.search);
        if stale {
            let list = match mode {
                PickerMode::Add => picker_list(catalog, &self.search),
                PickerMode::Unresolved { product_raw } => {
                    unresolved_picker_list(catalog, product_raw, &self.search)
                }
            };
            self.cached = Some((self.search.clone(), list));
        }
        &self.cached.as_ref().expect("剛計算過的清單").1
    }
}

/// 清單中的一列。
struct Entry {
    text: String,
    picked: Picked,
    suggested: bool,
}

fn entries(list: &PickerList, mode: PickerMode<'_>) -> Vec<Entry> {
    let product = |c: &ProductChoice, suggested: bool| Entry {
        text: match mode {
            PickerMode::Add => c.label(),
            PickerMode::Unresolved { .. } => format!("改為 {}", c.label()),
        },
        picked: Picked::Product(c.key.clone()),
        suggested,
    };
    let keep = matches!(mode, PickerMode::Unresolved { .. }).then(|| Entry {
        text: KEEP.to_string(),
        picked: Picked::Keep,
        suggested: false,
    });
    keep.into_iter()
        .chain(list.suggestions.iter().map(|c| product(c, true)))
        .chain(list.results.shown.iter().map(|c| product(c, false)))
        .collect()
}

/// 方向鍵與 Enter（搜尋框有焦點時才取用，避免被 TextEdit 處理）。
#[derive(Default)]
struct Keys {
    up: bool,
    down: bool,
    enter: bool,
}

fn take_keys(ui: &egui::Ui, search_id: egui::Id) -> Keys {
    if !ui.memory(|m| m.has_focus(search_id)) {
        return Keys::default();
    }
    ui.input_mut(|i| Keys {
        up: i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
        down: i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
        enter: i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
    })
}

/// 繪製選擇器按鈕與開啟中的清單；選擇後回傳結果並關閉。
pub fn product_picker(
    ui: &mut egui::Ui,
    button: PickerButton<'_>,
    state: &mut PickerState,
    catalog: &Catalog,
    mode: PickerMode<'_>,
) -> Option<Picked> {
    let mut response = ui.button(button.text).labelled_by(button.label);
    if let Some(hover) = button.hover {
        response = response.on_hover_text(hover);
    }
    // 與 ComboBox 相同：螢幕閱讀器讀出標籤與目前的選擇
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::new(egui::Role::ComboBox);
        info.enabled = ui.is_enabled();
        info.current_text_value = Some(button.text.to_string());
        info
    });
    let popup_id = button.id.with("popup");
    let just_opened = !state.open;
    let mut picked = None;
    let shown = egui::Popup::from_toggle_button_response(&response)
        .id(popup_id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            picked = popup_body(ui, button.id, state, catalog, mode, just_opened);
        });
    state.open = shown.is_some();
    if picked.is_some() {
        egui::Popup::close_id(ui.ctx(), popup_id);
    }
    if picked.is_some() || !state.open {
        state.reset();
    }
    picked
}

fn popup_body(
    ui: &mut egui::Ui,
    id: egui::Id,
    state: &mut PickerState,
    catalog: &Catalog,
    mode: PickerMode<'_>,
    just_opened: bool,
) -> Option<Picked> {
    let search_id = id.with("search");
    let keys = take_keys(ui, search_id);
    let l = ui.label(SEARCH_LABEL);
    let search = ui
        .add(
            egui::TextEdit::singleline(&mut state.search)
                .id(search_id)
                .desired_width(SEARCH_WIDTH),
        )
        .labelled_by(l.id);
    if just_opened {
        search.request_focus();
        state.highlighted = 0;
    }
    if search.changed() {
        state.highlighted = 0;
    }
    let mut highlighted = state.highlighted;
    let list = state.list(catalog, mode);
    let rows = entries(list, mode);
    let last = rows.len().saturating_sub(1);
    if keys.down {
        highlighted = (highlighted + 1).min(last);
    }
    if keys.up {
        highlighted = highlighted.saturating_sub(1);
    }
    highlighted = highlighted.min(last);
    let picked = if keys.enter {
        rows.get(highlighted).map(|row| row.picked.clone())
    } else {
        rows_ui(ui, &rows, list, highlighted, keys.up || keys.down)
    };
    state.highlighted = highlighted;
    picked
}

fn rows_ui(
    ui: &mut egui::Ui,
    rows: &[Entry],
    list: &PickerList,
    highlighted: usize,
    moved: bool,
) -> Option<Picked> {
    let mut picked = None;
    egui::ScrollArea::vertical()
        .max_height(LIST_HEIGHT)
        .show(ui, |ui| {
            let mut heading_shown = false;
            for (i, row) in rows.iter().enumerate() {
                if row.suggested && !heading_shown {
                    ui.weak(SUGGESTED);
                    heading_shown = true;
                }
                let r = ui.selectable_label(i == highlighted, &row.text);
                if moved && i == highlighted {
                    r.scroll_to_me(None);
                }
                if r.clicked() {
                    picked = Some(row.picked.clone());
                }
            }
            if let Some(footer) = list.footer() {
                ui.weak(footer);
            }
        });
    debug_assert!(list.results.shown.len() <= PICKER_LIMIT);
    picked
}
