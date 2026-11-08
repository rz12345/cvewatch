//! 命中畫面：搜尋、篩選、可排序欄位、虛擬列表格、多選與狀態變更、匯出 VEX。

use cvewatch_core::matching::engine::Confidence;
use cvewatch_core::matching::query::FindingRow;
use cvewatch_core::matching::scoring::Priority;
use cvewatch_core::matching::triage::Status;
use cvewatch_frontend::filter::{Filter, KevFilter, SortKey};
use cvewatch_frontend::Report;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::app::GuiApp;
use crate::text::{one_line, short_time};

/// 表格列高。
const ROW_HEIGHT: f32 = 22.0;

/// 可點選排序的欄位標題（design D4）；其餘欄位標題不可點選。
const SORTABLE: [(&str, SortKey); 5] = [
    ("等級", SortKey::Risk),
    ("CVSS", SortKey::Cvss),
    ("EPSS", SortKey::Epss),
    ("首次發現", SortKey::FirstSeen),
    ("資產", SortKey::Asset),
];

/// 表格欄位（依顯示順序）。
const COLUMNS: [&str; 11] = [
    "標記",
    "等級",
    "CVE",
    "資產",
    "資產名稱",
    "元件",
    "版本",
    "CVSS",
    "EPSS",
    "狀態",
    "首次發現",
];

/// 點選列的方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Click {
    /// 只移動游標（右鍵點在已選取的列）
    Cursor,
    Single,
    Toggle,
    Range,
}

/// 命中的標記：probable（`?`）、已處理但仍在受影響範圍（`!`）、KEV。
pub fn marks(r: &FindingRow) -> String {
    let mut m = String::new();
    if r.confidence == Confidence::Probable {
        m.push('?');
    }
    if r.still_vulnerable {
        m.push('!');
    }
    if r.scores.kev_date.is_some() {
        m.push_str(" KEV");
    }
    m.trim().to_string()
}

impl GuiApp {
    pub(super) fn show_findings(&mut self, ui: &mut egui::Ui) {
        self.findings_toolbar(ui);
        if self.ws.findings.view.filter_open {
            ui.separator();
            self.filter_panel(ui);
        }
        ui.separator();
        self.findings_table(ui);
    }

