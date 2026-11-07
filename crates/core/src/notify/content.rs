//! 信件內容（純文字 + 簡單表格）：緊急信（一次 sync 一封）與每日摘要。

use std::collections::HashMap;
use std::fmt::Write as _;

use chrono::{DateTime, FixedOffset, Local, Utc};
use rusqlite::Connection;

use super::digest::DigestData;
use super::email::Mail;
use super::urgent::{EventKind, UrgentEvent};
use crate::error::CoreError;
use crate::matching::engine::Confidence;
use crate::matching::evidence::Source;
use crate::matching::query::FindingRow;
use crate::matching::scoring::Priority;

pub const SUBJECT_PREFIX: &str = "[cvewatch]";

/// 顯示時間用的時區（本機，或固定位移供測試）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Local,
    Fixed(FixedOffset),
}

impl Zone {
    /// `YYYY-MM-DD HH:MM`；無法解析時原樣回傳前 16 字元。
    pub fn time(self, raw: &str) -> String {
        match DateTime::parse_from_rfc3339(raw) {
            Ok(t) => self.at(t.with_timezone(&Utc)),
            Err(_) => raw.get(..16).unwrap_or(raw).to_string(),
        }
    }

    pub fn at(self, t: DateTime<Utc>) -> String {
        const FMT: &str = "%Y-%m-%d %H:%M";
        match self {
            Zone::Local => t.with_timezone(&Local).format(FMT).to_string(),
            Zone::Fixed(o) => t.with_timezone(&o).format(FMT).to_string(),
        }
    }

    pub fn date(self, t: DateTime<Utc>) -> String {
        self.at(t).get(..10).unwrap_or_default().to_string()
    }
}

