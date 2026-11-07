//! Findings 清單：篩選摘要列 + 表格（`等級 | KEV | ? | CVE | CVSS | EPSS | 資產 | 元件@版本 | 狀態 | 首次發現`）。

use cvewatch_core::matching::engine::Confidence;
use cvewatch_core::matching::query::FindingRow;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::{cut, full_time, scrolling_table, Scroll, TableSpec, Width};
use crate::app::App;

pub const HEADERS: [&str; 11] = [
    "",
    "等級",
    "KEV",
    "?",
    "CVE",
    "CVSS",
    "EPSS",
    "資產",
    "元件@版本",
    "狀態",
    "首次發現",
];

pub const COLUMNS: [Width; 11] = [
    Width::Fixed(1),
    Width::Fixed(4),
    Width::Fixed(3),
    Width::Fixed(1),
    Width::Fixed(16),
    Width::Fixed(4),
    Width::Fixed(5),
    Width::Flex(2),
    Width::Flex(3),
    Width::Fixed(6),
    Width::Fixed(10),
];

/// 一列的儲存格（全部欄位，含多選標記）。
pub fn cells(app: &App, r: &FindingRow, selected: bool) -> Vec<String> {
    let mut level = r.priority.as_str().to_string();
    if r.still_vulnerable {
        level.push('!');
    }
    vec![
        if selected { app.glyphs.mark } else { "" }.to_string(),
        level,
        if r.scores.kev_date.is_some() {
            "KEV"
        } else {
            ""
        }
        .to_string(),
        if r.confidence == Confidence::Probable {
            "?"
        } else {
            ""
        }
        .to_string(),
        r.cve_id.clone(),
        r.scores.cvss.map_or("-".into(), |v| format!("{v:.1}")),
        r.scores.epss.map_or("-".into(), |v| format!("{v:.3}")),
        r.asset_tag.clone(),
        format!("{}@{}", r.component, r.version),
        r.status.label().to_string(),
        full_time(&r.first_seen, app.services.tz)
            .get(..10)
            .unwrap_or_default()
            .to_string(),
    ]
}

pub fn list(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let [summary, body] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    let f = &app.ws.findings;
    // 畫面窄時從右側截斷：筆數、多選、搜尋放前面，篩選條件最後
    let mut parts = vec![format!("{} 筆", f.visible.len())];
    if !f.selected.is_empty() {
        let shown = app.visible_selection().len();
        let hidden = f.selected.len() - shown;
        parts.push(if hidden > 0 {
            format!("已選 {shown}（另 {hidden} 筆未顯示，不會變更）")
        } else {
            format!("已選 {shown}")
        });
    }
    if !f.search.is_empty() {
        parts.push(format!("搜尋「{}」", f.search));
    }
    parts.push(format!("排序 {}", f.sort.label()));
    parts.push(format!("篩選 {}", f.filter.summary()));
    let text = cut(app, &parts.join("｜"), usize::from(summary.width));
    frame.render_widget(
        Paragraph::new(text).style(Style::new().add_modifier(Modifier::DIM)),
        summary,
    );
    if f.visible.is_empty() {
        frame.render_widget(Paragraph::new("沒有符合條件的命中。"), body);
        return;
    }
    let spec = TableSpec {
        headers: &HEADERS,
        cols: &COLUMNS,
    };
    let scroll = Scroll::new(f.visible.len(), f.view.cursor, &app.offsets.findings);
    scrolling_table(frame, body, app, &spec, &scroll, |i| {
        let r = &app.data.findings[f.visible[i]];
        (cells(app, r, f.selected.contains(&r.id)), Style::new())
    });
}
