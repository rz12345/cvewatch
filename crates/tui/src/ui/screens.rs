//! Dashboard、Assets（含資產詳情）、Pending、Sync log 畫面。

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::findings::{cells, COLUMNS, HEADERS};
use super::lifecycle;
use super::{cut, full_time, scrolling_table, short_time, table, Scroll, TableSpec, Width};
use crate::app::App;
use crate::width::sanitize;
use cvewatch_core::matching::dashboard::{is_interrupted, SourceStatus};
use cvewatch_frontend::component_groups::group_components;
use cvewatch_frontend::data::AssetEntry;
use cvewatch_frontend::display::{
    class_label, identity_rows, pending_versions_text, rating_summary, version_text,
};

/// 資產畫面下方「未辨識產品」區塊最多顯示的列數。
const UNRESOLVED_ROWS: u16 = 6;
/// 資產詳情的元件區塊最多顯示的列數。
const COMPONENT_ROWS: u16 = 8;
/// 元件表格「角色」欄寬：最長的組名「資料庫管理系統」為 7 個全形字。
const ROLE_WIDTH: u16 = 14;

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
    // 版本待補的元件未納入比對（18 design D7）：有時以粗體提醒
    let pending = pending_versions_text(&d.pending_versions);
    lines.push(if d.pending_versions.any() {
        bold(pending)
    } else {
        Line::raw(pending)
    });
    // 生命週期計數（22 design D11）：三者都為 0 時不顯示
    if let Some(text) = lifecycle::dashboard_line(&app.data.lifecycle_counts) {
        lines.push(bold(text));
    }
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
    asset_table(frame, top, app);
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
                let text = format!(
                    "  「{}」 {}  {}",
                    u.product_raw,
                    version_text(&u.version_raw),
                    u.asset_tag
                );
                Line::raw(cut(app, &text, usize::from(inner.width)))
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// 資產表格（套用生命週期篩選，見 [`App::visible_assets`]）。
fn asset_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let assets = app.visible_assets();
    if assets.is_empty() {
        let text = match app.assets_lifecycle_filter {
            Some(f) => format!(
                "沒有符合生命週期篩選「{}」的資產。按 l 切換篩選。",
                f.label()
            ),
            None => "沒有使用中的資產。以 `cvewatch asset import` 匯入清冊。".to_string(),
        };
        frame.render_widget(Paragraph::new(text), area);
        return;
    }
    let row = |i: usize| (asset_cells(app, assets[i]), Style::new());
    let cols = [
        Width::Flex(2),
        Width::Flex(3),
        Width::Fixed(6),
        Width::Fixed(8),
        Width::Fixed(4),
        Width::Fixed(4),
        Width::Fixed(6),
        Width::Fixed(6),
        Width::Fixed(lifecycle::COLUMN_WIDTH),
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
        "週期",
    ];
    let spec = TableSpec {
        headers: &headers,
        cols: &cols,
    };
    let scroll = Scroll::new(assets.len(), app.assets_cursor, &app.offsets.assets);
    scrolling_table(frame, area, app, &spec, &scroll, row);
}

fn asset_cells(app: &App, a: &AssetEntry) -> Vec<String> {
    let r = &a.asset.record;
    // 有版本待補元件的資產：名稱前加警示字元（寬度安全，見 glyphs.rs）
    let name = if a.has_pending() {
        format!("{} {}", app.glyphs.warn, r.name)
    } else {
        r.name.clone()
    };
    vec![
        r.asset_tag.clone(),
        name,
        r.category.as_str().to_string(),
        r.exposure.as_str().to_string(),
        r.cia.value().to_string(),
        a.components.len().to_string(),
        a.open_findings.to_string(),
        match a.unresolved.len() {
            0 => String::new(),
            n => n.to_string(),
        },
        lifecycle::column(app, a.lifecycle.flags()),
    ]
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
    // 識別欄位與 IP（20 design D12）：「廠牌 … 財產編號 …」「主機名稱 … IP …」兩行，未填為「-」
    let identity = identity_rows(r, &entry.ips);
    for chunk in [&identity[..4], &identity[4..]] {
        // 清冊值可能含換行或控制字元（Excel 的 Alt+Enter）：與其他欄位一樣先轉成單行
        let text: Vec<String> = chunk
            .iter()
            .map(|(l, v)| format!("{l} {}", sanitize(v)))
            .collect();
        info.push(Line::raw(text.join("  ")));
    }
    for u in &entry.unresolved {
        info.push(Line::raw(format!(
            "未辨識產品「{}」 {}",
            u.product_raw,
            version_text(&u.version_raw)
        )));
    }
    let info_height = u16::try_from(info.len()).unwrap_or(u16::MAX);
    let component_height =
        COMPONENT_ROWS.min(u16::try_from(entry.components.len()).unwrap_or(u16::MAX)) + 2;
    let [info_area, life_area, comp_area, find_area] = Layout::vertical([
        Constraint::Length(info_height),
        Constraint::Length(lifecycle::detail_height(entry)),
        Constraint::Length(component_height),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(info), info_area);
    lifecycle::detail(frame, life_area, app, entry);
    let comp_rows = component_rows(app, entry);
    let comp_cols = [
        Width::Fixed(ROLE_WIDTH),
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
        &["角色", "元件", "版本", "來源", "更新時間"],
        &comp_cols,
        comp_inner.width,
        comp_rows,
    );
    frame.render_widget(t, comp_inner);
    asset_findings(frame, find_area, app, tag, cursor);
}

/// 元件表格的列：依角色分組排列，第一欄為組名（21 design D6）。
fn component_rows(app: &App, entry: &AssetEntry) -> Vec<(Vec<String>, Style)> {
    group_components(&app.settings.catalog, &entry.components)
        .into_iter()
        .flat_map(|(group, components)| {
            components.into_iter().map(move |c| {
                let values = vec![
                    group.label().to_string(),
                    c.label().to_string(),
                    version_text(&c.version_raw).to_string(),
                    c.source.as_str().to_string(),
                    full_time(&c.updated_at, app.services.tz),
                ];
                (values, Style::new())
            })
        })
        .collect()
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
