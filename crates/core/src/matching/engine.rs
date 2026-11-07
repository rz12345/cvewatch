//! 證據彙整：依優先序（原廠公告 > CNA > NVD > ADP；套件元件只看 OSV）決定最終判定。
//!
//! - 取優先序最高、且結論不是未知的來源作為最終判定
//! - 最終判定為不受影響、但有其他來源判定受影響 → 不建立命中，記錄矛盾
//! - 全部未知（但產品相符）→ probable 命中
//! - 最終判定受影響 → confirmed 命中

use super::evidence::{sort_by_precedence, Evidence, Source, Verdict};

string_enum!(
    Confidence {
        Confirmed => "confirmed",
        Probable => "probable",
    }
);

/// 最終判定與持相反結論（受影響）的來源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conflict {
    pub final_source: Source,
    pub final_verdict: Verdict,
    pub other_source: Source,
    pub other_verdict: Verdict,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub verdict: Verdict,
    /// 最終判定來源；全部未知時為優先序最高的相符來源
    pub decided_by: Source,
    /// 應建立命中時為 Some（受影響 → confirmed，全部未知 → probable）
    pub confidence: Option<Confidence>,
    pub conflicts: Vec<Conflict>,
    /// 依優先序排列的全部證據
    pub evidence: Vec<Evidence>,
}

impl Decision {
    /// 命中是否仍在受影響範圍（confirmed 或 probable）。
    pub fn in_range(&self) -> bool {
        self.confidence.is_some()
    }

    pub fn has_version_data(&self) -> bool {
        self.evidence.iter().any(|e| e.has_version_data)
    }

    pub fn investigating(&self) -> bool {
        self.evidence.iter().any(|e| e.investigating)
    }
}

/// 沒有任何來源提到此產品（`evidence` 為空）時為 None。
pub fn decide(mut evidence: Vec<Evidence>) -> Option<Decision> {
    sort_by_precedence(&mut evidence);
    let first = evidence.first()?.source;
    let Some(final_evidence) = evidence.iter().find(|e| e.verdict != Verdict::Unknown) else {
        return Some(Decision {
            verdict: Verdict::Unknown,
            decided_by: first,
            confidence: Some(Confidence::Probable),
            conflicts: Vec::new(),
            evidence,
        });
    };
    let (source, verdict) = (final_evidence.source, final_evidence.verdict);
    let conflicts = if verdict == Verdict::NotAffected {
        evidence
            .iter()
            .filter(|e| e.verdict == Verdict::Affected)
            .map(|e| Conflict {
                final_source: source,
                final_verdict: verdict,
                other_source: e.source,
                other_verdict: e.verdict,
            })
            .collect()
    } else {
        Vec::new()
    };
    let confidence = (verdict == Verdict::Affected).then_some(Confidence::Confirmed);
    Some(Decision {
        verdict,
        decided_by: source,
        confidence,
        conflicts,
        evidence,
    })
}
