//! 命中詳情：CVE 描述、各來源 CVSS／EPSS／KEV、證據與版本區間、來源矛盾、原廠修正版本、時間軸、連結。

use cvewatch_core::matching::evidence::{Source, Verdict};
use cvewatch_core::matching::findings::MANUAL;
use cvewatch_core::matching::query::{ConflictRow, FindingDetail};
use cvewatch_core::matching::triage::{IgnoreReason, Justification};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::full_time;
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

/// 多行文字（例如 CVE 描述）：保留換行分段。
fn paragraphs(s: &str) -> Vec<Line<'static>> {
    s.lines().map(text).collect()
}

fn heading(title: &str) -> Line<'static> {
    Line::styled(
        title.to_string(),
        Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )
}

fn source_label(s: &str) -> String {
    match Source::parse(s) {
        Some(src) => src.label().to_string(),
        None if s == MANUAL => "人工".to_string(),
        None => s.to_string(),
    }
}

fn verdict_label(v: &str) -> &'static str {
    Verdict::parse(v).unwrap_or(Verdict::Unknown).label()
}

pub fn lines(app: &App, data: &DetailData) -> Vec<Line<'static>> {
    let mut out = summary(app, data);
    out.extend(scores(data));
    out.extend(evidence(&data.finding));
    out.extend(conflicts(&data.finding.conflicts));
    out.extend(fixes(data));
    out.extend(timeline(app, &data.finding));
    out.push(text(""));
    out.push(heading("連結（按 o 開啟）"));
    out.extend(
        data.links
            .iter()
            .map(|l| text(format!("  {}  {}", l.label, l.url))),
    );
    out
}

fn summary(app: &App, data: &DetailData) -> Vec<Line<'static>> {
    let d = &data.finding;
    let r = &d.row;
    let decided = if r.manual && r.decided_by != MANUAL {
        format!("{}（人工套用）", source_label(&r.decided_by))
    } else {
        source_label(&r.decided_by)
    };
    let mut out = vec![
        Line::styled(
            format!(
                "{}  {}  {}（{}）  判定來源 {decided}",
                r.cve_id,
                r.priority.as_str(),
                r.status.label(),
                r.confidence.as_str()
            ),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        text(format!(
            "資產 {}（{}，重要性 {}）  元件 {} {}",
            r.asset_tag, r.asset_name, r.criticality, r.component, r.version
        )),
        text(format!(
            "首次發現 {}  最近比對 {}",
            full_time(&d.first_seen, app.services.tz),
            full_time(&d.last_evaluated, app.services.tz)
        )),
    ];
    if let Some(reason) = &d.ignore_reason {
        let reason = IgnoreReason::parse(reason).map_or(reason.clone(), |r| r.label().to_string());
        let why = d.justification.as_deref().map(|j| {
            Justification::parse(j).map_or(format!("，{j}"), |x| format!("，{}（{j}）", x.label()))
        });
        out.push(text(format!(
            "忽略原因 {reason}{}",
            why.unwrap_or_default()
        )));
    }
    if r.still_vulnerable {
        out.push(text(format!(
            "{} 已處理，但最近一次比對仍在受影響範圍（清冊版本未更新？）",
            app.glyphs.warn
        )));
    } else if !d.in_range {
        out.push(text("最近一次比對已不在受影響範圍"));
    }
    if r.component_removed {
        out.push(text("元件已自清冊／套件清單移除，命中保留但不再更新"));
    }
    out.push(text(""));
    let description = data
        .cve
        .as_ref()
        .and_then(|c| c.description.clone().or_else(|| c.title.clone()))
        .or_else(|| r.title.clone())
        .unwrap_or_else(|| "（本機沒有描述）".into());
    out.extend(paragraphs(&description));
    out
}

