//! 命中處理狀態：未處理（open）/ 已處理（resolved）/ 忽略（ignored），附原因、理由與時間軸。
//!
//! 忽略必須帶原因；原因為 not_affected 時必須帶 CycloneDX VEX justification（07 無損轉成 VEX）。

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::CoreError;

string_enum!(
    Status {
        Open => "open",
        Resolved => "resolved",
        Ignored => "ignored",
    }
);

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Open => "未處理",
            Status::Resolved => "已處理",
            Status::Ignored => "忽略",
        }
    }
}

string_enum!(
    IgnoreReason {
        NotAffected => "not_affected",
        RiskAccepted => "risk_accepted",
        FalsePositive => "false_positive",
    }
);

impl IgnoreReason {
    pub fn label(self) -> &'static str {
        match self {
            IgnoreReason::NotAffected => "判定不受影響",
            IgnoreReason::RiskAccepted => "接受風險",
            IgnoreReason::FalsePositive => "比對錯誤",
        }
    }
}

string_enum!(
    /// CycloneDX VEX `analysis.justification` 允許值。
    Justification {
        CodeNotPresent => "code_not_present",
        CodeNotReachable => "code_not_reachable",
        RequiresConfiguration => "requires_configuration",
        RequiresDependency => "requires_dependency",
        RequiresEnvironment => "requires_environment",
        ProtectedByCompiler => "protected_by_compiler",
        ProtectedAtRuntime => "protected_at_runtime",
        ProtectedAtPerimeter => "protected_at_perimeter",
        ProtectedByMitigatingControl => "protected_by_mitigating_control",
    }
);

/// 使用者要求的狀態變更（未驗證）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeRequest {
    pub status: String,
    pub reason: Option<String>,
    pub justification: Option<String>,
    pub note: String,
}

/// 驗證後的變更。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub status: Status,
    pub reason: Option<IgnoreReason>,
    pub justification: Option<Justification>,
    pub note: String,
}

fn reasons() -> String {
    IgnoreReason::ALL
        .iter()
        .map(|r| format!("{}（{}）", r.as_str(), r.label()))
        .collect::<Vec<_>>()
        .join("、")
}

pub fn validate(req: &ChangeRequest) -> Result<Change, CoreError> {
    let status = Status::parse(&req.status).ok_or_else(|| {
        CoreError::Invalid(format!(
            "未知的狀態 `{}`；可用：{}",
            req.status,
            Status::allowed()
        ))
    })?;
    let (reason, justification) = if status == Status::Ignored {
        ignore_details(req)?
    } else if req.reason.is_some() || req.justification.is_some() {
        return Err(CoreError::Invalid(
            "--reason / --justification 只能在設為 ignored 時使用".into(),
        ));
    } else {
        (None, None)
    };
    Ok(Change {
        status,
        reason,
        justification,
        note: req.note.clone(),
    })
}

/// 忽略的原因（必填）與理由（原因為 not_affected 時必填）。
fn ignore_details(
    req: &ChangeRequest,
) -> Result<(Option<IgnoreReason>, Option<Justification>), CoreError> {
    let raw = req.reason.as_deref().ok_or_else(|| {
        CoreError::Invalid(format!(
            "設為 ignored 必須以 --reason 指定原因；可用：{}",
            reasons()
        ))
    })?;
    let reason = IgnoreReason::parse(raw)
        .ok_or_else(|| CoreError::Invalid(format!("未知的原因 `{raw}`；可用：{}", reasons())))?;
    let justification = match (reason, req.justification.as_deref()) {
        (IgnoreReason::NotAffected, None) => {
            return Err(CoreError::Invalid(format!(
                "原因為 not_affected 時必須以 --justification 指定理由；可用：{}",
                Justification::allowed()
            )));
        }
        (IgnoreReason::NotAffected, Some(j)) => Some(Justification::parse(j).ok_or_else(|| {
            CoreError::Invalid(format!(
                "未知的 justification `{j}`；可用：{}",
                Justification::allowed()
            ))
        })?),
        (_, Some(_)) => {
            return Err(CoreError::Invalid(
                "--justification 只能搭配 --reason not_affected".into(),
            ));
        }
        (_, None) => None,
    };
    Ok((Some(reason), justification))
}

/// 寫入一筆時間軸事件。
pub fn record_event(
    conn: &Connection,
    finding_id: i64,
    at: &str,
    actor: &str,
    from: Option<Status>,
    to: Status,
    note: &str,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO finding_event (finding_id, at, actor, from_status, to_status, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            finding_id,
            at,
            actor,
            from.map(Status::as_str),
            to.as_str(),
            note
        ],
    )?;
    Ok(())
}

/// 使用者變更狀態：一律記錄事件（備註可為空字串）。KEV 狀態同時更新，
/// 使已在 KEV 的 CVE 被忽略後不會因「新列入 KEV」而重新開啟。
pub fn set_status(
    conn: &Connection,
    finding_id: i64,
    change: &Change,
    now: DateTime<Utc>,
) -> Result<(), CoreError> {
    apply_user_change(conn, finding_id, change, false, &now.to_rfc3339())
}

/// 使用者的狀態變更（`finding set`、等待區 `--dismiss` 共用）：同步 KEV 狀態並記錄事件。
/// `dismissed` 為真時標記「因等待區判定不相關而忽略」。
pub fn apply_user_change(
    conn: &Connection,
    finding_id: i64,
    change: &Change,
    dismissed: bool,
    now: &str,
) -> Result<(), CoreError> {
    let current: Option<(String, String, bool, bool)> = conn
        .query_row(
            "SELECT status, cve_id, in_range, kev_listed FROM finding WHERE id = ?1",
            params![finding_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((status, cve_id, in_range, kev_listed)) = current else {
        return Err(CoreError::Invalid(format!("找不到命中 {finding_id}")));
    };
    let from = parse_status(finding_id, &status)?;
    let in_kev: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM kev WHERE cve_id = ?1)",
        params![cve_id],
        |r| r.get(0),
    )?;
    // KEV 狀態只在仍受影響時記錄（與比對相同）：不受影響期間列入 KEV，回到受影響範圍時仍會重新開啟
    let kev_listed = super::findings::kev_to_record(in_range, in_kev, kev_listed);
    conn.execute(
        "UPDATE finding SET status = ?2, ignore_reason = ?3, justification = ?4, kev_listed = ?5,
                dismissed = ?6
         WHERE id = ?1",
        params![
            finding_id,
            change.status.as_str(),
            change.reason.map(IgnoreReason::as_str),
            change.justification.map(Justification::as_str),
            kev_listed,
            dismissed
        ],
    )?;
    record_event(
        conn,
        finding_id,
        now,
        "user",
        Some(from),
        change.status,
        &change.note,
    )
}

/// 資料庫中的命中狀態；無法辨識時回報錯誤（不當成新命中或略過）。
pub(crate) fn parse_status(finding_id: i64, text: &str) -> Result<Status, CoreError> {
    Status::parse(text)
        .ok_or_else(|| CoreError::Invalid(format!("命中 {finding_id} 的狀態「{text}」無法辨識")))
}
