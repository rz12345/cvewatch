//! 命中、來源矛盾與等待區的文字輸出。

use cvewatch_core::matching::engine::Confidence;
use cvewatch_core::matching::evidence::{Source, Verdict};
use cvewatch_core::matching::findings::MANUAL;
use cvewatch_core::matching::pending_ops::PendingRow;
use cvewatch_core::matching::query::{ConflictRow, FindingDetail, FindingRow};
use cvewatch_core::matching::scoring::ScoreInputs;

const PROBABLE_MARK: &str = "?";
const STILL_VULNERABLE_MARK: &str = "!";
const REMOVED_MARK: &str = "（已移除）";

fn source_label(s: &str) -> String {
    match Source::parse(s) {
        Some(src) => src.label().to_string(),
        None if s == MANUAL => "人工".to_string(),
        None => s.to_string(),
    }
}

/// 判定來源；使用者人工判定套用時另外標示。
fn decided_label(r: &FindingRow) -> String {
    let source = source_label(&r.decided_by);
    if r.manual && r.decided_by != MANUAL {
        format!("{source}（人工套用）")
    } else {
        source
    }
}

fn score_text(s: &ScoreInputs) -> (String, String, String) {
    (
        s.kev_date
            .clone()
            .map_or_else(|| "-".into(), |_| "KEV".into()),
        s.epss.map_or_else(|| "-".into(), |v| format!("{v:.3}")),
        s.cvss.map_or_else(|| "-".into(), |v| format!("{v:.1}")),
    )
}

fn time(s: &str) -> &str {
    s.get(..16).unwrap_or(s)
}

pub fn print_list(rows: &[FindingRow]) {
    if rows.is_empty() {
        println!("沒有符合條件的命中。");
        return;
    }
    println!(
        "{:>5}  {:<4} {:<6} {:<12} {:<20} {:<22} {:<16} {:<4} {:>6} {:>5}  來源",
        "ID", "等級", "狀態", "資產", "元件", "版本", "CVE", "KEV", "EPSS", "CVSS"
    );
    for r in rows {
        let mut level = r.priority.as_str().to_string();
        if r.confidence == Confidence::Probable {
            level.push_str(PROBABLE_MARK);
        }
        if r.still_vulnerable {
            level.push_str(STILL_VULNERABLE_MARK);
        }
        let (kev, epss, cvss) = score_text(&r.scores);
        let component = if r.component_removed {
            format!("{}{REMOVED_MARK}", r.component)
        } else {
            r.component.clone()
        };
        println!(
            "{:>5}  {:<4} {:<6} {:<12} {:<20} {:<22} {:<16} {:<4} {:>6} {:>5}  {}",
            r.id,
            level,
            r.status.label(),
            r.asset_tag,
            component,
            r.version,
            r.cve_id,
            kev,
            epss,
            cvss,
            decided_label(r)
        );
    }
    println!();
    println!("共 {} 筆。`{PROBABLE_MARK}` probable（版本資訊不足）、`{STILL_VULNERABLE_MARK}` 已處理但仍在受影響範圍。", rows.len());
}

pub fn print_detail(d: &FindingDetail) {
    print_summary(d);
    print_evidence(d);
    if !d.conflicts.is_empty() {
        println!();
        println!("來源矛盾");
        print_conflict_lines(&d.conflicts);
    }
    print_timeline(d);
}

