//! cvewatch 處理狀態 ↔ CycloneDX VEX `analysis` 的雙向對應（design.md「狀態對應」表）。
//!
//! 匯出：open → in_triage；resolved → resolved + [update]；ignored/not_affected → not_affected +
//! justification；ignored/risk_accepted → exploitable + [will_not_fix]；ignored/false_positive →
//! false_positive。匯入為反向對應；不完整（缺少 analysis、not_affected 缺理由、未知 state）
//! 時回傳 [`Incomplete`]，不套用。

use crate::matching::triage::{IgnoreReason, Justification, Status};

string_enum!(
    /// CycloneDX `analysis.state`（1.4–1.6 相同）。
    AnalysisState {
        Resolved => "resolved",
        ResolvedWithPedigree => "resolved_with_pedigree",
        Exploitable => "exploitable",
        InTriage => "in_triage",
        FalsePositive => "false_positive",
        NotAffected => "not_affected",
    }
);

string_enum!(
    /// CycloneDX `analysis.response`。
    Response {
        CanNotFix => "can_not_fix",
        WillNotFix => "will_not_fix",
        Update => "update",
        Rollback => "rollback",
        WorkaroundAvailable => "workaround_available",
    }
);

/// cvewatch 一筆命中的處理判定（狀態、忽略原因、不受影響理由）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    pub status: Status,
    pub reason: Option<IgnoreReason>,
    pub justification: Option<Justification>,
}

/// 匯出用的 analysis 欄位（detail 與時間另行處理）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VexAnalysis {
    pub state: AnalysisState,
    pub justification: Option<Justification>,
    pub response: Vec<Response>,
}

/// 匯入時無法套用的原因（報告為「格式不完整」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incomplete(pub String);

impl Verdict {
    pub fn open() -> Self {
        Verdict {
            status: Status::Open,
            reason: None,
            justification: None,
        }
    }

    pub fn resolved() -> Self {
        Verdict {
            status: Status::Resolved,
            reason: None,
            justification: None,
        }
    }

    pub fn ignored(reason: IgnoreReason, justification: Option<Justification>) -> Self {
        Verdict {
            status: Status::Ignored,
            reason: Some(reason),
            justification,
        }
    }
}

/// cvewatch → VEX。資料庫保證 not_affected 一定有理由；萬一缺少仍輸出 not_affected
/// （匯入時會列為缺少理由，不會被誤還原成其他狀態）。
pub fn to_analysis(v: &Verdict) -> VexAnalysis {
    let (state, response) = match (v.status, v.reason) {
        (Status::Open, _) => (AnalysisState::InTriage, Vec::new()),
        (Status::Resolved, _) => (AnalysisState::Resolved, vec![Response::Update]),
        (Status::Ignored, Some(IgnoreReason::RiskAccepted)) => {
            (AnalysisState::Exploitable, vec![Response::WillNotFix])
        }
        (Status::Ignored, Some(IgnoreReason::FalsePositive)) => {
            (AnalysisState::FalsePositive, Vec::new())
        }
        (Status::Ignored, Some(IgnoreReason::NotAffected) | None) => {
            (AnalysisState::NotAffected, Vec::new())
        }
    };
    let justification = (state == AnalysisState::NotAffected)
        .then_some(v.justification)
        .flatten();
    VexAnalysis {
        state,
        justification,
        response,
    }
}

