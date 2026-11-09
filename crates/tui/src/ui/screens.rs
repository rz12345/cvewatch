//! Dashboard、Assets（含資產詳情）、Pending、Sync log 畫面。

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::findings::{cells, COLUMNS, HEADERS};
use super::{cut, full_time, scrolling_table, short_time, table, Scroll, TableSpec, Width};
use crate::app::App;
use crate::width::sanitize;
use cvewatch_core::matching::dashboard::{is_interrupted, SourceStatus};
use cvewatch_frontend::display::{class_label, rating_summary};

/// 資產畫面下方「未辨識產品」區塊最多顯示的列數。
const UNRESOLVED_ROWS: u16 = 6;
/// 資產詳情的元件區塊最多顯示的列數。
const COMPONENT_ROWS: u16 = 8;

fn bold(text: String) -> Line<'static> {
    Line::styled(text, Style::new().add_modifier(Modifier::BOLD))
}

pub fn dashboard(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let d = &app.data.dashboard;
    let priorities: Vec<String> = d
        .open_by_priority
        .iter()
        .map(|(p, n)| format!("{} {n}", p.as_str()))
        .collect();
    let mut lines = vec![
        bold(format!("未處理命中  {}", priorities.join("   "))),
        Line::raw(format!(
            "KEV 命中（未處理） {}   等待區 {}（逾期 {}）   來源矛盾 {}",
            d.kev_open, d.pending, d.pending_stale, d.conflicts
        )),
    ];
    if d.dead_notifications > 0 {
        lines.push(Line::styled(
            format!(
                "無法送出的通知 {}（以 `cvewatch notify outbox` 檢視）",
                d.dead_notifications
            ),
            Style::new().add_modifier(Modifier::BOLD),
        ));
    }
    lines.extend([Line::raw(""), bold("各來源最後同步".into())]);
    if d.sources.is_empty() {
        lines.push(Line::raw(
            "  尚無同步紀錄。執行 `cvewatch sync` 或按 S 在背景同步。",
        ));
    }
    for s in &d.sources {
        let line = source_line(app, s);
        lines.push(Line::raw(cut(app, &line, usize::from(area.width))));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// 來源的最後一次結束結果（成功／失敗與時間），之後仍在執行的同步另外標示。
fn source_line(app: &App, s: &SourceStatus) -> String {
    let tz = app.services.tz;
    let running = s.running_since.as_deref().map(|since| {
        format!(
            "{} 執行中（開始 {}）",
            app.glyphs.running,
            short_time(since, tz)
        )
    });
    if !s.has_finished() {
        return format!("  {:<16} {}", s.source, running.unwrap_or_default());
    }
    let (mark, label) = if s.failed() {
        (app.glyphs.fail, "失敗")
    } else {
        (app.glyphs.ok, "成功")
    };
    let error = s
        .error
        .as_deref()
        .filter(|_| s.failed())
        .map(|e| format!("  {}", sanitize(e)))
        .unwrap_or_default();
    let running = running.map(|r| format!("  {r}")).unwrap_or_default();
    format!(
        "  {:<16} {mark} {label}  {}{error}{running}",
        s.source,
        short_time(s.time(), tz)
    )
}

pub fn sync_log(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if app.data.sync_runs.is_empty() {
        frame.render_widget(Paragraph::new("尚無同步紀錄。"), area);
        return;
    }
    let row = |i: usize| {
        let r = &app.data.sync_runs[i];
        let status = match r.status.as_str() {
            "success" => "成功",
            "failed" => "失敗",
            _ if is_interrupted(r, app.data.loaded_at) => "中斷",
            _ => "執行中",
        };
        let values = vec![
            full_time(&r.started_at, app.services.tz),
            r.source.clone(),
            status.to_string(),
            r.added.to_string(),
            r.updated.to_string(),
            r.error.clone().unwrap_or_default(),
        ];
        (values, Style::new())
    };
    let cols = [
        Width::Fixed(16),
        Width::Fixed(15),
        Width::Fixed(6),
        Width::Fixed(6),
        Width::Fixed(6),
        Width::Flex(1),
    ];
    let headers = ["開始時間", "來源", "狀態", "新增", "更新", "訊息"];
    let spec = TableSpec {
        headers: &headers,
        cols: &cols,
    };
    let scroll = Scroll::new(app.data.sync_runs.len(), app.sync_cursor, &app.offsets.sync);
    scrolling_table(frame, area, app, &spec, &scroll, row);
}

pub fn assets(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let unresolved_height = if app.data.unresolved.is_empty() {
        0
    } else {
        UNRESOLVED_ROWS.min(u16::try_from(app.data.unresolved.len()).unwrap_or(u16::MAX)) + 2
    };
    let [top, bottom] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(unresolved_height)]).areas(area);
    if app.data.assets.is_empty() {
        frame.render_widget(
            Paragraph::new("沒有使用中的資產。以 `cvewatch asset import` 匯入清冊。"),
            top,
        );
    } else {
        let row = |i: usize| {
            let a = &app.data.assets[i];
            let r = &a.asset.record;
            let values = vec![
                r.asset_tag.clone(),
                r.name.clone(),
                r.category.as_str().to_string(),
                r.exposure.as_str().to_string(),
                r.cia.value().to_string(),
                a.components.len().to_string(),
                a.open_findings.to_string(),
                match a.unresolved.len() {
                    0 => String::new(),
                    n => n.to_string(),
                },
            ];
            (values, Style::new())
        };
        let cols = [
            Width::Flex(2),
            Width::Flex(3),
            Width::Fixed(6),
            Width::Fixed(8),
            Width::Fixed(4),
            Width::Fixed(4),
            Width::Fixed(6),
            Width::Fixed(6),
        ];
        let headers = [
            "資產",
            "名稱",
            "類別",
            "曝露度",
            "價值",
            "元件",
            "未處理",
            "未辨識",
        ];
        let spec = TableSpec {
            headers: &headers,
            cols: &cols,
        };
        let scroll = Scroll::new(
            app.data.assets.len(),
            app.assets_cursor,
            &app.offsets.assets,
        );
        scrolling_table(frame, top, app, &spec, &scroll, row);
    }
    if unresolved_height > 0 {
        let block = Block::new()
            .borders(Borders::TOP)
            .border_set(app.glyphs.border)
            .title(" 未辨識產品（可在 catalog.d 補別名） ");
        let inner = block.inner(bottom);
        frame.render_widget(block, bottom);
        let lines: Vec<Line<'static>> = app
            .data
            .unresolved
            .iter()
            .map(|u| {
                let text = format!("  「{}」 {}  {}", u.product_raw, u.version_raw, u.asset_tag);
                Line::raw(cut(app, &text, usize::from(inner.width)))
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

pub fn asset_detail(frame: &mut Frame<'_>, area: Rect, app: &App, tag: &str, cursor: usize) {
    let Some(entry) = app.data.asset(tag) else {
        frame.render_widget(
            Paragraph::new(format!("找不到資產 {tag}（請按 R 重新載入）")),
            area,
        );
        return;
    };
    let r = &entry.asset.record;
    let mut info = vec![
        bold(format!("{}（{}）", r.asset_tag, r.name)),
        Line::raw(format!(
            "類別 {}  曝露度 {}  {}  ISMS 編號 {}",
            class_label(&r.category, &app.settings.classes),
            r.exposure.as_str(),
            rating_summary(&r.cia),
            r.isms_id.as_deref().unwrap_or("-")
        )),
        Line::raw(format!(
            "資產說明 {}  地點 {}  保管人 {}",
            r.system_code
                .as_deref()
                .map_or_else(|| "-".to_string(), |c| app.data.systems.label(c)),
            r.location.as_deref().unwrap_or("-"),
            r.custodian.as_deref().unwrap_or("-")
        )),
    ];
    for u in &entry.unresolved {
        info.push(Line::raw(format!(
            "未辨識產品「{}」 {}",
            u.product_raw, u.version_raw
        )));
    }
    let info_height = u16::try_from(info.len()).unwrap_or(u16::MAX);
    let component_height =
        COMPONENT_ROWS.min(u16::try_from(entry.components.len()).unwrap_or(u16::MAX)) + 2;
    let [info_area, comp_area, find_area] = Layout::vertical([
        Constraint::Length(info_height),
        Constraint::Length(component_height),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(info), info_area);
    let comp_rows = entry
        .components
        .iter()
        .map(|c| {
            let values = vec![
                c.label().to_string(),
                c.version_raw.clone(),
                c.source.as_str().to_string(),
                full_time(&c.updated_at, app.services.tz),
            ];
            (values, Style::new())
        })
        .collect();
    let comp_cols = [
        Width::Flex(1),
        Width::Fixed(20),
        Width::Fixed(8),
        Width::Fixed(16),
    ];
    let comp_block = Block::new()
        .borders(Borders::TOP)
        .border_set(app.glyphs.border)
        .title(format!(" 元件 {} ", entry.components.len()));
    let comp_inner = comp_block.inner(comp_area);
    frame.render_widget(comp_block, comp_area);
    let t = table(
        app,
        &["元件", "版本", "來源", "更新時間"],
        &comp_cols,
        comp_inner.width,
        comp_rows,
    );
    frame.render_widget(t, comp_inner);
    asset_findings(frame, find_area, app, tag, cursor);
}

fn asset_findings(frame: &mut Frame<'_>, area: Rect, app: &App, tag: &str, cursor: usize) {
    let findings = app.data.asset_findings(tag);
    let block = Block::new()
        .borders(Borders::TOP)
        .border_set(app.glyphs.border)
        .title(format!(" 命中 {}（全部狀態） ", findings.len()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if findings.is_empty() {
        frame.render_widget(Paragraph::new("沒有命中。"), inner);
        return;
    }
    let spec = TableSpec {
        headers: &HEADERS,
        cols: &COLUMNS,
    };
    let scroll = Scroll::new(findings.len(), cursor, &app.offsets.asset_findings);
    scrolling_table(frame, inner, app, &spec, &scroll, |i| {
        (cells(app, findings[i], false), Style::new())
    });
}

pub fn pending(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if app.data.pending.is_empty() {
        frame.render_widget(Paragraph::new("等待區是空的。"), area);
        return;
    }
    let now = app.data.loaded_at;
    let row = |i: usize| {
        let p = &app.data.pending[i];
        let days = cvewatch_frontend::display::pending_days(p, now)
            .map_or_else(|| "-".into(), |d| format!("{d} 天"));
        let values = vec![
            p.cve_id.clone(),
            p.kev_date.clone().unwrap_or_else(|| "-".into()),
            p.reason.label().to_string(),
            days,
            if p.stale { "逾期" } else { "" }.to_string(),
            p.title.clone().unwrap_or_default(),
        ];
        let style = if p.kev_date.is_some() {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        (values, style)
    };
    let cols = [
        Width::Fixed(16),
        Width::Fixed(10),
        Width::Fixed(12),
        Width::Fixed(8),
        Width::Fixed(4),
        Width::Flex(1),
    ];
    let headers = ["CVE", "KEV", "原因", "進入", "逾期", "說明"];
    let spec = TableSpec {
        headers: &headers,
        cols: &cols,
    };
    let scroll = Scroll::new(
        app.data.pending.len(),
        app.pending_cursor,
        &app.offsets.pending,
    );
    scrolling_table(frame, area, app, &spec, &scroll, row);
}
