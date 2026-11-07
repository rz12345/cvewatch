//! 畫面繪製：外框（標題、最後同步、分頁）、狀態列、各畫面與對話框。

mod detail;
mod findings;
mod overlay;
mod screens;
mod settings;

use chrono::DateTime;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::cell::Cell as StdCell;
use std::ops::Range;

use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{App, Screen, View};
use crate::data::SYNC_LOG_LIMIT;
use crate::services::DisplayTz;
use crate::width::{display_width, truncate_with};

/// 欄間距。
const COLUMN_SPACING: u16 = 1;
/// 彈性欄位的最小寬度。
const MIN_FLEX_WIDTH: u16 = 4;

/// 明文傳輸例外啟用時，所有畫面頂端常駐的警示。
pub const INSECURE_BANNER: &str =
    " 明文傳輸已啟用（設定檔 allow_insecure_transport = true）：SMTP／Redmine 可能未加密 ";

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let insecure = app.config.notify.allow_insecure_transport;
    let [banner, body, status] = Layout::vertical([
        Constraint::Length(u16::from(insecure)),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    if insecure {
        frame.render_widget(
            Paragraph::new(cut(app, INSECURE_BANNER, usize::from(banner.width))).style(
                Style::new()
                    .fg(Color::White)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            banner,
        );
    }
    let block = Block::bordered()
        .border_set(app.glyphs.border)
        .title(title(app))
        .title_top(Line::from(last_sync(app)).right_aligned())
        .title_bottom(tabs(app));
    let inner = block.inner(body);
    frame.render_widget(block, body);
    match app.view() {
        Some(View::FindingDetail { data, scroll }) => {
            detail::draw(frame, inner, app, data, *scroll)
        }
        Some(View::AssetDetail { asset_tag, cursor }) => {
            screens::asset_detail(frame, inner, app, asset_tag, *cursor)
        }
        None => match app.screen {
            Screen::Dashboard => screens::dashboard(frame, inner, app),
            Screen::Findings => findings::list(frame, inner, app),
            Screen::Assets => screens::assets(frame, inner, app),
            Screen::Pending => screens::pending(frame, inner, app),
            Screen::SyncLog => screens::sync_log(frame, inner, app),
            Screen::Settings => settings::draw(frame, inner, app),
        },
    }
    status_line(frame, status, app);
    overlay::draw(frame, app);
}

fn title(app: &App) -> String {
    let sub = match app.view() {
        Some(View::FindingDetail { data, .. }) => format!("命中 #{}", data.finding.row.id),
        Some(View::AssetDetail { asset_tag, .. }) => format!("資產 {asset_tag}"),
        None => app.screen.title().to_string(),
    };
    format!(" cvewatch {} {sub} ", app.glyphs.separator)
}

/// 最近一次同步的時間；任一來源最近一次失敗即標示失敗與來源數（不只看最後結束的來源）。
fn last_sync(app: &App) -> String {
    let d = &app.data.dashboard;
    let failed = d.sources.iter().filter(|s| s.failed()).count();
    match d.last_finished() {
        Some(s) if failed > 0 => format!(
            " last sync {} {} {failed} 個來源失敗 ",
            short_time(s.time(), app.services.tz),
            app.glyphs.fail
        ),
        Some(s) => format!(
            " last sync {} {} ",
            short_time(s.time(), app.services.tz),
            app.glyphs.ok
        ),
        None => " 尚未同步 ".into(),
    }
}

fn tabs(app: &App) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (i, s) in Screen::ALL.iter().enumerate() {
        let text = format!("[{}]{}", i + 1, s.tab());
        let style = if *s == app.screen {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        spans.push(Span::styled(text, style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::raw(" ?help q quit "));
    Line::from(spans)
}

fn status_line(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut right = Vec::new();
    if app.sync.is_some() {
        right.push(format!("sync 執行中{}", app.glyphs.running));
    }
    if app.new_data {
        right.push("有新資料，按 R 重新整理".to_string());
    }
    let right = right.join("  ");
    let right_width = u16::try_from(display_width(&right)).unwrap_or(area.width);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_width)]).areas(area);
    if let Some(input) = &app.ws.findings.view.search_input {
        let text = format!("/{}", input.value());
        frame.render_widget(Paragraph::new(text), left_area);
        let x = left_area.x + 1 + u16::try_from(input.visual_cursor()).unwrap_or(0);
        frame.set_cursor_position((x.min(left_area.right().saturating_sub(1)), left_area.y));
    } else {
        let left = app.message.clone().unwrap_or_else(|| hint(app));
        let width = usize::from(left_area.width);
        frame.render_widget(Paragraph::new(cut(app, &left, width)), left_area);
    }
    frame.render_widget(
        Paragraph::new(right).style(Style::new().add_modifier(Modifier::BOLD)),
        right_area,
    );
}

fn hint(app: &App) -> String {
    if app.view().is_none() && app.screen == Screen::SyncLog {
        return format!("最近 {SYNC_LOG_LIMIT} 筆同步紀錄");
    }
    let text = match app.view() {
        Some(View::FindingDetail { .. }) => {
            "j/k 捲動  r 已處理  i 忽略  u 未處理  o 開啟連結  y 複製  Esc 返回"
        }
        Some(View::AssetDetail { .. }) => "j/k 移動  Enter 命中詳情  Esc 返回",
        None => match app.screen {
            Screen::Dashboard => "1-6 切換畫面  R 重新載入  S 背景 sync  ? 說明",
            Screen::Findings => {
                "Enter 詳情  Space 多選  / 搜尋  f 篩選  s 排序  r/i/u 狀態  o 連結  E 匯出"
            }
            Screen::Assets => "Enter 資產詳情（元件、命中、未辨識產品）",
            Screen::Pending => "a 套用到資產  d 標為不相關  o 開啟連結  y 複製",
            Screen::SyncLog => "",
            Screen::Settings => {
                "Tab 切換分頁  j/k 移動  Enter 編輯／切換  Del 清除  Ctrl+S 儲存  Esc 離開"
            }
        },
    };
    text.to_string()
}

/// 依顯示寬度截斷，截斷標記依終端機（Windows Terminal `…`、conhost `~`）。
pub fn cut(app: &App, s: &str, width: usize) -> String {
    truncate_with(s, width, app.glyphs.ellipsis)
}

/// 時間顯示：轉為本機時區 `YYYY-MM-DD HH:MM`；無法解析時顯示原字串前 16 字元。
pub fn full_time(raw: &str, tz: DisplayTz) -> String {
    DateTime::parse_from_rfc3339(raw).map_or_else(
        |_| raw.get(..16).unwrap_or(raw).to_string(),
        |t| tz.format(t, "%Y-%m-%d %H:%M"),
    )
}

/// `MM-DD HH:MM`。
pub fn short_time(raw: &str, tz: DisplayTz) -> String {
    let full = full_time(raw, tz);
    full.get(5..).unwrap_or(&full).to_string()
}

/// 欄寬：固定或彈性（權重）。
#[derive(Debug, Clone, Copy)]
pub enum Width {
    Fixed(u16),
    Flex(u16),
}

/// 依可用寬度分配欄寬：固定欄照給，剩餘寬度依權重分給彈性欄。
pub fn widths(cols: &[Width], total: u16) -> Vec<u16> {
    let gaps = COLUMN_SPACING * u16::try_from(cols.len().saturating_sub(1)).unwrap_or(0);
    let fixed: u16 = cols
        .iter()
        .map(|c| match c {
            Width::Fixed(w) => *w,
            Width::Flex(_) => 0,
        })
        .sum();
    let weights: u16 = cols
        .iter()
        .map(|c| match c {
            Width::Flex(w) => *w,
            Width::Fixed(_) => 0,
        })
        .sum();
    let rest = total.saturating_sub(fixed + gaps);
    cols.iter()
        .map(|c| match c {
            Width::Fixed(w) => *w,
            Width::Flex(w) => (rest * w / weights.max(1)).max(MIN_FLEX_WIDTH),
        })
        .collect()
}

/// 表格：每個儲存格依欄寬截斷（中文以顯示寬度計算並加省略符號）。
pub fn table(
    app: &App,
    headers: &[&str],
    cols: &[Width],
    total: u16,
    rows: Vec<(Vec<String>, Style)>,
) -> Table<'static> {
    let highlight = u16::try_from(display_width(app.glyphs.highlight)).unwrap_or(0);
    let ws = widths(cols, total.saturating_sub(highlight));
    let cells = |values: Vec<String>| -> Vec<Cell<'static>> {
        values
            .iter()
            .zip(&ws)
            .map(|(v, w)| Cell::from(cut(app, v, usize::from(*w))))
            .collect()
    };
    let header = Row::new(cells(headers.iter().map(|h| h.to_string()).collect()))
        .style(Style::new().add_modifier(Modifier::BOLD));
    let body: Vec<Row<'static>> = rows
        .into_iter()
        .map(|(values, style)| Row::new(cells(values)).style(style))
        .collect();
    Table::new(body, ws.iter().map(|w| Constraint::Length(*w)))
        .header(header)
        .column_spacing(COLUMN_SPACING)
        .highlight_symbol(app.glyphs.highlight)
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
}

