//! 命中詳情：CVE 描述、各來源 CVSS／EPSS／KEV、證據與版本區間、來源矛盾、原廠修正版本、時間軸、連結。

use cvewatch_frontend::display::{self, Marks};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::data::DetailData;
use crate::width::sanitize;

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App, data: &DetailData, scroll: u16) {
    let paragraph = Paragraph::new(lines(app, data)).wrap(Wrap { trim: false });
    // 可捲動上限：換行後總行數減去可見高度（按鍵處理依此限制，不捲過結尾）
    let total = paragraph.line_count(area.width);
    let max = u16::try_from(total.saturating_sub(usize::from(area.height))).unwrap_or(u16::MAX);
    app.offsets.detail_max.set(max);
    frame.render_widget(paragraph.scroll((scroll.min(max), 0)), area);
}

/// 一行文字；外部資料中的控制字元（換行、tab）換成空白，避免前後文字黏在一起。
fn text(s: impl AsRef<str>) -> Line<'static> {
    Line::raw(sanitize(s.as_ref()))
}

fn heading(title: &str) -> Line<'static> {
    Line::styled(
        title.to_string(),
        Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )
}

/// 詳情內容（段落文字由 `cvewatch_frontend::display` 產生，與 GUI 共用）：摘要段第一行粗體，
/// 其他段以底線標題開頭，最後是連結。
pub fn lines(app: &App, data: &DetailData) -> Vec<Line<'static>> {
    let marks = Marks {
        warn: app.glyphs.warn,
        arrow: app.glyphs.arrow,
    };
    let mut out = Vec::new();
    for s in display::sections(data, app.services.tz, marks) {
        match s.title {
            None => out.extend(s.lines.iter().enumerate().map(|(i, l)| {
                if i == 0 {
                    Line::styled(sanitize(l), Style::new().add_modifier(Modifier::BOLD))
                } else {
                    text(l)
                }
            })),
            Some(title) => {
                out.push(text(""));
                out.push(heading(title));
                out.extend(s.lines.iter().map(text));
            }
        }
    }
    out.push(text(""));
    out.push(heading("連結（按 o 開啟）"));
    out.extend(
        data.links
            .iter()
            .map(|l| text(format!("  {}  {}", l.label, l.url))),
    );
    out
}
