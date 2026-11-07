//! 設定畫面：分頁標籤、欄位列（錯誤欄位標紅）、鍵位與測試結果；挑選清單與未儲存確認框。

use cvewatch_core::notify::settings::SmtpSecurity;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use super::{centered, cut};
use crate::app::App;
use crate::settings::{FieldId, Kind, Row, Tab, CONFIRM_CHOICES};
use crate::width::display_width;

/// 欄位名稱欄寬（顯示寬度）。
const LABEL_WIDTH: usize = 20;
/// 遮罩字元。
const MASK: char = '●';
const PICKER_WIDTH: u16 = 56;
const PICKER_HEIGHT: u16 = 16;

fn error_style() -> Style {
    Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)
}

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let [tabs, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(4),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(tab_line(app)), tabs);
    rows(frame, body, app);
    frame.render_widget(Paragraph::new(footer_lines(app)), footer);
    picker(frame, app);
    confirm(frame, app);
}

fn tab_line(app: &App) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for t in Tab::ALL {
        let style = if t == app.page.tab {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        spans.push(Span::styled(format!(" {} ", t.title()), style));
        spans.push(Span::raw(" "));
    }
    if app.page.dirty() {
        spans.push(Span::styled(
            "● 有未儲存的變更",
            Style::new().add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn pad(label: &str) -> String {
    let w = display_width(label);
    format!("{label}{}", " ".repeat(LABEL_WIDTH.saturating_sub(w)))
}

fn rows(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let page = &app.page;
    let errors = page.errors();
    let mut lines = Vec::new();
    let mut cursor_pos = None;
    for (i, row) in page.rows().iter().enumerate() {
        let (label, value) = row_text(app, *row);
        let editing = page
            .editing
            .as_ref()
            .filter(|e| e.row == *row && i == page.cursor);
        let value = match editing {
            Some(e) if e.masked => MASK.to_string().repeat(e.input.value().chars().count()),
            Some(e) => e.input.value().to_string(),
            None => value,
        };
        if let Some(e) = editing {
            let offset = if e.masked {
                e.input.value().chars().count() * display_width(&MASK.to_string())
            } else {
                e.input.visual_cursor()
            };
            cursor_pos = Some((LABEL_WIDTH + 2 + offset, i));
        }
        let error = match row {
            Row::Field(f) => errors.get(f),
            Row::Recipient(_) | Row::AddRecipient => errors.get(&FieldId::To).filter(|_| {
                // 收件者的錯誤只標在第一列收件者（或「新增」列）
                matches!(row, Row::Recipient(0))
                    || (page.form.to.is_empty() && *row == Row::AddRecipient)
            }),
        };
        let mut spans = vec![Span::raw(format!(" {}", pad(&label))), Span::raw(" ")];
        let value_style = if error.is_some() {
            error_style()
        } else {
            Style::new()
        };
        spans.push(Span::styled(value, value_style));
        if let Some(e) = error {
            spans.push(Span::styled(format!("  ✗ {e}"), error_style()));
        }
        let mut line = Line::from(spans);
        if i == page.cursor && page.editing.is_none() && page.picker.is_none() {
            line = line.style(Style::new().add_modifier(Modifier::REVERSED));
        }
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), area);
    if let Some((x, y)) = cursor_pos {
        let x = area.x + u16::try_from(x).unwrap_or(area.width);
        let y = area.y + u16::try_from(y).unwrap_or(0);
        frame.set_cursor_position((x.min(area.right().saturating_sub(1)), y));
    }
}

fn checkbox(on: bool) -> String {
    if on { "[x]" } else { "[ ]" }.to_string()
}

fn row_text(app: &App, row: Row) -> (String, String) {
    let form = &app.page.form;
    match row {
        Row::Recipient(i) => (
            if i == 0 { FieldId::To.label() } else { "" }.to_string(),
            form.to.get(i).cloned().unwrap_or_default(),
        ),
        Row::AddRecipient => (
            if form.to.is_empty() {
                FieldId::To.label()
            } else {
                ""
            }
            .to_string(),
            "[+ 新增]".into(),
        ),
        Row::Field(f) => (f.label().to_string(), field_value(app, f)),
    }
}

fn field_value(app: &App, f: FieldId) -> String {
    let form = &app.page.form;
    match f.kind() {
        Kind::Toggle => checkbox(match f {
            FieldId::EmailEnabled => form.email_enabled,
            FieldId::RedmineEnabled => form.redmine_enabled,
            FieldId::NoteOnTriage => form.note_on_triage,
            _ => form.send_empty,
        }),
        Kind::Radio => match form.security {
            SmtpSecurity::Starttls => "(•) STARTTLS  ( ) TLS".into(),
            SmtpSecurity::Tls => "( ) STARTTLS  (•) TLS".into(),
            SmtpSecurity::None => "無加密（設定檔例外；Enter 改為 STARTTLS）".into(),
        },
        Kind::Secret => {
            let secret = if f == FieldId::Password {
                &form.smtp_password
            } else {
                &form.api_key
            };
            format!("{}  [Enter 變更] [Del 清除]", secret.status())
        }
        Kind::Pick => app.page.pick_label(f),
        Kind::Text if form.text(f).is_empty() => "（未設定）".into(),
        Kind::Text => form.text(f).to_string(),
    }
}

fn footer_lines(app: &App) -> Vec<Line<'static>> {
    let page = &app.page;
    let keys = match page.tab {
        Tab::Email => "[t] 測試寄信  [Ctrl+S] 儲存  [Esc] 離開  Tab 切換分頁",
        Tab::Redmine => "[l] 載入清單  [t] 測試 Redmine  [Ctrl+S] 儲存  [Esc] 離開",
        Tab::Schedule => "[i] 重新安裝排程  [Ctrl+S] 儲存  [Esc] 離開  Tab 切換分頁",
    };
    let mut lines = vec![Line::raw(format!(" {keys}"))];
    if page.schedule_pending {
        lines.push(Line::styled(
            " 排程已變更，按 i 重新安裝排程",
            Style::new().add_modifier(Modifier::BOLD),
        ));
    }
    if page.tab == Tab::Redmine {
        if let Some(e) = &page.lists_error {
            lines.push(Line::styled(
                cut(
                    app,
                    &format!(" 無法載入清單：{e}（可直接輸入數字 ID）"),
                    200,
                ),
                error_style(),
            ));
        }
    }
    if let Some((ok, text)) = &page.last_result {
        let style = if *ok { Style::new() } else { error_style() };
        lines.push(Line::styled(format!(" {text}"), style));
    }
    if !page.jobs.is_empty() {
        let names: Vec<&str> = page.jobs.iter().map(|j| j.kind.label()).collect();
        lines.push(Line::raw(format!(
            " 背景執行中：{}{}",
            names.join("、"),
            app.glyphs.running
        )));
    }
    lines
}

fn picker(frame: &mut Frame<'_>, app: &App) {
    let Some(p) = &app.page.picker else {
        return;
    };
    let area = centered(frame.area(), PICKER_WIDTH, PICKER_HEIGHT);
    frame.render_widget(Clear, area);
    let width = usize::from(area.width.saturating_sub(4));
    let items: Vec<ListItem<'static>> = p
        .items
        .iter()
        .map(|n| ListItem::new(cut(app, &n.label(), width)))
        .collect();
    let list = List::new(items)
        .block(
            Block::bordered()
                .border_set(app.glyphs.border)
                .title(format!(" 選擇{}（Enter 選定，Esc 取消） ", p.field.label())),
        )
        .highlight_symbol(app.glyphs.highlight)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default().with_selected(Some(p.cursor));
    frame.render_stateful_widget(list, area, &mut state);
}

fn confirm(frame: &mut Frame<'_>, app: &App) {
    let Some(c) = &app.page.confirm else {
        return;
    };
    let area = centered(frame.area(), 46, 5);
    frame.render_widget(Clear, area);
    let mut spans = vec![Span::raw(" ")];
    for (i, choice) in CONFIRM_CHOICES.iter().enumerate() {
        let style = if i == c.cursor {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        let key = ["s", "d", "c"][i];
        spans.push(Span::styled(format!("[{key}] {choice}"), style));
        spans.push(Span::raw("  "));
    }
    let text = vec![Line::raw(" 設定有未儲存的變更。"), Line::from(spans)];
    frame.render_widget(
        Paragraph::new(text).block(
            Block::bordered()
                .border_set(app.glyphs.border)
                .title(" 未儲存的變更 "),
        ),
        area,
    );
}