/// 表格欄位定義。
pub struct TableSpec<'a> {
    pub headers: &'a [&'a str],
    pub cols: &'a [Width],
}

/// 捲動狀態：列數、游標與保存的捲動位置。
pub struct Scroll<'a> {
    pub len: usize,
    pub cursor: usize,
    pub offset: &'a StdCell<usize>,
}

impl<'a> Scroll<'a> {
    pub fn new(len: usize, cursor: usize, offset: &'a StdCell<usize>) -> Self {
        Scroll {
            len,
            cursor,
            offset,
        }
    }
}

/// 依游標調整捲動位置（游標離開可見範圍才捲動），回傳要顯示的列範圍。
pub fn window(offset: &StdCell<usize>, cursor: usize, len: usize, height: usize) -> Range<usize> {
    if len == 0 || height == 0 {
        offset.set(0);
        return 0..0;
    }
    let cursor = cursor.min(len - 1);
    let mut start = offset.get().min(len.saturating_sub(height));
    if cursor < start {
        start = cursor;
    } else if cursor >= start + height {
        start = cursor + 1 - height;
    }
    offset.set(start);
    start..(start + height).min(len)
}

/// 可捲動的表格：只建立可見範圍的列（資料量數千列時不必每次重建全部）。
pub fn scrolling_table(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    spec: &TableSpec<'_>,
    scroll: &Scroll<'_>,
    row: impl Fn(usize) -> (Vec<String>, Style),
) {
    // 扣除表頭一列
    let height = usize::from(area.height.saturating_sub(1));
    let range = window(scroll.offset, scroll.cursor, scroll.len, height);
    let selected = scroll.cursor.checked_sub(range.start);
    let rows = range.map(row).collect();
    let t = table(app, spec.headers, spec.cols, area.width, rows);
    let mut state = TableState::default().with_selected(selected);
    frame.render_stateful_widget(t, area, &mut state);
}