    fn findings_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let label = ui.label("搜尋");
            let edit = ui
                .add(
                    egui::TextEdit::singleline(&mut self.ws.findings.search)
                        .hint_text("CVE、資產、描述（Ctrl+F）")
                        .desired_width(260.0),
                )
                .labelled_by(label.id);
            if std::mem::take(&mut self.ws.findings.view.focus_search) {
                edit.request_focus();
            }
            if edit.changed() {
                self.ws.refresh_visible();
            }
            let open = self.ws.findings.view.filter_open;
            if ui.selectable_label(open, "篩選…").clicked() {
                self.ws.findings.view.filter_open = !open;
            }
            ui.label(format!(
                "共 {} 筆｜已選 {} 筆｜排序：{}",
                self.ws.findings.visible.len(),
                self.ws.findings.selected.len(),
                self.ws.findings.sort.label()
            ));
        });
        ui.label(self.ws.findings.filter.summary());
        ui.horizontal(|ui| {
            for (label, status) in [
                ("已處理", Status::Resolved),
                ("忽略", Status::Ignored),
                ("未處理", Status::Open),
            ] {
                if ui.button(label).clicked() {
                    self.start_triage(status);
                }
            }
            let any = !self.ws.findings.selected.is_empty();
            if ui.add_enabled(any, egui::Button::new("清除選取")).clicked() {
                self.clear_selection();
            }
            ui.separator();
            if ui.button("匯出 VEX").clicked() {
                self.export_vex();
            }
        });
    }

    fn filter_panel(&mut self, ui: &mut egui::Ui) {
        let f = self.ws.findings.filter.clone();
        let mut next = f.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label("狀態");
            for s in Status::ALL {
                let mut on = f.statuses.contains(s);
                if ui.checkbox(&mut on, s.label()).changed() {
                    next = next.with_status_toggled(*s);
                }
            }
            ui.separator();
            ui.label("等級");
            for p in Priority::ALL {
                let mut on = f.priorities.contains(p);
                if ui.checkbox(&mut on, p.as_str()).changed() {
                    next = next.with_priority_toggled(*p);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            next = self.asset_combo(ui, &next);
            let conf = f.confidence.map_or("全部", |c| c.as_str());
            egui::ComboBox::from_label("信心")
                .selected_text(conf)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(f.confidence.is_none(), "全部")
                        .clicked()
                    {
                        next = next.with_confidence(None);
                    }
                    for c in Confidence::ALL {
                        if ui
                            .selectable_label(f.confidence == Some(*c), c.as_str())
                            .clicked()
                        {
                            next = next.with_confidence(Some(*c));
                        }
                    }
                });
            ui.label("KEV");
            for k in [KevFilter::All, KevFilter::Only, KevFilter::Exclude] {
                if ui.radio(f.kev == k, k.label()).clicked() {
                    next = next.with_kev(k);
                }
            }
            if ui.button("重設篩選").clicked() {
                next = Filter::default();
            }
        });
        if next != f {
            self.ws.findings.filter = next;
            self.ws.refresh_visible();
        }
    }

    fn asset_combo(&self, ui: &mut egui::Ui, f: &Filter) -> Filter {
        let mut next = f.clone();
        egui::ComboBox::from_label("資產篩選")
            .selected_text(f.asset.as_deref().unwrap_or("全部"))
            .show_ui(ui, |ui| {
                if ui.selectable_label(f.asset.is_none(), "全部").clicked() {
                    next = f.with_asset(None);
                }
                for a in &self.ws.data.assets {
                    let tag = &a.asset.record.asset_tag;
                    let on = f.asset.as_deref() == Some(tag.as_str());
                    if ui.selectable_label(on, tag).clicked() {
                        next = f.with_asset(Some(tag.clone()));
                    }
                }
            });
        next
    }

    fn findings_table(&mut self, ui: &mut egui::Ui) {
        let rows = self.ws.findings.visible.len();
        if rows == 0 {
            ui.label("目前篩選沒有命中。");
            return;
        }
        let tz = self.ws.services.tz;
        let mut clicked: Option<(i64, Click)> = None;
        let mut double: Option<i64> = None;
        let mut sort: Option<SortKey> = None;
        let mut menu: Option<MenuAction> = None;
        let current_sort = self.ws.findings.sort;
        let scroll_to = self.ws.findings.view.scroll_to.take();
        let mut table = TableBuilder::new(ui)
            .id_salt("findings-table")
            .striped(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .auto_shrink([false, false]);
        if let Some(row) = scroll_to {
            table = table.scroll_to_row(row, None);
        }
        for (i, _) in COLUMNS.iter().enumerate() {
            table = table.column(match i {
                2 => Column::initial(140.0).at_least(110.0).resizable(true),
                3 | 5 => Column::initial(110.0)
                    .at_least(60.0)
                    .resizable(true)
                    .clip(true),
                4 => Column::initial(200.0)
                    .at_least(80.0)
                    .resizable(true)
                    .clip(true),
                10 => Column::remainder().at_least(90.0),
                _ => Column::auto().at_least(40.0),
            });
        }
        table
            .header(ROW_HEIGHT, |mut header| {
                for name in COLUMNS {
                    header.col(|ui| {
                        match SORTABLE.iter().find(|(n, _)| *n == name) {
                            Some((n, key)) => {
                                if ui.selectable_label(current_sort == *key, *n).clicked() {
                                    sort = Some(*key);
                                }
                            }
                            None => {
                                ui.strong(name);
                            }
                        };
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, rows, |mut row| {
                    let Some(r) = self.ws.row_at(row.index()) else {
                        return;
                    };
                    let id = r.id;
                    row.set_selected(self.ws.findings.selected.contains(&id));
                    for text in cells(r, tz) {
                        row.col(|ui| {
                            ui.add(egui::Label::new(text).selectable(false).truncate());
                        });
                    }
                    let resp = row.response();
                    if resp.double_clicked() {
                        double = Some(id);
                    } else if resp.clicked() {
                        let m = resp.ctx.input(|i| i.modifiers);
                        let how = if m.shift {
                            Click::Range
                        } else if m.command || m.ctrl {
                            Click::Toggle
                        } else {
                            Click::Single
                        };
                        clicked = Some((id, how));
                    }
                    // 右鍵：游標移到該列；點在未選取的列時改為只選這一列（選單套用到它）
                    if resp.secondary_clicked() {
                        let how = if self.ws.findings.selected.contains(&id) {
                            Click::Cursor
                        } else {
                            Click::Single
                        };
                        clicked = Some((id, how));
                    }
                    resp.context_menu(|ui| {
                        if let Some(a) = context_menu(ui, id) {
                            menu = Some(a);
                            ui.close();
                        }
                    });
                });
            });
        if let Some(key) = sort {
            self.ws.findings.sort = key;
            self.ws.refresh_visible();
        }
        if let Some((id, how)) = clicked {
            self.click_row(id, how);
        }
        if let Some(id) = double {
            self.click_row(id, Click::Single);
            self.open_detail(id);
        }
        if let Some(a) = menu {
            self.menu_action(a);
        }
    }

    fn click_row(&mut self, id: i64, how: Click) {
        let view = &mut self.ws.findings.view;
        let anchor = view.anchor;
        view.current = Some(id);
        match how {
            Click::Cursor => {}
            Click::Single => {
                self.ws.findings.selected = [id].into();
                self.ws.findings.view.anchor = Some(id);
            }
            Click::Toggle => {
                self.ws.findings.toggle(id);
                self.ws.findings.view.anchor = Some(id);
            }
            Click::Range => {
                let ids = self.ws.visible_ids();
                let pos = |x: i64| ids.iter().position(|i| *i == x);
                let (Some(a), Some(b)) = (anchor.and_then(pos), pos(id)) else {
                    self.ws.findings.selected = [id].into();
                    self.ws.findings.view.anchor = Some(id);
                    return;
                };
                let (lo, hi) = (a.min(b), a.max(b));
                self.ws.findings.selected = ids[lo..=hi].iter().copied().collect();
            }
        }
    }

    /// 上下鍵移動游標（單選），並捲動表格讓該列可見。
    pub(crate) fn move_cursor(&mut self, down: bool) {
        let ids = self.ws.visible_ids();
        if ids.is_empty() {
            return;
        }
        let pos = self
            .current_visible()
            .and_then(|c| ids.iter().position(|i| *i == c));
        let next = match (pos, down) {
            (None, _) => 0,
            (Some(p), true) => (p + 1).min(ids.len() - 1),
            (Some(p), false) => p.saturating_sub(1),
        };
        self.click_row(ids[next], Click::Single);
        self.ws.findings.view.scroll_to = Some(next);
    }

    fn menu_action(&mut self, a: MenuAction) {
        match a {
            MenuAction::Triage(s) => self.start_triage(s),
            MenuAction::Detail(id) => self.open_detail(id),
            MenuAction::Copy(id) => self.copy_finding_cve(id),
        }
    }

    fn copy_finding_cve(&mut self, id: i64) {
        let cve = self
            .ws
            .data
            .findings
            .iter()
            .find(|f| f.id == id)
            .map(|f| f.cve_id.clone());
        match cve {
            Some(c) => self.copy(&c),
            None => self.report(Report::info("請先選取命中")),
        }
    }

    /// `y`：複製詳情或游標列（仍顯示中）的 CVE ID。
    pub fn copy_current_cve(&mut self) {
        if let Some(d) = &self.detail {
            let cve = d.data.finding.row.cve_id.clone();
            return self.copy(&cve);
        }
        match self.current_visible() {
            Some(id) => self.copy_finding_cve(id),
            None => self.report(Report::info("請先選取命中")),
        }
    }

    /// 匯出目前篩選：另存新檔對話框（預設文件資料夾與檔名），不覆蓋既有檔案。
    pub fn export_vex(&mut self) {
        let (path, _count) = match self.ws.vex_export_default() {
            Ok(x) => x,
            Err(r) => return self.report(r),
        };
        let dir = path.parent().map_or_else(
            || self.ws.services.documents_dir.clone(),
            |p| p.to_path_buf(),
        );
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(target) = self.dialogs.save_vex(&dir, &name) else {
            return self.report(Report::info("已取消匯出"));
        };
        let r = self.ws.export_vex(&target.to_string_lossy());
        self.report(r);
    }
}

/// 右鍵選單的動作：狀態變更套用到所選命中（或游標列），詳情與複製針對按右鍵的那一列。
#[derive(Debug, Clone, Copy)]
enum MenuAction {
    Triage(Status),
    Detail(i64),
    Copy(i64),
}

fn context_menu(ui: &mut egui::Ui, id: i64) -> Option<MenuAction> {
    let mut out = None;
    for (label, a) in [
        ("設為已處理", MenuAction::Triage(Status::Resolved)),
        ("設為忽略…", MenuAction::Triage(Status::Ignored)),
        ("設為未處理", MenuAction::Triage(Status::Open)),
        ("開啟詳情", MenuAction::Detail(id)),
        ("複製 CVE ID", MenuAction::Copy(id)),
    ] {
        if ui.button(label).clicked() {
            out = Some(a);
        }
    }
    out
}

/// 一列的儲存格文字（依 [`COLUMNS`] 順序）。
fn cells(r: &FindingRow, tz: cvewatch_frontend::services::DisplayTz) -> [String; 11] {
    let score =
        |s: Option<f64>, digits: usize| s.map_or("-".to_string(), |v| format!("{v:.digits$}"));
    // 外部資料（CVE 紀錄、清冊）的文字一律去除控制字元，避免儲存格換行破版
    [
        marks(r),
        r.priority.as_str().to_string(),
        one_line(&r.cve_id),
        one_line(&r.asset_tag),
        one_line(&r.asset_name),
        one_line(&r.component),
        one_line(&r.version),
        score(r.scores.cvss, 1),
        score(r.scores.epss, 3),
        r.status.label().to_string(),
        short_time(&r.first_seen, tz),
    ]
}
