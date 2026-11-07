//! CycloneDX VEX 匯入（`cvewatch vex import`）：讀檔（`.age` 需口令）、對應元件與命中、
//! 衝突判斷、套用或 `--dry-run` 報告。
//!
//! 套用時與使用者手動變更相同（[`triage::apply_decision`]），操作者記為 import、備註為
//! 「<檔名>：<detail>」；之後仍受 04 自動規則約束（例如列入 KEV 時重新開啟）。

pub mod doc;
pub mod plan;
pub mod resolve;

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, TransactionBehavior};
use zeroize::Zeroizing;

pub use plan::{Item, Plan, Planned};
pub use resolve::Method;

use super::detail::{format_time, import_note};
use super::file;
use crate::error::CoreError;
use crate::matching::triage::{self, Change, Decider};

/// 匯入選項。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportOptions {
    pub dry_run: bool,
    /// 本機較新時仍覆蓋
    pub force: bool,
}

/// 匯入結果（dry-run 時 `applied` 為 false，資料庫不變）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub file_name: String,
    pub plan: Plan,
    pub applied: bool,
}

/// 套用後、提交前以狀態／原因／理由確實改變的命中與其變更呼叫一次（例如排入 Redmine 註記，
/// 一個 CVE 一則）；只更新 detail 的不含在內。失敗時整批回復。
pub type AfterImport<'a> = dyn Fn(&Connection, &[(i64, Change)]) -> Result<(), CoreError> + 'a;

/// 讀取 VEX 檔；age 加密檔向 `passphrase` 取得口令後解密。
pub fn read_file(
    path: &Path,
    passphrase: impl FnOnce() -> Result<Zeroizing<String>, CoreError>,
) -> Result<doc::Bom, CoreError> {
    let size = std::fs::metadata(path)
        .map_err(|e| CoreError::Invalid(format!("無法讀取 {}：{e}", path.display())))?
        .len();
    if size > doc::MAX_VEX_BYTES as u64 {
        return Err(CoreError::Invalid(format!(
            "{} 超過 {} MB，不是可讀取的 VEX 檔",
            path.display(),
            doc::MAX_VEX_BYTES / 1024 / 1024
        )));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| CoreError::Invalid(format!("無法讀取 {}：{e}", path.display())))?;
    let plain = if file::is_encrypted(&bytes) {
        let phrase = passphrase()?;
        Zeroizing::new(file::decrypt(&bytes, &phrase)?)
    } else {
        Zeroizing::new(bytes)
    };
    doc::parse(&plain)
}

/// 檔名（記錄於事件備註）。
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// affects 參照 → 要對應的元件描述：文件中的元件；bom-link（`urn:cdx:…#<bom-ref>`）取其片段；
/// 都沒有時把參照本身當成 bom-ref，並在形如 purl／CPE 時一併作為識別。
fn descriptor(reference: &str, components: &HashMap<String, doc::Component>) -> doc::Component {
    if let Some(c) = components.get(reference) {
        return c.clone();
    }
    let fragment = reference
        .strip_prefix("urn:cdx:")
        .and_then(|rest| rest.split_once('#'))
        .map(|(_, f)| f);
    if let Some(c) = fragment.and_then(|f| components.get(f)) {
        return c.clone();
    }
    let r = fragment.unwrap_or(reference).trim().to_string();
    doc::Component {
        purl: r.starts_with("pkg:").then(|| r.clone()),
        cpe: r.starts_with("cpe:").then(|| r.clone()),
        bom_ref: Some(r),
        ..doc::Component::default()
    }
}

