//! CVE 詳情與搜尋結果的顯示格式。

use cvewatch_core::ingest::assigner;
use cvewatch_core::ingest::cve5::{CveRecord, CveState};
use cvewatch_core::ingest::cvss::{self, MetricRow};
use cvewatch_core::ingest::relevance::Relevance;
use cvewatch_core::ingest::show::{AffectedRow, CveDetail, SearchHit};
use serde_json::Value;

/// 搜尋結果中描述顯示的字數。
const SEARCH_DESCRIPTION_CHARS: usize = 80;
/// 每筆 affected 最多列出的版本條件數。
const MAX_VERSION_LINES: usize = 8;

const STATUS_LABELS: [(&str, &str); 4] = [
    ("affected", "受影響"),
    ("not_affected", "不受影響"),
    ("fixed", "已修正"),
    ("under_investigation", "調查中（未知）"),
];

fn status_label(status: &str) -> &str {
    STATUS_LABELS
        .iter()
        .find(|(k, _)| *k == status)
        .map_or(status, |(_, v)| v)
}

fn state_label(state: &str) -> String {
    match state {
        "REJECTED" => "已撤銷（REJECTED）".to_string(),
        "PUBLISHED" => "已發布".to_string(),
        other => other.to_string(),
    }
}

/// `versions[]` 一項 → `>= 11.1.0, < 11.1.2-h3：affected`
fn version_line(v: &Value) -> String {
    let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default();
    let mut cond = text("version").to_string();
    if !text("lessThan").is_empty() {
        cond = format!(">= {cond}, < {}", text("lessThan"));
    } else if !text("lessThanOrEqual").is_empty() {
        cond = format!(">= {cond}, <= {}", text("lessThanOrEqual"));
    }
    let status = text("status");
    format!("{cond}：{}", status_label(status))
}

fn print_affected(a: &AffectedRow) {
    let default = a
        .default_status
        .as_deref()
        .map(|d| format!("（預設 {}）", status_label(d)))
        .unwrap_or_default();
    println!(
        "  [{}] {} / {}{default}",
        a.source.to_uppercase(),
        a.vendor,
        a.product
    );
    if let Some(purl) = &a.purl {
        println!("      purl：{purl}");
    }
    let versions: Vec<Value> = serde_json::from_str(&a.versions_json).unwrap_or_default();
    for v in versions.iter().take(MAX_VERSION_LINES) {
        println!("      {}", version_line(v));
    }
    if versions.len() > MAX_VERSION_LINES {
        println!("      …另有 {} 筆", versions.len() - MAX_VERSION_LINES);
    }
}

fn print_metrics(metrics: &[MetricRow]) {
    if metrics.is_empty() {
        return;
    }
    println!("CVSS（全部來源）：");
    for m in metrics {
        println!(
            "  {} {} {:.1} {} {}",
            m.source.label(),
            m.version,
            m.base_score,
            m.severity.as_deref().unwrap_or("-"),
            m.vector.as_deref().unwrap_or("")
        );
    }
}

pub fn print_detail(d: &CveDetail) {
    let rejected = d.state == "REJECTED";
    println!("{}{}", d.id, if rejected { "  [已撤銷]" } else { "" });
    if let Some(t) = &d.title {
        println!("標題：{t}");
    }
    println!("狀態：{}", state_label(&d.state));
    println!(
        "指派單位：{}",
        assigner::display(d.assigner_short.as_deref())
    );
    let reason = Relevance::parse(&d.relevance_reason)
        .map_or_else(|| d.relevance_reason.clone(), |r| r.label().to_string());
    println!("保留原因：{reason}");
    if let (Some(p), u) = (&d.date_published, &d.date_updated) {
        println!("發布：{p}　更新：{}", u.as_deref().unwrap_or("-"));
    }
    if !d.has_record {
        println!("（尚未取得 CVE.org 紀錄，下次 sync 會補齊）");
    }
    if let Some(desc) = &d.description {
        println!("描述：{desc}");
    }
    println!("嚴重度：{}", cvss::display(d.preferred_metric()));
    print_metrics(&d.metrics);
    if !d.affected.is_empty() {
        println!("Affected：");
        d.affected.iter().for_each(print_affected);
    }
    match &d.nvd_status {
        Some(s) => println!("NVD：{s}（CPE 設定 {} 筆）", d.nvd_match_count),
        None => println!("NVD：無資料"),
    }
    if let Some((e, a, t)) = &d.ssvc {
        let v = |x: &Option<String>| x.clone().unwrap_or_else(|| "-".into());
        println!(
            "SSVC：exploitation={} automatable={} technical impact={}",
            v(e),
            v(a),
            v(t)
        );
    }
    print_exploitation(d);
    print_advisories(d);
    if !d.osv.is_empty() {
        println!("OSV 套件命中：");
        d.osv.iter().for_each(|o| println!("  {o}"));
    }
    if !d.references.is_empty() {
        println!("參考連結：");
        d.references.iter().for_each(|r| println!("  {r}"));
    }
}

