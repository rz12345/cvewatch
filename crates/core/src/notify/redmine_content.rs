//! Redmine 議題內容：主旨、描述（純文字）與註記。

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use rusqlite::{params, Connection};

use super::content::{component_label, decided_by_label, event_line};
use super::urgent::UrgentEvent;
use crate::error::CoreError;
use crate::ingest::show::{self, CveDetail};
use crate::matching::query::FindingRow;
use crate::matching::scoring::Priority;
use crate::matching::triage::Change;

/// 建立議題的內容（專案、tracker、指派等在送出時依當下設定填入）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IssueDraft {
    pub cve_id: String,
    pub subject: String,
    pub description: String,
    /// 受影響命中中最高的等級（對應 `priority_map`）
    pub priority: String,
}

impl IssueDraft {
    pub fn priority(&self) -> Option<Priority> {
        Priority::parse(&self.priority)
    }
}

/// 對既有議題加註記。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NoteDraft {
    pub cve_id: String,
    pub notes: String,
}

/// 依 CVE 分組（保留風險排序）。
pub fn by_cve(events: &[UrgentEvent]) -> BTreeMap<String, Vec<&UrgentEvent>> {
    let mut out: BTreeMap<String, Vec<&UrgentEvent>> = BTreeMap::new();
    for e in events {
        out.entry(e.finding.cve_id.clone()).or_default().push(e);
    }
    out
}

/// 主旨：`[cvewatch][P1][KEV] CVE-2026-XXXX PAN-OS — FW-01`（多資產時 `— 3 項資產`）。
pub fn subject(rows: &[FindingRow], kev: bool, names: &HashMap<String, String>) -> String {
    let top = rows
        .iter()
        .map(|r| r.priority)
        .min()
        .unwrap_or(Priority::P4);
    let cve = rows.first().map_or("", |r| r.cve_id.as_str());
    let mut products: Vec<&str> = Vec::new();
    for r in rows {
        let name = names.get(&r.component).unwrap_or(&r.component).as_str();
        if !products.contains(&name) {
            products.push(name);
        }
    }
    let product = if products.len() == 1 {
        products[0].to_string()
    } else {
        products.join("/")
    };
    let mut assets: Vec<&str> = rows.iter().map(|r| r.asset_tag.as_str()).collect();
    assets.sort_unstable();
    assets.dedup();
    let target = match assets.as_slice() {
        [one] => (*one).to_string(),
        many => format!("{} 項資產", many.len()),
    };
    let kev_tag = if kev { "[KEV]" } else { "" };
    format!("[cvewatch][{top}]{kev_tag} {cve} {product} — {target}")
}

/// 描述：CVE 摘要、CVSS/EPSS/KEV（到期日）、受影響資產表、判定依據、原廠修正版本、參考連結。
pub fn description(
    rows: &[FindingRow],
    cve: Option<&CveDetail>,
    names: &HashMap<String, String>,
) -> String {
    let mut d = String::new();
    let first = rows.first();
    if let Some(title) = cve
        .and_then(|c| c.title.clone())
        .or_else(|| first.and_then(|r| r.title.clone()))
    {
        let _ = writeln!(d, "{title}\n");
    }
    if let Some(desc) = cve.and_then(|c| c.description.clone()) {
        let _ = writeln!(d, "{desc}\n");
    }
    if let Some(r) = first {
        let cvss = r.scores.cvss.map_or("-".into(), |c| format!("{c:.1}"));
        let epss = r.scores.epss.map_or("-".into(), |e| format!("{e:.3}"));
        let _ = writeln!(d, "CVSS：{cvss}　EPSS：{epss}");
    }
    match cve.and_then(|c| c.kev.as_ref()) {
        Some(k) => {
            let due = k.due_date.as_deref().unwrap_or("-");
            let _ = writeln!(d, "KEV：已列入（{}，修補期限 {due}）", k.date_added);
        }
        None => d.push_str("KEV：未列入\n"),
    }
    d.push_str("\n受影響資產：\n");
    d.push_str("| 等級 | 資產 | 元件@版本 | 判定依據 | 信心 |\n");
    for r in rows {
        let _ = writeln!(
            d,
            "| {} | {} | {} | {} | {} |",
            r.priority,
            r.asset_tag,
            component_label(r, names),
            decided_by_label(&r.decided_by),
            if r.confidence.as_str() == "probable" {
                "probable（版本待確認）"
            } else {
                "confirmed"
            }
        );
    }
    let fixes = cve.map(|c| c.fixes.as_slice()).unwrap_or_default();
    if !fixes.is_empty() {
        d.push_str("\n原廠修正版本：\n");
        for f in fixes {
            let version = f.fixed_version.as_deref().unwrap_or("（見公告）");
            let _ = writeln!(d, "- {} {}：{version}", f.provider, f.product);
        }
    }
    let refs: Vec<String> = cve.map(|c| c.references.clone()).unwrap_or_default();
    let cve_id = first.map_or("", |r| r.cve_id.as_str());
    d.push_str("\n參考連結：\n");
    let _ = writeln!(d, "- https://www.cve.org/CVERecord?id={cve_id}");
    for r in refs.iter().take(MAX_REFERENCES) {
        let _ = writeln!(d, "- {r}");
    }
    d.push_str(
        "\n（由 cvewatch 自動建立；cvewatch 中的處理狀態變更會以註記補充，不會變更議題狀態。）\n",
    );
    d
}

