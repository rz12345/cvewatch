//! Dashboard：各等級未處理數、KEV 數、等待區與逾期數、來源矛盾數、各來源最後同步。
//! 點選等級或 KEV 數字開啟套用對應篩選的命中畫面；點選版本待補數字開啟只顯示有版本待補元件的
//! 資產畫面（18 design D8）；點選生命週期數字開啟套用對應生命週期篩選的資產畫面（22 design D11）。

use cvewatch_core::lifecycle::{LifecycleCounts, LifecycleFilter};
use cvewatch_core::matching::dashboard::SourceStatus;
use cvewatch_core::matching::scoring::Priority;
use cvewatch_frontend::display::pending_versions_text;
use cvewatch_frontend::filter::{Filter, KevFilter};
use cvewatch_frontend::services::DisplayTz;
use eframe::egui;

use super::{heading, ALERT, OK};
use crate::app::{GuiApp, Screen};
use crate::text::{one_line, short_time};

/// 跳轉到命中畫面時套用的篩選。
enum Jump {
    Priority(Priority),
    Kev,
    PendingVersions,
    Lifecycle(LifecycleFilter),
}

/// 生命週期數字（「已 EoL 2」；整段可點選）。
pub fn lifecycle_count_text(filter: LifecycleFilter, count: usize) -> String {
    format!("{} {count}", filter.label())
}

/// 生命週期數字列最後的提醒天數。
pub fn reminder_text(window: u32) -> String {
    format!("（提醒 {window} 天）")
}

impl GuiApp {
    pub(super) fn show_dashboard(&mut self, ui: &mut egui::Ui) {
        let tz = self.ws.services.tz;
        let d = &self.ws.data.dashboard;
        let mut jump = None;
        heading(ui, "未處理命中");
        ui.horizontal(|ui| {
            for (p, n) in &d.open_by_priority {
                if ui.button(format!("{}：{n}", p.as_str())).clicked() {
                    jump = Some(Jump::Priority(*p));
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button(format!("KEV 命中（未處理）：{}", d.kev_open))
                .clicked()
            {
                jump = Some(Jump::Kev);
            }
            ui.label(format!(
                "等待區 {}（逾期 {}）   來源矛盾 {}",
                d.pending, d.pending_stale, d.conflicts
            ));
        });
        let pending = &d.pending_versions;
        let text = pending_versions_text(pending);
        let button = egui::Button::new(if pending.any() {
            egui::RichText::new(text).color(ALERT)
        } else {
            egui::RichText::new(text)
        });
        if ui.add(button).clicked() {
            jump = Some(Jump::PendingVersions);
        }
        if let Some(filter) = lifecycle_row(ui, &self.ws.data.lifecycle_counts) {
            jump = Some(Jump::Lifecycle(filter));
        }
        if d.dead_notifications > 0 {
            ui.colored_label(
                ALERT,
                format!(
                    "無法送出的通知 {}（以 `cvewatch notify outbox` 檢視）",
                    d.dead_notifications
                ),
            );
        }
        ui.add_space(12.0);
        heading(ui, "各來源最後同步");
        if d.sources.is_empty() {
            ui.label("尚無同步紀錄。執行 `cvewatch sync` 或按「同步」在背景同步。");
        }
        egui::Grid::new("dashboard-sources")
            .num_columns(4)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                for s in &d.sources {
                    source_row(ui, s, tz);
                }
            });
        if let Some(j) = jump {
            self.jump(j);
        }
    }

    fn jump(&mut self, jump: Jump) {
        let base = Filter::default();
        self.ws.findings.filter = match jump {
            Jump::Priority(p) => base.with_priority_toggled(p),
            Jump::Kev => base.with_kev(KevFilter::Only),
            Jump::PendingVersions => {
                self.assets_pending_only = true;
                self.assets_show_retired = false;
                // 先前的生命週期篩選會讓部分版本待補資產看不到
                self.assets_lifecycle = None;
                return self.go(Screen::Assets);
            }
            Jump::Lifecycle(filter) => {
                self.assets_lifecycle = Some(filter);
                self.assets_pending_only = false;
                self.assets_show_retired = false;
                return self.go(Screen::Assets);
            }
        };
        self.ws.findings.search.clear();
        self.ws.refresh_visible();
        self.go(Screen::Findings);
    }
}

/// 「已 EoL N｜即將 EoL N｜保固或合約到期 N（提醒 90 天）」，各數字可點選；三者都為 0 時不顯示。
/// 回傳被點選的篩選。已 EoL 與到期數大於 0 時以警示色顯示。
fn lifecycle_row(ui: &mut egui::Ui, counts: &LifecycleCounts) -> Option<LifecycleFilter> {
    if counts.is_empty() {
        return None;
    }
    let mut picked = None;
    ui.horizontal(|ui| {
        for (i, filter) in LifecycleFilter::ALL.into_iter().enumerate() {
            if i > 0 {
                ui.label("｜");
            }
            let (count, alert) = match filter {
                LifecycleFilter::Eol => (counts.eol, counts.eol > 0),
                LifecycleFilter::EolSoon => (counts.eol_soon, false),
                LifecycleFilter::Expiry => (counts.expiry, counts.expiry > 0),
            };
            let text = egui::RichText::new(lifecycle_count_text(filter, count));
            let text = if alert { text.color(ALERT) } else { text };
            if ui.add(egui::Button::new(text)).clicked() {
                picked = Some(filter);
            }
        }
        ui.label(reminder_text(counts.window));
    });
    picked
}

/// 來源、最後一次結束的結果與時間、錯誤摘要、之後仍在執行的同步。
fn source_row(ui: &mut egui::Ui, s: &SourceStatus, tz: DisplayTz) {
    ui.label(&s.source);
    if s.has_finished() {
        if s.failed() {
            ui.colored_label(ALERT, "失敗");
        } else {
            ui.colored_label(OK, "成功");
        }
        ui.label(short_time(s.time(), tz));
    } else {
        ui.label("");
        ui.label("");
    }
    let mut extra = Vec::new();
    if let Some(e) = s.error.as_deref().filter(|_| s.failed()) {
        extra.push(one_line(e));
    }
    if let Some(since) = &s.running_since {
        extra.push(format!("執行中（開始 {}）", short_time(since, tz)));
    }
    ui.label(extra.join("  "));
    ui.end_row();
}
