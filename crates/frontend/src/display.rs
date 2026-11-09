//! 顯示用文字（TUI 與 GUI 共用）：時間格式、等待區天數、命中詳情的段落，以及資產畫面的
//! 使用者用語（產品顯示名稱、子類別「代碼 名稱」、C/I/A 與曝露度的中文名稱，design 11 D1、13 D8）。
//!
//! 只產生純文字；粗體、標題樣式與控制字元處理由各前端決定。

use chrono::{DateTime, Utc};
use cvewatch_core::assets::plan::version_or;
use cvewatch_core::assets::{
    AssetClasses, AssetRecord, Cia, ClassCode, Component, Exposure, PendingCounts,
};
use cvewatch_core::catalog::Catalog;
use cvewatch_core::matching::evidence::{Source, Verdict};
use cvewatch_core::matching::findings::MANUAL;
use cvewatch_core::matching::pending_ops::PendingRow;
use cvewatch_core::matching::query::{actor_label, ConflictRow, FindingDetail};
use cvewatch_core::matching::triage::{IgnoreReason, Justification};

use crate::data::DetailData;
use crate::services::DisplayTz;

/// `YYYY-MM-DD HH:MM`（顯示時區）；無法解析時取前 16 字。
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

/// 單行顯示：外部資料中的控制字元（換行、tab 等）換成空白，避免表格或標籤破版。
pub fn one_line(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// 進入等待區的天數（`now` 通常為資料載入時間）；時間無法解析時為 None。
pub fn pending_days(row: &PendingRow, now: DateTime<Utc>) -> Option<i64> {
    DateTime::parse_from_rfc3339(&row.entered_at)
        .ok()
        .map(|t| (now - t.to_utc()).num_days().max(0))
}

/// 目錄產品的顯示名稱（例如 `proxmox-ve` → 「Proxmox VE」）；目錄沒有此 key 時原樣回傳。
pub fn product_name(catalog: &Catalog, key: &str) -> String {
    catalog
        .get(key)
        .map_or_else(|| key.to_string(), |p| p.display_name.clone())
}

/// 元件名稱（不含版本）：目錄產品用顯示名稱，套件用不含版本的 purl（不在目錄中，原樣回傳）。
pub fn component_name(catalog: &Catalog, component: &Component) -> String {
    product_name(catalog, component.key())
}

/// 版本待補的元件版本（18 design D8；資產清單標示、詳情與編輯對話框共用）。
pub const PENDING_VERSION: &str = "版本待補";
/// 版本待補元件在詳情中的提示。
pub const PENDING_NOTE: &str = "（未納入比對）";

/// 元件或未辨識產品的版本：待補時為 [`PENDING_VERSION`]。
pub fn version_text(version_raw: &str) -> &str {
    version_or(version_raw, PENDING_VERSION)
}

/// 詳情中的元件：「Synology DSM 版本待補（未納入比對）」「PAN-OS 11.1.6」。
pub fn component_line(catalog: &Catalog, component: &Component) -> String {
    let name = component_name(catalog, component);
    if component.is_pending() {
        format!("{name} {PENDING_VERSION}{PENDING_NOTE}")
    } else {
        format!("{name} {}", component.version_raw)
    }
}

/// Dashboard 的版本待補數字：「版本待補 3 個元件（2 個資產），未納入比對」；沒有時為「版本待補 0」。
pub fn pending_versions_text(counts: &PendingCounts) -> String {
    counts
        .summary()
        .unwrap_or_else(|| format!("{PENDING_VERSION} 0"))
}

/// 要顯示的資產名稱；與資產編號相同時為 None（清單與詳情標題不重複顯示）。
pub fn shown_name(record: &AssetRecord) -> Option<&str> {
    let name = record.name.trim();
    (name != record.asset_tag.trim()).then_some(name)
}

/// 曝露度的中文名稱。
fn exposure_zh(e: Exposure) -> &'static str {
    match e {
        Exposure::Internet => "網際網路",
        Exposure::Dmz => "DMZ",
        Exposure::Internal => "內部網路",
        Exposure::Mgmt => "管理網路",
    }
}

/// 子類別「代碼 名稱」，例如「HW01 伺服器」；設定檔已不再定義的代碼為「HW12（未定義）」。
/// 儲存與匯出仍使用代碼。
pub fn class_label(code: &ClassCode, classes: &AssetClasses) -> String {
    classes.label(code)
}

/// 「C2 / I3 / A4（價值 4）」。
pub fn rating_summary(cia: &Cia) -> String {
    format!(
        "C{} / I{} / A{}（價值 {}）",
        cia.confidentiality,
        cia.integrity,
        cia.availability,
        cia.value()
    )
}

/// 「英文值（中文）」，例如「internal（內部網路）」。
pub fn exposure_label(e: Exposure) -> String {
    format!("{}（{}）", e.as_str(), exposure_zh(e))
}

/// 詳情中的符號（TUI 在舊主控台改用 ASCII）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marks {
    /// 已處理但仍在受影響範圍
    pub warn: &'static str,
    /// 時間軸的狀態轉換
    pub arrow: &'static str,
}