/// 產品 key → 顯示名稱（例如 `paloalto-panos` → `PAN-OS`）。
pub fn product_names(conn: &Connection) -> Result<HashMap<String, String>, CoreError> {
    let mut stmt = conn.prepare("SELECT key, display_name FROM catalog_product")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// 元件顯示：產品名稱（或套件 purl）與版本。
pub fn component_label(row: &FindingRow, names: &HashMap<String, String>) -> String {
    let name = names.get(&row.component).unwrap_or(&row.component);
    format!("{name} {}", row.version)
}

pub fn decided_by_label(decided_by: &str) -> &str {
    match Source::parse(decided_by) {
        Some(s) => s.label(),
        None if decided_by == "manual" => "人工判定",
        None => decided_by,
    }
}

fn scores(row: &FindingRow) -> String {
    let cvss = row.scores.cvss.map_or("-".into(), |c| format!("{c:.1}"));
    let epss = row.scores.epss.map_or("-".into(), |e| format!("{e:.3}"));
    format!("CVSS {cvss}｜EPSS {epss}")
}

fn kev_text(e: &UrgentEvent) -> String {
    match &e.kev {
        Some(k) => match &k.due_date {
            Some(due) => format!("KEV {}（到期 {due}）", k.date_added),
            None => format!("KEV {}", k.date_added),
        },
        None => "不在 KEV".into(),
    }
}

/// 緊急信：合併當次所有事件。
pub fn urgent_mail(
    events: &[UrgentEvent],
    names: &HashMap<String, String>,
    urgent_cvss: f64,
    now: DateTime<Utc>,
    zone: Zone,
) -> Mail {
    let kev = events.iter().filter(|e| e.kev.is_some()).count();
    let subject = match events {
        [one] => {
            let f = &one.finding;
            let kev_tag = if one.kev.is_some() { "[KEV]" } else { "" };
            format!(
                "{SUBJECT_PREFIX}[緊急][{}]{kev_tag} {} {} — {}",
                f.priority,
                f.cve_id,
                names.get(&f.component).unwrap_or(&f.component),
                f.asset_tag
            )
        }
        _ => format!(
            "{SUBJECT_PREFIX}[緊急] {} 項緊急事件（KEV {kev}）",
            events.len()
        ),
    };
    let mut body = format!(
        "cvewatch 偵測到 {} 項緊急事件（{}）。\n",
        events.len(),
        zone.at(now)
    );
    for e in events {
        let f = &e.finding;
        let _ = write!(
            body,
            "\n[{}] {} {}\n  資產：{}（{}）\n  元件：{}\n  {}｜{}\n  判定來源：{}｜信心：{}\n",
            f.priority,
            f.cve_id,
            e.kind.label(),
            f.asset_tag,
            f.asset_name,
            component_label(f, names),
            scores(f),
            kev_text(e),
            decided_by_label(&f.decided_by),
            f.confidence
        );
        if let Some(title) = &f.title {
            let _ = writeln!(body, "  標題：{title}");
        }
        if e.is_probable() {
            body.push_str("  ※ 版本待確認（probable）：資料不足以確定版本是否受影響，請人工確認\n");
        }
        let _ = writeln!(body, "  命中編號：#{}", f.id);
    }
    let _ = write!(
        body,
        "\n---\n緊急條件：未處理，且 CVE 列入 KEV 或 CVSS ≥ {urgent_cvss:.1}。\n\
         以 `cvewatch tui` 或 `cvewatch finding show <命中編號>` 檢視證據與處理。\n"
    );
    Mail { subject, body }
}

fn priority_counts(rows: &[FindingRow]) -> String {
    Priority::ALL
        .iter()
        .filter_map(|p| {
            let n = rows.iter().filter(|r| r.priority == *p).count();
            (n > 0).then(|| format!("{p} {n}"))
        })
        .collect::<Vec<_>>()
        .join("、")
}

fn digest_subject(d: &DigestData, zone: Zone) -> String {
    let date = zone.date(d.until);
    if d.is_empty() {
        return format!("{SUBJECT_PREFIX} 每日摘要 {date}：無新事項");
    }
    let mut parts = Vec::new();
    if !d.new_findings.is_empty() {
        parts.push(format!(
            "新命中 {}（{}）",
            d.new_findings.len(),
            priority_counts(&d.new_findings)
        ));
    }
    if !d.auto_changes.is_empty() {
        parts.push(format!("狀態變化 {}", d.auto_changes.len()));
    }
    let failed = d.failed_sources().count();
    if failed > 0 {
        parts.push(format!("同步失敗 {failed}"));
    }
    if !d.dead.is_empty() {
        parts.push(format!("無法送出 {}", d.dead.len()));
    }
    if parts.is_empty() {
        parts.push("等待區有變化".into());
    }
    format!("{SUBJECT_PREFIX} 每日摘要 {date}：{}", parts.join("、"))
}

/// 每日摘要。
pub fn digest_mail(d: &DigestData, names: &HashMap<String, String>, zone: Zone) -> Mail {
    let mut b = format!(
        "cvewatch 每日摘要\n期間：{} – {}\n",
        zone.at(d.since),
        zone.at(d.until)
    );
    if d.is_empty() {
        b.push_str("\n無新事項。\n");
    }
    sync_section(&mut b, d, zone);
    findings_section(&mut b, d, names);
    changes_section(&mut b, d, zone);
    pending_section(&mut b, d, zone);
    dead_section(&mut b, d, zone);
    Mail {
        subject: digest_subject(d, zone),
        body: b,
    }
}

fn sync_section(b: &mut String, d: &DigestData, zone: Zone) {
    b.push_str("\n■ 同步狀態\n");
    if d.sources.is_empty() {
        b.push_str("  尚未同步\n");
        return;
    }
    for s in &d.sources {
        let status = if s.failed() { "失敗" } else { "成功" };
        let _ = write!(b, "  {:<16} {status}  {}", s.source, zone.time(s.time()));
        match (&s.error, s.failed()) {
            (Some(e), true) => {
                let _ = writeln!(b, "  {e}");
            }
            _ => b.push('\n'),
        }
    }
}

fn findings_section(b: &mut String, d: &DigestData, names: &HashMap<String, String>) {
    let _ = writeln!(b, "\n■ 新命中（{}）", d.new_findings.len());
    if d.new_findings.is_empty() {
        b.push_str("  無\n");
        return;
    }
    b.push_str("  等級 CVE              CVSS  EPSS   KEV         資產         元件@版本              判定來源  信心\n");
    for f in &d.new_findings {
        let cvss = f.scores.cvss.map_or("-".into(), |c| format!("{c:.1}"));
        let epss = f.scores.epss.map_or("-".into(), |e| format!("{e:.3}"));
        let kev = f.scores.kev_date.clone().unwrap_or_else(|| "-".into());
        let confidence = match f.confidence {
            Confidence::Probable => "probable（版本待確認）",
            Confidence::Confirmed => "confirmed",
        };
        let status = if f.status == crate::matching::triage::Status::Open {
            String::new()
        } else {
            format!("  [{}]", f.status.label())
        };
        let _ = writeln!(
            b,
            "  {:<4} {:<16} {:<5} {:<6} {:<11} {:<12} {:<22} {:<8}  {confidence}{status}",
            f.priority.as_str(),
            f.cve_id,
            cvss,
            epss,
            kev,
            f.asset_tag,
            component_label(f, names),
            decided_by_label(&f.decided_by)
        );
    }
}

fn changes_section(b: &mut String, d: &DigestData, zone: Zone) {
    let _ = writeln!(b, "\n■ 狀態自動變化（{}）", d.auto_changes.len());
    if d.auto_changes.is_empty() {
        b.push_str("  無\n");
    }
    for c in &d.auto_changes {
        let _ = writeln!(
            b,
            "  {}  #{} {} {}：{} → {}（{}）",
            zone.time(&c.at),
            c.finding_id,
            c.cve_id,
            c.asset_tag,
            c.from.label(),
            c.to.label(),
            c.note
        );
    }
}

fn pending_section(b: &mut String, d: &DigestData, zone: Zone) {
    b.push_str("\n■ 等待區\n");
    let _ = writeln!(b, "  新進入（{}）", d.pending_entered.len());
    for m in &d.pending_entered {
        let _ = writeln!(b, "    {}  {}（{}）", zone.time(&m.at), m.cve_id, m.reason);
    }
    let _ = writeln!(b, "  移出（{}）", d.pending_exited.len());
    for m in &d.pending_exited {
        let _ = writeln!(b, "    {}  {}（{}）", zone.time(&m.at), m.cve_id, m.reason);
    }
    let _ = writeln!(b, "  逾期（{}）", d.pending_stale.len());
    for p in &d.pending_stale {
        let _ = writeln!(
            b,
            "    {}  {}（進入 {}）",
            p.cve_id,
            p.reason.label(),
            zone.time(&p.entered_at)
        );
    }
}

fn dead_section(b: &mut String, d: &DigestData, zone: Zone) {
    let _ = writeln!(b, "\n■ 無法送出的通知（本期 {}）", d.dead.len());
    for i in &d.dead {
        let _ = writeln!(
            b,
            "  #{} {} {}（{}，失敗 {} 次）：{}",
            i.id,
            i.channel.label(),
            i.kind,
            zone.time(i.dead_at.as_deref().unwrap_or_default()),
            i.attempts,
            i.last_error.as_deref().unwrap_or_default()
        );
    }
    if d.dead_total > 0 {
        let _ = writeln!(
            b,
            "  目前共 {} 筆無法送出，以 `cvewatch notify outbox` 檢視。",
            d.dead_total
        );
    }
}

/// 事件種類的說明（Redmine 註記等共用）。
pub fn event_line(e: &UrgentEvent, names: &HashMap<String, String>) -> String {
    let f = &e.finding;
    let probable = if e.is_probable() {
        "（版本待確認）"
    } else {
        ""
    };
    match e.kind {
        EventKind::NewUrgent => format!("{}：{}{probable}", f.asset_tag, component_label(f, names)),
        EventKind::BecameKev => format!(
            "{}：{}，CVE 已列入 KEV（{}）{probable}",
            f.asset_tag,
            component_label(f, names),
            kev_text(e)
        ),
    }
}
