//! 生命週期的畫面元素（22 design D11）：資產表格「週期」欄、資產詳情的生命週期段落與
//! Dashboard 的計數行。項目文字沿用 core 的 `LifecycleItem::line()`（與 CLI 相同）。

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::cut;
use crate::app::App;
use crate::width::sanitize;
use cvewatch_core::lifecycle::{LifecycleCounts, LifecycleFlags, LifecycleItem};
use cvewatch_frontend::data::AssetEntry;

/// 資產表格「週期」欄寬：「⚠ 已 EoL」「⚠ 已到期」「即將 EoL」「即將到期」都是 8 欄。
pub const COLUMN_WIDTH: u16 = 8;
/// 資產詳情的生命週期段落最多顯示的項目列數（超過時最後一列提示其餘項數）。
const DETAIL_ROWS: usize = 10;

/// 「週期」欄：最嚴重的一個狀態（已 EoL > 已到期 > 即將 EoL > 即將到期）。已到的狀態加
/// 警示字元；即將到的狀態不加，使四種字樣都在欄寬 8 內。
pub fn column(app: &App, flags: LifecycleFlags) -> String {
    let warn = app.glyphs.warn;
    if flags.eol {
        format!("{warn} 已 EoL")
    } else if flags.expired {
        format!("{warn} 已到期")
    } else if flags.eol_soon {
        "即將 EoL".to_string()
    } else if flags.expiring {
        "即將到期".to_string()
    } else {
        String::new()
    }
}

/// Dashboard 的計數行：「即將 EoL 1、保固或合約到期 3（提醒 90 天）」；數字為 0 的項目省略，
/// 三者都為 0 時為 None（不顯示）。
pub fn dashboard_line(counts: &LifecycleCounts) -> Option<String> {
    if counts.is_empty() {
        return None;
    }
    let parts: Vec<String> = [
        ("已 EoL", counts.eol),
        ("即將 EoL", counts.eol_soon),
        ("保固或合約到期", counts.expiry),
    ]
    .into_iter()
    .filter(|(_, n)| *n > 0)
    .map(|(label, n)| format!("{label} {n}"))
    .collect();
    Some(format!("{}（提醒 {} 天）", parts.join("、"), counts.window))
}

/// 資產詳情生命週期段落的高度（上框線一列加項目列）。
pub fn detail_height(entry: &AssetEntry) -> u16 {
    let rows = entry.lifecycle.items.len().clamp(1, DETAIL_ROWS);
    u16::try_from(rows).unwrap_or(u16::MAX) + 1
}

/// 資產詳情的生命週期段落：每個項目一行（硬體、各清冊元件、保固、合約、授權）。
pub fn detail(frame: &mut Frame<'_>, area: Rect, app: &App, entry: &AssetEntry) {
    let block = Block::new()
        .borders(Borders::TOP)
        .border_set(app.glyphs.border)
        .title(format!(
            " 生命週期（提醒 {} 天） ",
            app.data.lifecycle_counts.window
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = usize::from(inner.width);
    let tag = &entry.asset.record.asset_tag;
    let lines = detail_lines(&entry.lifecycle.items, tag, |text| cut(app, text, width));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// 項目列；超過 [`DETAIL_ROWS`] 時最後一列改為「另有 N 項」提示。已到（已 EoL、已到期）的項目以粗體標示。
fn detail_lines(
    items: &[LifecycleItem],
    tag: &str,
    fit: impl Fn(&str) -> String,
) -> Vec<Line<'static>> {
    if items.is_empty() {
        return vec![Line::raw("沒有生命週期項目。")];
    }
    let shown = if items.len() > DETAIL_ROWS {
        DETAIL_ROWS - 1
    } else {
        items.len()
    };
    let mut lines: Vec<Line<'static>> = items[..shown]
        .iter()
        .map(|item| {
            // 覆寫來源為使用者輸入：與其他欄位一樣先轉成單行
            let text = fit(&sanitize(&item.line()));
            if item.status.is_past() {
                Line::styled(text, Style::new().add_modifier(Modifier::BOLD))
            } else {
                Line::raw(text)
            }
        })
        .collect();
    if shown < items.len() {
        let rest = items.len() - shown;
        lines.push(Line::raw(fit(&format!(
            "另有 {rest} 項，以 `cvewatch asset show {tag}` 檢視"
        ))));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_line_omits_zero_counts_and_hides_when_empty() {
        let counts = |eol, eol_soon, expiry| LifecycleCounts {
            eol,
            eol_soon,
            expiry,
            window: 90,
        };
        assert_eq!(dashboard_line(&counts(0, 0, 0)), None);
        assert_eq!(
            dashboard_line(&counts(0, 1, 3)).as_deref(),
            Some("即將 EoL 1、保固或合約到期 3（提醒 90 天）")
        );
        assert_eq!(
            dashboard_line(&counts(2, 0, 0)).as_deref(),
            Some("已 EoL 2（提醒 90 天）")
        );
    }
}