/// 詳情的一段：標題（摘要段沒有標題）與各行。摘要段的第一行是重點（CVE、等級、狀態、判定來源）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: Option<&'static str>,
    pub lines: Vec<String>,
}

fn section(title: &'static str, lines: Vec<String>) -> Section {
    Section {
        title: Some(title),
        lines,
    }
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

/// 摘要、風險資料、證據、來源矛盾（有時）、原廠修正版本（有時）、時間軸。連結另外處理。
pub fn sections(data: &DetailData, tz: DisplayTz, marks: Marks) -> Vec<Section> {
    let mut out = vec![
        summary(data, tz, marks),
        scores(data),
        evidence(&data.finding),
    ];
    if !data.finding.conflicts.is_empty() {
        out.push(conflicts(&data.finding.conflicts));
    }
    out.extend(fixes(data));
    out.push(timeline(&data.finding, tz, marks));
    out
}

fn summary(data: &DetailData, tz: DisplayTz, marks: Marks) -> Section {
    let d = &data.finding;
    let r = &d.row;
    let decided = if r.manual && r.decided_by != MANUAL {
        format!("{}（人工套用）", source_label(&r.decided_by))
    } else {
        source_label(&r.decided_by)
    };
    let mut lines = vec![
        format!(
            "{}  {}  {}（{}）  判定來源 {decided}",
            r.cve_id,
            r.priority.as_str(),
            r.status.label(),
            r.confidence.as_str()
        ),
        format!(
            "資產 {}（{}，資產價值 {}）  元件 {} {}",
            r.asset_tag, r.asset_name, r.asset_value, r.component, r.version
        ),
        format!(
            "首次發現 {}  最近比對 {}",
            full_time(&d.first_seen, tz),
            full_time(&d.last_evaluated, tz)
        ),
    ];
    if let Some(reason) = &d.ignore_reason {
        let reason = IgnoreReason::parse(reason).map_or(reason.clone(), |r| r.label().to_string());
        let why = d.justification.as_deref().map(|j| {
            Justification::parse(j).map_or(format!("，{j}"), |x| format!("，{}（{j}）", x.label()))
        });
        lines.push(format!("忽略原因 {reason}{}", why.unwrap_or_default()));
    }
    if r.still_vulnerable {
        lines.push(format!(
            "{} 已處理，但最近一次比對仍在受影響範圍（清冊版本未更新？）",
            marks.warn
        ));
    } else if !d.in_range {
        lines.push("最近一次比對已不在受影響範圍".into());
    }
    if r.component_removed {
        lines.push("元件已自清冊／套件清單移除，命中保留但不再更新".into());
    }
    lines.push(String::new());
    let description = data
        .cve
        .as_ref()
        .and_then(|c| c.description.clone().or_else(|| c.title.clone()))
        .or_else(|| r.title.clone())
        .unwrap_or_else(|| "（本機沒有描述）".into());
    lines.extend(description.lines().map(str::to_string));
    Section { title: None, lines }
}

fn scores(data: &DetailData) -> Section {
    let Some(cve) = &data.cve else {
        return section("風險資料", vec!["  本機沒有此 CVE 的紀錄".into()]);
    };
    let mut lines: Vec<String> = cve
        .metrics
        .iter()
        .map(|m| {
            let provider = m
                .provider
                .as_deref()
                .map(|p| format!(" {p}"))
                .unwrap_or_default();
            let vector = m
                .vector
                .as_deref()
                .map(|v| format!("  {v}"))
                .unwrap_or_default();
            format!(
                "  CVSS {} {}{provider}：{:.1}{vector}",
                m.version,
                m.source.label(),
                m.base_score
            )
        })
        .collect();
    if cve.metrics.is_empty() {
        lines.push("  CVSS：無".into());
    }
    lines.push(match &cve.epss {
        Some((score, pct, date)) => format!("  EPSS {score:.3}（百分位 {pct:.3}，{date}）"),
        None => "  EPSS：無".into(),
    });
    lines.push(match &cve.kev {
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
    });
    section("風險資料", lines)
}

fn evidence(d: &FindingDetail) -> Section {
    let mut lines = Vec::new();
    if d.evidence.is_empty() {
        lines.push("  （無；人工建立或來源已不再提到此產品）".into());
    }
    for e in &d.evidence {
        lines.push(format!(
            "  [{}] {}：{}",
            e.source.label(),
            e.verdict.label(),
            e.summary
        ));
        if !e.fixed_versions.is_empty() {
            lines.push(format!("      修正版本：{}", e.fixed_versions.join("、")));
        }
        lines.extend(e.notes.iter().map(|n| format!("      註記：{n}")));
    }
    section("證據（優先序：原廠公告 > CNA > NVD > ADP）", lines)
}

fn conflicts(rows: &[ConflictRow]) -> Section {
    let lines = rows
        .iter()
        .map(|c| {
            format!(
                "  最終 {} 判定{}，但 {} 判定{}",
                source_label(&c.final_source),
                verdict_label(&c.final_verdict),
                source_label(&c.other_source),
                verdict_label(&c.other_verdict)
            )
        })
        .collect();
    section("來源矛盾", lines)
}

fn fixes(data: &DetailData) -> Option<Section> {
    let cve = data.cve.as_ref().filter(|c| !c.fixes.is_empty())?;
    let lines = cve
        .fixes
        .iter()
        .map(|f| {
            format!(
                "  {} {}：{}{}",
                f.provider,
                f.product,
                f.fixed_version.as_deref().unwrap_or("（未提供版本）"),
                f.details
                    .as_deref()
                    .map(|d| format!("  {d}"))
                    .unwrap_or_default()
            )
        })
        .collect();
    Some(section("原廠修正版本", lines))
}

fn timeline(d: &FindingDetail, tz: DisplayTz, marks: Marks) -> Section {
    let lines = d
        .events
        .iter()
        .map(|e| {
            let from = e.from.map_or("（新）", |s| s.label());
            let note = if e.note.is_empty() {
                String::new()
            } else {
                format!("  {}", e.note)
            };
            format!(
                "  {}  {}  {from} {} {}{note}",
                full_time(&e.at, tz),
                actor_label(&e.actor),
                marks.arrow,
                e.to.label()
            )
        })
        .collect();
    section("時間軸", lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
    use cvewatch_core::matching::pending::PendingReason;

    #[test]
    fn times_in_display_zone() {
        let tz = DisplayTz::Fixed(FixedOffset::east_opt(8 * 3600).unwrap());
        assert_eq!(
            full_time("2026-11-01T06:05:00+00:00", tz),
            "2026-11-01 14:05"
        );
        assert_eq!(short_time("2026-11-01T06:05:00+00:00", tz), "11-01 14:05");
        assert_eq!(full_time("not a time", tz), "not a time");
    }

    #[test]
    fn product_names_come_from_the_catalog() {
        let catalog = Catalog::seed().unwrap();
        assert_eq!(product_name(&catalog, "proxmox-ve"), "Proxmox VE");
        assert_eq!(product_name(&catalog, "no-such-key"), "no-such-key");
    }

    #[test]
    fn name_equal_to_tag_is_not_repeated() {
        let mut r = AssetRecord {
            asset_tag: "PVE-01".into(),
            name: "PVE-01 ".into(),
            category: ClassCode::parse("HW01").unwrap(),
            location: None,
            exposure: Exposure::Internal,
            cia: Cia::new(2, 3, 4),
            isms_id: None,
            system_code: None,
            responsible_unit: None,
            custodian: None,
            user_unit: None,
            compliance: None,
            personal_data: false,
            notes: None,
        };
        assert_eq!(shown_name(&r), None);
        r.name = "pve01".into();
        assert_eq!(shown_name(&r), Some("pve01"));
    }

    #[test]
    fn component_names_have_no_version() {
        use cvewatch_core::assets::ComponentKind;
        let catalog = Catalog::seed().unwrap();
        let mut c = Component {
            id: 1,
            asset_id: 1,
            kind: ComponentKind::Package {
                purl: "pkg:deb/debian/openssl".into(),
                purl_full: "pkg:deb/debian/openssl@3.0.11-1".into(),
            },
            version_raw: "3.0.11-1".into(),
            version_norm: None,
            source: cvewatch_core::assets::ComponentSource::Dpkg,
            updated_at: String::new(),
        };
        assert_eq!(component_name(&catalog, &c), "pkg:deb/debian/openssl");
        c.kind = ComponentKind::Product {
            product_id: 1,
            product_key: "paloalto-panos".into(),
        };
        assert_eq!(component_name(&catalog, &c), "PAN-OS");
    }

    #[test]
    fn class_and_exposure_labels() {
        let classes = AssetClasses::builtin();
        let code = |s: &str| ClassCode::parse(s).unwrap();
        assert_eq!(class_label(&code("HW01"), &classes), "HW01 伺服器");
        assert_eq!(class_label(&code("HW12"), &classes), "HW12（未定義）");
        for c in classes.all() {
            assert!(class_label(&c.code, &classes).starts_with(c.code.as_str()));
        }
        assert_eq!(rating_summary(&Cia::new(2, 3, 4)), "C2 / I3 / A4（價值 4）");
        assert_eq!(exposure_label(Exposure::Internal), "internal（內部網路）");
        for e in Exposure::ALL {
            assert!(exposure_label(*e).starts_with(e.as_str()));
        }
    }

    #[test]
    fn control_characters_become_spaces() {
        assert_eq!(one_line("a\nb\tc\r"), "a b c ");
    }

    #[test]
    fn pending_days_since_entry() {
        let row = PendingRow {
            cve_id: "CVE-2099-0001".into(),
            reason: PendingReason::KeywordOnly,
            entered_at: "2026-11-01T00:00:00+00:00".into(),
            stale: false,
            kev_date: None,
            title: None,
        };
        let now = DateTime::parse_from_rfc3339("2026-11-07T12:00:00+00:00")
            .unwrap()
            .to_utc();
        assert_eq!(pending_days(&row, now), Some(6));
        let bad = PendingRow {
            entered_at: "x".into(),
            ..row
        };
        assert_eq!(pending_days(&bad, now), None);
    }
}