/// 描述中列出的參考連結上限。
pub const MAX_REFERENCES: usize = 10;

/// 建立議題的草稿。`affected` 為此 CVE 的所有未處理命中（使用中資產、未移除元件；緊急與否
/// 由 CVE 決定，因此即為預先載入的緊急命中中屬於此 CVE 者），議題列出全部受影響資產。
pub fn issue_draft(
    conn: &Connection,
    cve_id: &str,
    events: &[&UrgentEvent],
    affected: Vec<FindingRow>,
    names: &HashMap<String, String>,
) -> Result<IssueDraft, CoreError> {
    let mut rows = affected;
    if rows.is_empty() {
        rows = events.iter().map(|e| e.finding.clone()).collect();
    }
    let cve = show::load(conn, cve_id)?;
    let kev = events.iter().any(|e| e.kev.is_some());
    let top = rows
        .iter()
        .map(|r| r.priority)
        .min()
        .unwrap_or(Priority::P4);
    Ok(IssueDraft {
        cve_id: cve_id.to_string(),
        subject: subject(&rows, kev, names),
        description: description(&rows, cve.as_ref(), names),
        priority: top.as_str().to_string(),
    })
}

/// 已有議題（或建立中）的 CVE 又有新事件：一筆註記列出新資產或 KEV 變化。
pub fn event_note(
    cve_id: &str,
    events: &[&UrgentEvent],
    names: &HashMap<String, String>,
) -> NoteDraft {
    let mut notes = String::from("cvewatch：此 CVE 有新的緊急事件\n");
    for e in events {
        let _ = writeln!(notes, "- {}", event_line(e, names));
    }
    NoteDraft {
        cve_id: cve_id.to_string(),
        notes,
    }
}

/// 狀態變更註記的一行：`FW-01：resolved（已升至 11.1.6）`。
pub fn triage_line(asset_tag: &str, change: &Change) -> String {
    let mut details: Vec<String> = Vec::new();
    if let Some(r) = change.reason {
        details.push(match change.justification {
            Some(j) => format!("{}／{}", r.as_str(), j.as_str()),
            None => r.as_str().to_string(),
        });
    }
    if !change.note.trim().is_empty() {
        details.push(change.note.trim().to_string());
    }
    if details.is_empty() {
        format!("{asset_tag}：{}", change.status.as_str())
    } else {
        format!(
            "{asset_tag}：{}（{}）",
            change.status.as_str(),
            details.join("；")
        )
    }
}

/// 已有議題、或建立議題的請求仍在佇列中（非 dead）的 CVE（一次查詢，供逐一比對）。
pub fn known_issue_cves(conn: &Connection) -> Result<std::collections::HashSet<String>, CoreError> {
    let mut known: std::collections::HashSet<String> = conn
        .prepare("SELECT cve_id FROM redmine_issue")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    known.extend(pending_create_cves(conn)?);
    Ok(known)
}