fn scores(data: &DetailData) -> Vec<Line<'static>> {
    let mut out = vec![text(""), heading("風險資料")];
    let Some(cve) = &data.cve else {
        out.push(text("  本機沒有此 CVE 的紀錄"));
        return out;
    };
    for m in &cve.metrics {
        let provider = m
            .provider
            .as_deref()
            .map(|p| format!(" {p}"))
            .unwrap_or_default();
        out.push(text(format!(
            "  CVSS {} {}{provider}：{:.1}{}",
            m.version,
            m.source.label(),
            m.base_score,
            m.vector
                .as_deref()
                .map(|v| format!("  {v}"))
                .unwrap_or_default()
        )));
    }
    if cve.metrics.is_empty() {
        out.push(text("  CVSS：無"));
    }
    out.push(text(match &cve.epss {
        Some((score, pct, date)) => format!("  EPSS {score:.3}（百分位 {pct:.3}，{date}）"),
        None => "  EPSS：無".into(),
    }));
    out.push(text(match &cve.kev {
        Some(k) => format!(
            "  KEV 列入 {}{}{}",
            k.date_added,
            k.due_date
                .as_deref()
                .map(|d| format!("，期限 {d}"))
                .unwrap_or_default(),
            if k.ransomware {
                "，已知用於勒索軟體"
            } else {
                ""
            }
        ),
        None => "  KEV：未列入".into(),
    }));
    out
}

fn evidence(d: &FindingDetail) -> Vec<Line<'static>> {
    let mut out = vec![
        text(""),
        heading("證據（優先序：原廠公告 > CNA > NVD > ADP）"),
    ];
    if d.evidence.is_empty() {
        out.push(text("  （無；人工建立或來源已不再提到此產品）"));
    }
    for e in &d.evidence {
        out.push(text(format!(
            "  [{}] {}：{}",
            e.source.label(),
            e.verdict.label(),
            e.summary
        )));
        if !e.fixed_versions.is_empty() {
            out.push(text(format!(
                "      修正版本：{}",
                e.fixed_versions.join("、")
            )));
        }
        out.extend(e.notes.iter().map(|n| text(format!("      註記：{n}"))));
    }
    out
}

fn conflicts(rows: &[ConflictRow]) -> Vec<Line<'static>> {
    if rows.is_empty() {
        return Vec::new();
    }
    let mut out = vec![text(""), heading("來源矛盾")];
    out.extend(rows.iter().map(|c| {
        text(format!(
            "  最終 {} 判定{}，但 {} 判定{}",
            source_label(&c.final_source),
            verdict_label(&c.final_verdict),
            source_label(&c.other_source),
            verdict_label(&c.other_verdict)
        ))
    }));
    out
}

fn fixes(data: &DetailData) -> Vec<Line<'static>> {
    let Some(cve) = &data.cve else {
        return Vec::new();
    };
    if cve.fixes.is_empty() {
        return Vec::new();
    }
    let mut out = vec![text(""), heading("原廠修正版本")];
    out.extend(cve.fixes.iter().map(|f| {
        text(format!(
            "  {} {}：{}{}",
            f.provider,
            f.product,
            f.fixed_version.as_deref().unwrap_or("（未提供版本）"),
            f.details
                .as_deref()
                .map(|d| format!("  {d}"))
                .unwrap_or_default()
        ))
    }));
    out
}

fn timeline(app: &App, d: &FindingDetail) -> Vec<Line<'static>> {
    let mut out = vec![text(""), heading("時間軸")];
    out.extend(d.events.iter().map(|e| {
        let actor = cvewatch_core::matching::query::actor_label(&e.actor);
        let from = e.from.map_or("（新）", |s| s.label());
        let note = if e.note.is_empty() {
            String::new()
        } else {
            format!("  {}", e.note)
        };
        text(format!(
            "  {}  {actor}  {from} {} {}{note}",
            full_time(&e.at, app.services.tz),
            app.glyphs.arrow,
            e.to.label()
        ))
    }));
    out
}