/// 在單一交易中規劃並（非 dry-run 時）套用。dry-run 只讀取，不取得寫入鎖，避免擋住排程 sync。
pub fn import(
    conn: &mut Connection,
    bom: &doc::Bom,
    file_name: &str,
    options: ImportOptions,
    now: DateTime<Utc>,
    after: &AfterImport<'_>,
) -> Result<ImportReport, CoreError> {
    let behavior = if options.dry_run {
        TransactionBehavior::Deferred
    } else {
        TransactionBehavior::Immediate
    };
    let tx = conn.transaction_with_behavior(behavior)?;
    let plan = plan::plan(&tx, bom, options.force, now)?;
    if !options.dry_run {
        let changed = apply_plan(&tx, &plan, file_name, now)?;
        if !changed.is_empty() {
            after(&tx, &changed)?;
        }
        tx.commit()?;
        tracing::info!(
            file = file_name,
            applied = plan.apply.len(),
            unmatched = plan.unmatched.len(),
            local_newer = plan.local_newer.len(),
            incomplete = plan.incomplete.len(),
            "已匯入 VEX"
        );
    }
    Ok(ImportReport {
        file_name: file_name.to_string(),
        plan,
        applied: !options.dry_run,
    })
}

/// 套用計畫中的每一項（操作者 import、記錄來源判定時間）；回傳狀態／原因／理由確實改變者。
fn apply_plan(
    tx: &Connection,
    plan: &Plan,
    file_name: &str,
    now: DateTime<Utc>,
) -> Result<Vec<(i64, Change)>, CoreError> {
    let at = now.to_rfc3339();
    let mut changed = Vec::new();
    for p in &plan.apply {
        let change = Change {
            status: p.verdict.status,
            reason: p.verdict.reason,
            justification: p.verdict.justification,
            note: import_note(file_name, p.detail.as_deref()),
        };
        // 來源時間不晚於匯入當下（時鐘偏差的檔案不會永遠比本機之後的判定新）
        let decided_at = p.updated.map(|t| format_time(t.min(now)));
        let decider = Decider::Import {
            file: file_name,
            decided_at: decided_at.as_deref(),
        };
        let verdict_changed = p.verdict != p.local;
        // 只更新 detail 時保留「等待區判定不相關」標記（資料確定受影響時仍會自動重新開啟）
        let dismissed = !verdict_changed && is_dismissed(tx, p.finding_id)?;
        triage::apply_decision(tx, p.finding_id, &change, decider, dismissed, &at)?;
        if verdict_changed {
            changed.push((p.finding_id, change));
        }
    }
    Ok(changed)
}

fn is_dismissed(conn: &Connection, finding_id: i64) -> Result<bool, CoreError> {
    Ok(conn.query_row(
        "SELECT dismissed FROM finding WHERE id = ?1",
        [finding_id],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp(bom_ref: &str, name: &str) -> doc::Component {
        doc::Component {
            bom_ref: Some(bom_ref.into()),
            name: Some(name.into()),
            ..doc::Component::default()
        }
    }

    #[test]
    fn descriptor_prefers_document_components() {
        let map = HashMap::from([("abc".to_string(), comp("abc", "openssl"))]);
        assert_eq!(descriptor("abc", &map).name.as_deref(), Some("openssl"));
        let link = descriptor("urn:cdx:3e671687-395b-41f5-a30f-a58921a69b79/1#abc", &map);
        assert_eq!(link.name.as_deref(), Some("openssl"));
    }

    #[test]
    fn bare_purl_or_cpe_reference_is_used_as_identity() {
        let map = HashMap::new();
        let p = descriptor("pkg:deb/debian/openssl@3.0.11-1", &map);
        assert_eq!(p.purl.as_deref(), Some("pkg:deb/debian/openssl@3.0.11-1"));
        let c = descriptor(
            "cpe:2.3:o:paloaltonetworks:pan-os:11.1.4-h7:*:*:*:*:*:*:*",
            &map,
        );
        assert!(c.cpe.is_some() && c.purl.is_none());
        let other = descriptor("uuid-1", &map);
        assert_eq!((other.purl, other.cpe), (None, None));
    }

    #[test]
    fn file_name_is_base_name() {
        assert_eq!(file_name(Path::new("C:/x/team.json.age")), "team.json.age");
    }
}