fn print_exploitation(d: &CveDetail) {
    match &d.kev {
        Some(k) => println!(
            "KEV：已遭利用，加入 {}，到期 {}{}",
            k.date_added,
            k.due_date.as_deref().unwrap_or("-"),
            if k.ransomware {
                "（已知用於勒索軟體）"
            } else {
                ""
            }
        ),
        None => println!("KEV：未列入"),
    }
    match &d.epss {
        Some((score, pct, date)) => {
            println!("EPSS：{score:.5}（百分位 {:.1}%，{date}）", pct * 100.0)
        }
        None => println!("EPSS：無資料"),
    }
}

fn print_advisories(d: &CveDetail) {
    if d.advisories.is_empty() && d.fixes.is_empty() {
        return;
    }
    println!("原廠公告：");
    for a in &d.advisories {
        let why = a
            .justification
            .as_deref()
            .map(|j| format!("（{j}）"))
            .unwrap_or_default();
        println!(
            "  {} {} v{}：{} {} {}{why}",
            a.provider,
            a.tracking_id,
            a.doc_version,
            a.product,
            a.version_expr,
            status_label(&a.status)
        );
    }
    for f in &d.fixes {
        println!(
            "  修正：{} {} {}{}",
            f.product,
            f.fixed_version.as_deref().unwrap_or("（版本見說明）"),
            f.details.as_deref().unwrap_or(""),
            f.url
                .as_deref()
                .map(|u| format!(" {u}"))
                .unwrap_or_default()
        );
    }
}

/// `--fetch` 取得、未存入本機的紀錄。
pub fn print_fetched(r: &CveRecord, references: &[String]) {
    println!("{}（自 CVE.org 取得，未存入本機）", r.id);
    if let Some(t) = &r.title {
        println!("標題：{t}");
    }
    let state = match r.state {
        CveState::Published => "PUBLISHED",
        CveState::Rejected => "REJECTED",
    };
    println!("狀態：{}", state_label(state));
    println!(
        "指派單位：{}",
        assigner::display(r.assigner_short.as_deref())
    );
    if let Some(d) = &r.description_en {
        println!("描述：{d}");
    }
    for a in &r.cna.affected {
        println!(
            "  [CNA] {} / {}",
            a.vendor.as_deref().unwrap_or("?"),
            a.product.as_deref().unwrap_or("?")
        );
    }
    for m in &r.cna.metrics {
        println!("  CNA CVSS {} {:.1}", m.version, m.base_score);
    }
    references.iter().for_each(|u| println!("  {u}"));
}

pub fn print_search_hit(h: &SearchHit) {
    let e = &h.entry;
    let desc: String = e
        .description
        .chars()
        .take(SEARCH_DESCRIPTION_CHARS)
        .collect();
    println!(
        "{:<16} {} {}{}",
        e.id,
        if h.local { "[本機]" } else { "      " },
        e.products.first().map(String::as_str).unwrap_or("-"),
        if e.state == "REJECTED" {
            " [已撤銷]"
        } else {
            ""
        }
    );
    println!("                 {desc}");
}