/// VEX → cvewatch。`state` 為 `None` 表示沒有 `analysis` 或缺少 state。
/// 未知的 response 值忽略（其他工具可能有擴充），未知的 state 與理由視為不完整。
pub fn from_analysis(
    state: Option<&str>,
    justification: Option<&str>,
    responses: &[String],
) -> Result<Verdict, Incomplete> {
    let raw = state.ok_or_else(|| Incomplete("缺少 analysis.state".into()))?;
    let state = AnalysisState::parse(raw)
        .ok_or_else(|| Incomplete(format!("未知的 analysis.state「{raw}」")))?;
    let declined = responses.iter().any(|r| {
        matches!(
            Response::parse(r),
            Some(Response::WillNotFix | Response::CanNotFix)
        )
    });
    Ok(match state {
        AnalysisState::InTriage => Verdict::open(),
        AnalysisState::Exploitable if declined => {
            Verdict::ignored(IgnoreReason::RiskAccepted, None)
        }
        AnalysisState::Exploitable => Verdict::open(),
        AnalysisState::Resolved | AnalysisState::ResolvedWithPedigree => Verdict::resolved(),
        AnalysisState::FalsePositive => Verdict::ignored(IgnoreReason::FalsePositive, None),
        AnalysisState::NotAffected => {
            let raw = justification
                .map(str::trim)
                .filter(|j| !j.is_empty())
                .ok_or_else(|| Incomplete("not_affected 缺少理由（justification）".into()))?;
            let j = Justification::parse(raw)
                .ok_or_else(|| Incomplete(format!("未知的 justification「{raw}」")))?;
            Verdict::ignored(IgnoreReason::NotAffected, Some(j))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // ── 匯出（每一列對應表） ─────────────────────────────

    #[test]
    fn open_exports_in_triage() {
        let a = to_analysis(&Verdict::open());
        assert_eq!(a.state, AnalysisState::InTriage);
        assert!(a.response.is_empty());
        assert_eq!(a.justification, None);
    }

    #[test]
    fn resolved_exports_update_response() {
        let a = to_analysis(&Verdict::resolved());
        assert_eq!(
            (a.state, a.response),
            (AnalysisState::Resolved, vec![Response::Update])
        );
    }

    #[test]
    fn not_affected_exports_justification() {
        let v = Verdict::ignored(
            IgnoreReason::NotAffected,
            Some(Justification::RequiresConfiguration),
        );
        let a = to_analysis(&v);
        assert_eq!(a.state, AnalysisState::NotAffected);
        assert_eq!(a.justification, Some(Justification::RequiresConfiguration));
        assert!(a.response.is_empty());
    }

    #[test]
    fn risk_accepted_exports_exploitable_will_not_fix() {
        let a = to_analysis(&Verdict::ignored(IgnoreReason::RiskAccepted, None));
        assert_eq!(
            (a.state, a.response),
            (AnalysisState::Exploitable, vec![Response::WillNotFix])
        );
    }

    #[test]
    fn false_positive_exports_false_positive() {
        let a = to_analysis(&Verdict::ignored(IgnoreReason::FalsePositive, None));
        assert_eq!(a.state, AnalysisState::FalsePositive);
        assert!(a.response.is_empty());
    }

    #[test]
    fn every_verdict_round_trips() {
        let mut verdicts = vec![
            Verdict::open(),
            Verdict::resolved(),
            Verdict::ignored(IgnoreReason::RiskAccepted, None),
            Verdict::ignored(IgnoreReason::FalsePositive, None),
        ];
        verdicts.extend(
            Justification::ALL
                .iter()
                .map(|j| Verdict::ignored(IgnoreReason::NotAffected, Some(*j))),
        );
        for v in verdicts {
            let a = to_analysis(&v);
            let responses: Vec<String> = a.response.iter().map(|r| r.to_string()).collect();
            let back = from_analysis(
                Some(a.state.as_str()),
                a.justification.map(Justification::as_str),
                &responses,
            );
            assert_eq!(back, Ok(v), "{v:?}");
        }
    }

    // ── 匯入（反向對應表） ───────────────────────────────

    #[test]
    fn in_triage_imports_open() {
        assert_eq!(
            from_analysis(Some("in_triage"), None, &[]),
            Ok(Verdict::open())
        );
    }

    #[test]
    fn exploitable_without_decline_imports_open() {
        let r = strings(&["update", "workaround_available"]);
        assert_eq!(
            from_analysis(Some("exploitable"), None, &r),
            Ok(Verdict::open())
        );
        assert_eq!(
            from_analysis(Some("exploitable"), None, &[]),
            Ok(Verdict::open())
        );
    }

    #[test]
    fn exploitable_will_not_fix_or_can_not_fix_imports_risk_accepted() {
        for r in ["will_not_fix", "can_not_fix", "CAN_NOT_FIX"] {
            assert_eq!(
                from_analysis(Some("exploitable"), None, &strings(&["update", r])),
                Ok(Verdict::ignored(IgnoreReason::RiskAccepted, None)),
                "{r}"
            );
        }
    }

    #[test]
    fn resolved_and_with_pedigree_import_resolved() {
        for s in ["resolved", "resolved_with_pedigree"] {
            assert_eq!(from_analysis(Some(s), None, &[]), Ok(Verdict::resolved()));
        }
    }

    #[test]
    fn not_affected_with_justification_imports_ignored() {
        assert_eq!(
            from_analysis(Some("not_affected"), Some("code_not_present"), &[]),
            Ok(Verdict::ignored(
                IgnoreReason::NotAffected,
                Some(Justification::CodeNotPresent)
            ))
        );
    }

    #[test]
    fn false_positive_imports_ignored_false_positive() {
        assert_eq!(
            from_analysis(Some("false_positive"), None, &[]),
            Ok(Verdict::ignored(IgnoreReason::FalsePositive, None))
        );
    }

    #[test]
    fn not_affected_without_justification_is_incomplete() {
        for j in [None, Some(""), Some("  ")] {
            let err = from_analysis(Some("not_affected"), j, &[]).unwrap_err();
            assert!(err.0.contains("缺少理由"), "{j:?}: {}", err.0);
        }
    }

    #[test]
    fn unknown_justification_is_incomplete() {
        let err = from_analysis(Some("not_affected"), Some("because"), &[]).unwrap_err();
        assert!(err.0.contains("because"));
    }

    #[test]
    fn missing_or_unknown_state_is_incomplete() {
        assert!(from_analysis(None, None, &[])
            .unwrap_err()
            .0
            .contains("缺少"));
        assert!(from_analysis(Some("maybe"), None, &[])
            .unwrap_err()
            .0
            .contains("maybe"));
    }

    #[test]
    fn unknown_response_values_are_ignored() {
        let r = strings(&["something_new"]);
        assert_eq!(
            from_analysis(Some("exploitable"), None, &r),
            Ok(Verdict::open())
        );
    }
}
