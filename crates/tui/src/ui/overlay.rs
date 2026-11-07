//! 對話框與說明畫面（覆蓋在主畫面上）。

use cvewatch_core::matching::pending_ops::Action;
use cvewatch_core::matching::triage::{IgnoreReason, Justification};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use tui_input::Input;

use super::centered;
use crate::app::App;
use crate::dialog::{change_detail, filter_rows, Dialog, FilterRow, NotePurpose};
use crate::filter::Filter;

const DIALOG_WIDTH: u16 = 64;

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    if app.help {
        help(frame, app);
        return;
    }
    let Some(d) = &app.dialog else {
        return;
    };
    match d {
        Dialog::Reason { cursor, .. } => {
            let items = IgnoreReason::ALL
                .iter()
                .map(|r| format!("{}（{}）", r.label(), r.as_str()))
                .collect();
            menu(frame, app, " 忽略原因 ", items, *cursor);
        }
        Dialog::Justification { cursor, .. } => {
            let items = Justification::ALL
                .iter()
                .map(|j| format!("{}（{}）", j.label(), j.as_str()))
                .collect();
            menu(frame, app, " 不受影響的理由 ", items, *cursor);
        }
        Dialog::Note { purpose, input } => note(frame, app, purpose, input),
        Dialog::Links { links, cursor } => {
            let items = links
                .iter()
                .map(|l| format!("{}  {}", l.label, l.url))
                .collect();
            menu(frame, app, " 開啟連結 ", items, *cursor);
        }
        Dialog::Filter { draft, cursor, .. } => {
            let items = filter_rows()
                .iter()
                .map(|r| filter_item(draft, *r))
                .collect();
            menu(
                frame,
                app,
                &format!(
                    " 篩選（Space/{} 變更，Enter 套用，Esc 取消） ",
                    app.glyphs.left_right
                ),
                items,
                *cursor,
            );
        }
        Dialog::PendingTarget {
            cve,
            options,
            cursor,
        } => {
            menu(
                frame,
                app,
                &format!(" {cve} 套用到資產 "),
                options.clone(),
                *cursor,
            );
        }
        Dialog::ConfirmQuit { .. } => confirm_quit(frame, app),
    }
}

fn popup(frame: &mut Frame<'_>, app: &App, title: &str, height: u16) -> Rect {
    let area = centered(frame.area(), DIALOG_WIDTH, height);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_set(app.glyphs.border)
        .title(title.to_string());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn menu(frame: &mut Frame<'_>, app: &App, title: &str, items: Vec<String>, cursor: usize) {
    let height = u16::try_from(items.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let inner = popup(frame, app, title, height);
    let list = List::new(items.into_iter().map(ListItem::new))
        .highlight_symbol(app.glyphs.highlight)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default().with_selected(Some(cursor));
    frame.render_stateful_widget(list, inner, &mut state);
}

fn check(on: bool) -> &'static str {
    if on {
        "[x]"
    } else {
        "[ ]"
    }
}

fn filter_item(f: &Filter, row: FilterRow) -> String {
    match row {
        FilterRow::Status(s) => format!("狀態 {} {}", check(f.statuses.contains(&s)), s.label()),
        FilterRow::Priority(p) => {
            format!("等級 {} {}", check(f.priorities.contains(&p)), p.as_str())
        }
        FilterRow::Confidence => format!("信心 {}", f.confidence.map_or("全部", |c| c.as_str())),
        FilterRow::Kev => format!("KEV  {}", f.kev.label()),
        FilterRow::Asset => format!("資產 {}", f.asset.as_deref().unwrap_or("全部")),
        FilterRow::Reset => "重設為預設（只看未處理）".into(),
    }
}

fn note_title(purpose: &NotePurpose) -> String {
    match purpose {
        NotePurpose::Triage {
            targets,
            status,
            reason,
            justification,
        } => {
            let detail = change_detail(*reason, *justification);
            format!("將 {} 筆命中設為{}{detail}", targets.len(), status.label())
        }
        NotePurpose::Pending { cve, action } => match action {
            Action::Apply(target) => format!("{cve} 套用到 {target}（建立 probable 命中）"),
            Action::Dismiss => format!("{cve} 標為不相關（移出等待區）"),
        },
    }
}

fn note(frame: &mut Frame<'_>, app: &App, purpose: &NotePurpose, input: &Input) {
    let inner = popup(frame, app, " 備註 ", 7);
    let [desc, field, hint] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(2),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(note_title(purpose)).wrap(Wrap { trim: false }),
        desc,
    );
    let width = usize::from(field.width.saturating_sub(3));
    let scroll = input.visual_scroll(width);
    let shown = Paragraph::new(format!("> {}", input.value()))
        .scroll((0, u16::try_from(scroll).unwrap_or(0)))
        .style(Style::new().add_modifier(Modifier::UNDERLINED));
    frame.render_widget(shown, field);
    let x = field.x + 2 + u16::try_from(input.visual_cursor().saturating_sub(scroll)).unwrap_or(0);
    frame.set_cursor_position((x.min(field.right().saturating_sub(1)), field.y));
    frame.render_widget(Paragraph::new("Enter 確認  Esc 取消（不變更）"), hint);
}

fn confirm_quit(frame: &mut Frame<'_>, app: &App) {
    let inner = popup(frame, app, " 離開 ", 3);
    frame.render_widget(
        Paragraph::new("備註尚未送出，確定離開？ y 離開 / n 返回"),
        inner,
    );
}

fn help_entries(app: &App) -> Vec<(String, &'static str)> {
    let move_keys = format!("{} / j k", app.glyphs.up_down);
    let rows: [(&str, &'static str); 19] = [
        (
            "1-5",
            "切換畫面：Dashboard / Findings / Assets / Pending / Sync log",
        ),
        ("MOVE", "移動（PgUp/PgDn、g/G 到頭尾）"),
        ("Enter", "進入詳情"),
        ("Esc / h", "返回（Findings：清除多選與搜尋）"),
        ("/", "即時搜尋（CVE ID、資產、描述），Enter 完成、Esc 清除"),
        ("f", "篩選面板（狀態、等級、資產、信心、KEV）"),
        ("s", "切換排序"),
        ("Space", "多選"),
        ("r", "設為已處理（輸入備註）"),
        ("i", "設為忽略（選原因；不受影響時再選理由）"),
        ("u", "設為未處理"),
        ("o", "以預設瀏覽器開啟 CVE.org / NVD / 原廠公告"),
        ("y", "複製 CVE ID"),
        ("a / d", "等待區：套用到資產 / 標為不相關"),
        ("R", "重新載入資料"),
        ("S", "背景執行 sync"),
        ("?", "說明"),
        ("q", "離開"),
        ("Ctrl+C", "離開（有未送出的備註時先確認）"),
    ];
    rows.into_iter()
        .map(|(k, v)| {
            let key = if k == "MOVE" {
                move_keys.clone()
            } else {
                k.to_string()
            };
            (key, v)
        })
        .collect()
}

fn help(frame: &mut Frame<'_>, app: &App) {
    let entries = help_entries(app);
    let height = u16::try_from(entries.len()).unwrap_or(u16::MAX) + 3;
    let area = centered(frame.area(), 76, height);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_set(app.glyphs.border)
        .title(" 說明（按任意鍵關閉） ");
    let mut lines: Vec<Line<'static>> = entries
        .iter()
        .map(|(k, v)| Line::raw(format!(" {k:<10} {v}")))
        .collect();
    lines.push(Line::raw(
        " 標記：? probable（版本資訊不足）、! 已處理但仍在受影響範圍",
    ));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