/// 置中的對話框區域。
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flex_columns_share_remaining_width() {
        let w = widths(
            &[Width::Fixed(4), Width::Flex(1), Width::Flex(3)],
            4 + 2 + 40,
        );
        assert_eq!(w, [4, 10, 30]);
        let narrow = widths(&[Width::Fixed(30), Width::Flex(1)], 20);
        assert_eq!(narrow, [30, MIN_FLEX_WIDTH]);
    }

    #[test]
    fn window_scrolls_only_when_cursor_leaves_view() {
        let off = StdCell::new(0);
        assert_eq!(window(&off, 0, 100, 10), 0..10);
        assert_eq!(window(&off, 9, 100, 10), 0..10);
        assert_eq!(window(&off, 10, 100, 10), 1..11, "往下超出一列才捲一列");
        assert_eq!(window(&off, 99, 100, 10), 90..100);
        assert_eq!(
            window(&off, 95, 100, 10),
            90..100,
            "往上移動但仍在畫面內：不捲動"
        );
        assert_eq!(window(&off, 89, 100, 10), 89..99);
        assert_eq!(window(&off, 3, 5, 10), 0..5, "清單變短時回到頂端");
        assert_eq!(window(&off, 0, 0, 10), 0..0);
    }

    #[test]
    fn times_use_display_timezone() {
        let tz = DisplayTz::Fixed(chrono::FixedOffset::east_opt(8 * 3600).unwrap());
        assert_eq!(
            full_time("2026-11-01T06:05:00+00:00", tz),
            "2026-11-01 14:05"
        );
        assert_eq!(short_time("2026-11-01T06:05:00+00:00", tz), "11-01 14:05");
        assert_eq!(full_time("not a time", tz), "not a time");
    }
}