fn print_summary(d: &FindingDetail) {
    let r = &d.row;
    println!(
        "命中 #{}  {}  {}（{}，判定來源 {}）",
        r.id,
        r.cve_id,
        r.priority.as_str(),
        r.confidence.as_str(),
        decided_label(r)
    );
    if let Some(title) = &r.title {
        println!("  {title}");
    }
    println!(
        "  資產      {}（{}，重要性 {}）",
        r.asset_tag, r.asset_status, r.criticality
    );
    println!("  元件      {} {}", r.component, r.version);
    println!("  狀態      {}", r.status.label());
    if let Some(reason) = &d.ignore_reason {
        let why = d
            .justification
            .as_deref()
            .map(|j| format!("，{j}"))
            .unwrap_or_default();
        println!("  忽略原因  {reason}{why}");
    }
    let (kev, epss, cvss) = score_text(&r.scores);
    let kev = r
        .scores
        .kev_date
        .as_deref()
        .map_or(kev, |d| format!("KEV {d}"));
    println!("  風險      {kev}、EPSS {epss}、CVSS {cvss}");
    println!(
        "  首次發現  {}；最近比對 {}",
        time(&d.first_seen),
        time(&d.last_evaluated)
    );
    if r.component_removed {
        println!("  元件已自清冊／套件清單移除，命中保留但不再更新（再次出現時沿用此命中）");
    }
    if r.still_vulnerable {
        println!("  ⚠ 已處理，但最近一次比對仍在受影響範圍（清冊版本未更新？）");
    } else if !d.in_range {
        println!("  最近一次比對已不在受影響範圍");
    }
}

fn print_evidence(d: &FindingDetail) {
    println!();
    println!("證據（優先序：原廠公告 > CNA > NVD > ADP）");
    if d.evidence.is_empty() {
        println!("  （無；人工建立或來源已不再提到此產品）");
    }
    for e in &d.evidence {
        println!(
            "  [{}] {}：{}",
            e.source.label(),
            e.verdict.label(),
            e.summary
        );
        if !e.fixed_versions.is_empty() {
            println!("        修正版本：{}", e.fixed_versions.join("、"));
        }
        for note in &e.notes {
            println!("        註記：{note}");
        }
    }
}

fn print_timeline(d: &FindingDetail) {
    println!();
    println!("時間軸");
    for e in &d.events {
        let actor = if e.actor == "user" {
            "使用者"
        } else {
            "系統"
        };
        let from = e.from.map_or("（新）", |s| s.label());
        let note = if e.note.is_empty() {
            String::new()
        } else {
            format!("  {}", e.note)
        };
        println!(
            "  {}  {actor}  {from} → {}{note}",
            time(&e.at),
            e.to.label()
        );
    }
}

fn print_conflict_lines(rows: &[ConflictRow]) {
    for c in rows {
        println!(
            "  {}  {} {} {}：最終 {} 判定{}，但 {} 判定{}",
            c.cve_id,
            c.asset_tag,
            c.component,
            c.version,
            source_label(&c.final_source),
            verdict_label(&c.final_verdict),
            source_label(&c.other_source),
            verdict_label(&c.other_verdict)
        );
    }
}

fn verdict_label(v: &str) -> &'static str {
    Verdict::parse(v).unwrap_or(Verdict::Unknown).label()
}

pub fn print_conflicts(rows: &[ConflictRow]) {
    if rows.is_empty() {
        println!("沒有來源矛盾。");
        return;
    }
    println!(
        "來源矛盾 {} 筆（最終判定不受影響：不建立命中，既有命中依規則轉為已處理；請人工確認）：",
        rows.len()
    );
    print_conflict_lines(rows);
}

pub fn print_pending(rows: &[PendingRow], only_stale: bool) {
    if rows.is_empty() {
        println!(
            "{}",
            if only_stale {
                "沒有逾期的等待區項目。"
            } else {
                "等待區是空的。"
            }
        );
        return;
    }
    println!(
        "{:<16} {:<10} {:<10} {:<17}  {:<4}  說明",
        "CVE", "KEV", "原因", "進入時間", "標記"
    );
    for r in rows {
        let kev = r
            .kev_date
            .as_deref()
            .map_or("-".to_string(), |d| d.to_string());
        let mark = if r.stale { "逾期" } else { "" };
        println!(
            "{:<16} {:<10} {:<10} {:<17}  {:<4}  {}",
            r.cve_id,
            kev,
            r.reason.label(),
            time(&r.entered_at),
            mark,
            r.title.as_deref().unwrap_or("")
        );
    }
    println!();
    println!(
        "共 {} 筆。人工處理：`cvewatch pending resolve <CVE> --apply <資產編號>` 或 `--dismiss`。",
        rows.len()
    );
}
