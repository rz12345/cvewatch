//! 顯示用 CVSS 的選擇規則：CNA 4.0 → CNA 3.1 → ADP 3.1 → NVD 3.1 → 未評分。
//!
//! 所有來源的分數都保存在 `cve_metric`；這裡只決定顯示哪一筆。同一層有多筆
//! （例如 CNA 對不同情境給兩個 4.0 分數）時取最高分。

use rusqlite::{params, Connection};

use crate::error::CoreError;

/// 沒有任何可用分數時的顯示文字。
pub const UNRATED_LABEL: &str = "未評分";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricSource {
    Cna,
    Adp,
    Nvd,
}

impl MetricSource {
    pub fn as_str(self) -> &'static str {
        match self {
            MetricSource::Cna => "cna",
            MetricSource::Adp => "adp",
            MetricSource::Nvd => "nvd",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cna" => Some(MetricSource::Cna),
            "adp" => Some(MetricSource::Adp),
            "nvd" => Some(MetricSource::Nvd),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MetricSource::Cna => "CNA",
            MetricSource::Adp => "ADP",
            MetricSource::Nvd => "NVD",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricRow {
    pub source: MetricSource,
    pub provider: Option<String>,
    pub version: String,
    pub base_score: f64,
    pub vector: Option<String>,
    pub severity: Option<String>,
}

/// 優先順序（來源、CVSS 版本）。
pub const PREFERENCE: [(MetricSource, &str); 4] = [
    (MetricSource::Cna, "4.0"),
    (MetricSource::Cna, "3.1"),
    (MetricSource::Adp, "3.1"),
    (MetricSource::Nvd, "3.1"),
];

pub fn preferred(metrics: &[MetricRow]) -> Option<&MetricRow> {
    PREFERENCE.iter().find_map(|(source, version)| {
        metrics
            .iter()
            .filter(|m| m.source == *source && m.version == *version)
            .max_by(|a, b| a.base_score.total_cmp(&b.base_score))
    })
}

/// 例如 `9.8 CRITICAL（CNA CVSS 4.0）`；無分數時為「未評分」。
pub fn display(metric: Option<&MetricRow>) -> String {
    match metric {
        None => UNRATED_LABEL.to_string(),
        Some(m) => {
            let severity = m
                .severity
                .as_deref()
                .map(|s| format!(" {s}"))
                .unwrap_or_default();
            format!(
                "{:.1}{severity}（{} CVSS {}）",
                m.base_score,
                m.source.label(),
                m.version
            )
        }
    }
}

/// 讀取某 CVE 的全部 CVSS。
pub fn load_metrics(conn: &Connection, cve_id: &str) -> Result<Vec<MetricRow>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT source, provider, version, base_score, vector, severity
         FROM cve_metric WHERE cve_id = ?1 ORDER BY rowid",
    )?;
    let rows = stmt
        .query_map(params![cve_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                MetricRow {
                    source: MetricSource::Cna,
                    provider: r.get(1)?,
                    version: r.get(2)?,
                    base_score: r.get(3)?,
                    vector: r.get(4)?,
                    severity: r.get(5)?,
                },
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(source, row)| {
            MetricSource::parse(&source).map(|source| MetricRow { source, ..row })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(source: MetricSource, version: &str, score: f64) -> MetricRow {
        MetricRow {
            source,
            provider: None,
            version: version.to_string(),
            base_score: score,
            vector: None,
            severity: Some("HIGH".into()),
        }
    }

    #[test]
    fn display_formats_score_and_source() {
        let row = m(MetricSource::Nvd, "3.1", 7.5);
        assert_eq!(display(Some(&row)), "7.5 HIGH（NVD CVSS 3.1）");
        assert_eq!(display(None), "未評分");
    }

    #[test]
    fn highest_score_wins_within_a_tier() {
        let rows = [
            m(MetricSource::Cna, "4.0", 6.9),
            m(MetricSource::Cna, "4.0", 8.8),
        ];
        assert_eq!(preferred(&rows).unwrap().base_score, 8.8);
    }
}