/// 刪除 CVE 的議題對應（議題已在 Redmine 刪除時；之後的緊急事件會重新開案）。
pub fn forget_issue(conn: &Connection, cve_id: &str) -> Result<(), CoreError> {
    conn.execute(
        "DELETE FROM redmine_issue WHERE cve_id = ?1",
        params![cve_id],
    )?;
    Ok(())
}

pub fn issue_id(conn: &Connection, cve_id: &str) -> Result<Option<u64>, CoreError> {
    use rusqlite::OptionalExtension;
    let id: Option<i64> = conn
        .query_row(
            "SELECT issue_id FROM redmine_issue WHERE cve_id = ?1",
            params![cve_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(id.and_then(|i| u64::try_from(i).ok()))
}

/// 建立議題的請求仍在佇列中（非 dead）的 CVE。
fn pending_create_cves(conn: &Connection) -> Result<Vec<String>, CoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT json_extract(payload_json, '$.cve_id') FROM outbox
         WHERE channel = 'redmine' AND kind = 'create_issue' AND dead_at IS NULL",
    )?;
    let ids = stmt
        .query_map([], |r| r.get::<_, Option<String>>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids.into_iter().flatten().collect())
}

/// 建立議題的請求仍在佇列中（非 dead）。
pub fn pending_create(conn: &Connection, cve_id: &str) -> Result<bool, CoreError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM outbox
                        WHERE channel = 'redmine' AND kind = 'create_issue' AND dead_at IS NULL
                          AND json_extract(payload_json, '$.cve_id') = ?1)",
        params![cve_id],
        |r| r.get(0),
    )?)
}

pub fn record_issue(
    conn: &Connection,
    cve_id: &str,
    issue_id: u64,
    now: &str,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO redmine_issue (cve_id, issue_id, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (cve_id) DO UPDATE SET issue_id = excluded.issue_id",
        params![cve_id, i64::try_from(issue_id).unwrap_or(i64::MAX), now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::engine::Confidence;
    use crate::matching::scoring::ScoreInputs;
    use crate::matching::triage::Status;

    fn row(asset: &str, component: &str) -> FindingRow {
        FindingRow {
            id: 1,
            asset_tag: asset.into(),
            asset_status: "active".into(),
            component: component.into(),
            version: "1".into(),
            cve_id: "CVE-2026-0001".into(),
            title: None,
            priority: Priority::P1,
            confidence: Confidence::Confirmed,
            decided_by: "cna".into(),
            status: Status::Open,
            still_vulnerable: false,
            scores: ScoreInputs::default(),
            criticality: 2,
            component_removed: false,
            manual: false,
            asset_name: asset.into(),
            description: None,
            first_seen: "2026-11-01T00:00:00+00:00".into(),
        }
    }

    #[test]
    fn subject_lists_each_product_once() {
        let names: HashMap<String, String> = [("paloalto-panos".to_string(), "PAN-OS".to_string())]
            .into_iter()
            .collect();
        let rows = [
            row("FW-01", "paloalto-panos"),
            row("PC-1", "pkg:deb/debian/openssl"),
            row("FW-02", "paloalto-panos"),
        ];
        assert_eq!(
            subject(&rows, true, &names),
            "[cvewatch][P1][KEV] CVE-2026-0001 PAN-OS/pkg:deb/debian/openssl — 3 項資產"
        );
    }

    #[test]
    fn triage_line_formats() {
        use crate::matching::triage::{Change, IgnoreReason, Justification};
        let resolved = Change {
            status: Status::Resolved,
            reason: None,
            justification: None,
            note: "已升至 11.1.6".into(),
        };
        assert_eq!(
            triage_line("FW-01", &resolved),
            "FW-01：resolved（已升至 11.1.6）"
        );
        let ignored = Change {
            status: Status::Ignored,
            reason: Some(IgnoreReason::NotAffected),
            justification: Some(Justification::RequiresConfiguration),
            note: String::new(),
        };
        assert_eq!(
            triage_line("FW-01", &ignored),
            "FW-01：ignored（not_affected／requires_configuration）"
        );
    }
}
