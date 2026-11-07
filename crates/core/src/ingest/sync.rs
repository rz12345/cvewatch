//! `cvewatch sync` 的來源排程：`cvelist → nvd → csaf:* → kev → epss → osv`。
//!
//! 每個來源各自交易、各自寫 `sync_run`；失敗的來源保留先前資料與游標，其他來源照常進行，
//! 最後以 [`SyncReport::exit_code`] 回報（有任何失敗 → 結束碼 4）。

use rusqlite::Connection;

use super::csaf::{self, AdvisorySource, CsafReader};
use super::source::{SyncCtx, SyncStats};
use super::{cvelist, epss, kev, nvd, osv, runlog};
use crate::config::{CsafSourceConfig, SourcesConfig};
use crate::error::{CoreError, ExitCode};

/// `--source` 可用的名稱（另可用 `csaf:<id>`）。
pub const SOURCE_NAMES: [&str; 6] = ["cvelist", "nvd", "csaf", "kev", "epss", "osv"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceKind {
    Cvelist,
    Nvd,
    Csaf(CsafSourceConfig),
    Kev,
    Epss,
    Osv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePlan {
    /// `sync_run.source`，例如 `nvd`、`csaf:paloalto`
    pub name: String,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOutcome {
    pub name: String,
    pub result: Result<SyncStats, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub outcomes: Vec<SourceOutcome>,
}

impl SyncReport {
    pub fn failed(&self) -> impl Iterator<Item = &SourceOutcome> {
        self.outcomes.iter().filter(|o| o.result.is_err())
    }

    pub fn exit_code(&self) -> ExitCode {
        if self.failed().next().is_some() {
            ExitCode::PartialFailure
        } else {
            ExitCode::Success
        }
    }
}

/// 依設定與 `--source` 決定要執行的來源（固定順序）。未指定時執行所有啟用的來源；
/// 指定時即使設定停用也執行（使用者明確要求）。
pub fn plan(sources: &SourcesConfig, selected: &[String]) -> Result<Vec<SourcePlan>, CoreError> {
    let enabled_csaf = sources.csaf_sources();
    let all_csaf = sources.csaf_sources_all();
    for s in selected {
        let known = SOURCE_NAMES.contains(&s.as_str())
            || s.strip_prefix("csaf:")
                .is_some_and(|id| all_csaf.iter().any(|c| c.id == id));
        if !known {
            let ids: Vec<String> = all_csaf.iter().map(|c| csaf::source_name(&c.id)).collect();
            return Err(CoreError::Invalid(format!(
                "未知的來源 `{s}`；可用：{}、{}",
                SOURCE_NAMES.join("、"),
                ids.join("、")
            )));
        }
    }
    let wanted = |name: &str, enabled: bool| {
        if selected.is_empty() {
            enabled
        } else {
            selected.iter().any(|s| s == name)
        }
    };
    let mut out = Vec::new();
    let mut push = |name: String, kind: SourceKind| out.push(SourcePlan { name, kind });
    if wanted("cvelist", true) {
        push("cvelist".into(), SourceKind::Cvelist);
    }
    if wanted("nvd", sources.nvd_enabled) {
        push("nvd".into(), SourceKind::Nvd);
    }
    for c in all_csaf {
        let name = csaf::source_name(&c.id);
        let enabled = enabled_csaf.iter().any(|e| e.id == c.id);
        // 明確指定 csaf:<id> 時即使停用也執行；未指定或指定 csaf 時只跑啟用者
        let picked = selected.contains(&name)
            || (enabled && (selected.is_empty() || selected.iter().any(|s| s == "csaf")));
        if picked {
            push(name, SourceKind::Csaf(c));
        }
    }
    if wanted("kev", true) {
        push("kev".into(), SourceKind::Kev);
    }
    if wanted("epss", true) {
        push("epss".into(), SourceKind::Epss);
    }
    if wanted("osv", sources.osv_enabled) {
        push("osv".into(), SourceKind::Osv);
    }
    Ok(out)
}

async fn run_one(
    ctx: &SyncCtx<'_>,
    conn: &mut Connection,
    kind: &SourceKind,
) -> Result<SyncStats, CoreError> {
    match kind {
        SourceKind::Cvelist => cvelist::sync(ctx, conn).await,
        SourceKind::Nvd => nvd::sync(ctx, conn).await,
        SourceKind::Csaf(config) => {
            let reader = CsafReader {
                config: config.clone(),
            };
            reader.sync(ctx, conn).await
        }
        SourceKind::Kev => kev::sync(ctx, conn).await,
        SourceKind::Epss => epss::sync(ctx, conn).await,
        SourceKind::Osv => osv::sync(ctx, conn).await,
    }
}

/// 依序執行；單一來源失敗只記錄，不中止。只有寫 `sync_run` 本身失敗才回傳錯誤。
pub async fn run(
    ctx: &SyncCtx<'_>,
    conn: &mut Connection,
    plans: &[SourcePlan],
) -> Result<SyncReport, CoreError> {
    let mut report = SyncReport::default();
    for p in plans {
        let run_id = runlog::start(conn, &p.name, &ctx.now_str())?;
        tracing::info!(source = %p.name, "開始同步來源");
        let result = run_one(ctx, conn, &p.kind).await;
        let finished = chrono::Utc::now().to_rfc3339();
        match &result {
            Ok(stats) => {
                runlog::succeed(conn, run_id, stats, &finished)?;
                tracing::info!(source = %p.name, added = stats.added, updated = stats.updated, "來源同步完成");
            }
            Err(e) => {
                let message = e.to_string();
                runlog::fail(conn, run_id, &message, &finished)?;
                tracing::error!(source = %p.name, error = %message, "來源同步失敗");
            }
        }
        report.outcomes.push(SourceOutcome {
            name: p.name.clone(),
            result: result.map_err(|e| e.to_string()),
        });
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(plans: &[SourcePlan]) -> Vec<&str> {
        plans.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn default_order() {
        let p = plan(&SourcesConfig::default(), &[]).unwrap();
        assert_eq!(
            names(&p),
            vec!["cvelist", "nvd", "csaf:paloalto", "kev", "epss", "osv"]
        );
    }

    #[test]
    fn disabled_sources_are_skipped_unless_selected() {
        let cfg = SourcesConfig {
            nvd_enabled: false,
            osv_enabled: false,
            ..SourcesConfig::default()
        };
        assert_eq!(
            names(&plan(&cfg, &[]).unwrap()),
            vec!["cvelist", "csaf:paloalto", "kev", "epss"]
        );
        assert_eq!(names(&plan(&cfg, &["nvd".into()]).unwrap()), vec!["nvd"]);
        let no_pa = SourcesConfig {
            paloalto_enabled: false,
            ..SourcesConfig::default()
        };
        assert!(!names(&plan(&no_pa, &[]).unwrap()).contains(&"csaf:paloalto"));
        assert!(!names(&plan(&no_pa, &["csaf".into()]).unwrap()).contains(&"csaf:paloalto"));
        assert_eq!(
            names(&plan(&no_pa, &["csaf:paloalto".into()]).unwrap()),
            vec!["csaf:paloalto"]
        );
    }

    #[test]
    fn selection_keeps_order_and_validates() {
        let p = plan(
            &SourcesConfig::default(),
            &["kev".into(), "csaf:paloalto".into()],
        )
        .unwrap();
        assert_eq!(names(&p), vec!["csaf:paloalto", "kev"]);
        let all_csaf = plan(&SourcesConfig::default(), &["csaf".into()]).unwrap();
        assert_eq!(names(&all_csaf), vec!["csaf:paloalto"]);
        let err = plan(&SourcesConfig::default(), &["csaf:nope".into()]).unwrap_err();
        assert!(err.to_string().contains("未知的來源"), "{err}");
    }

    #[test]
    fn report_exit_code() {
        let ok = SourceOutcome {
            name: "kev".into(),
            result: Ok(SyncStats::default()),
        };
        let bad = SourceOutcome {
            name: "nvd".into(),
            result: Err("HTTP 503".into()),
        };
        let all_ok = SyncReport {
            outcomes: vec![ok.clone()],
        };
        assert_eq!(all_ok.exit_code(), ExitCode::Success);
        let partial = SyncReport {
            outcomes: vec![ok, bad],
        };
        assert_eq!(partial.exit_code(), ExitCode::PartialFailure);
    }
}
