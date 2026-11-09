//! 由 ISMS 清冊產生草稿（17 design D7）：與 `cvewatch asset isms-draft` 相同的 core 函式；
//! 只讀取資料庫、不寫入，所以不需要 `cvewatch.exe` 輔助程式，也不觸發比對。

use std::path::{Path, PathBuf};

use cvewatch_core::assets::isms::{generate_draft, DraftContext, DraftReport};
use cvewatch_core::assets::template::ensure_absent;

use super::Workspace;
use crate::report::Report;

/// 補版本後再匯入、不要勾選退役選項的提醒（畫面用語）。
pub const ISMS_DRAFT_NEXT_STEP: &str =
    "補上版本後再以「匯入清冊」匯入，不要勾選「將清冊中消失的資產標為退役」（--prune）";

/// 產生草稿後的狀態列訊息：路徑、摘要與提醒；有略過的列或資產說明警告時為警告層級。
pub fn isms_draft_report(target: &Path, report: &DraftReport) -> Report {
    let message = format!(
        "已產生 ISMS 草稿：{}；{}。{ISMS_DRAFT_NEXT_STEP}。明細見草稿的 isms_report 工作表（此檔未加密）",
        target.display(),
        report.summary()
    );
    if report.skipped_rows.is_empty() && report.system_name_conflicts.is_empty() {
        Report::info(message)
    } else {
        Report::warning(message)
    }
}

impl<V> Workspace<V> {
    /// 「由 ISMS 清冊產生草稿」的預設資料夾（「文件」）與檔名（含顯示時區的當天日期）。
    pub fn isms_draft_default(&self) -> (PathBuf, String) {
        self.dated_xlsx_default("cvewatch-isms-draft")
    }

    /// 讀取 ISMS 清冊 `source` 並寫出草稿到 `target`（不覆蓋既有檔案、不修改資料庫）。
    pub fn write_isms_draft(&self, source: &Path, target: &Path) -> Report {
        let result = ensure_absent(target)
            .and_then(|()| {
                DraftContext::load(&self.conn, &self.settings.classes, &self.settings.catalog)
            })
            .and_then(|ctx| generate_draft(source, target, &ctx));
        match result {
            Ok(report) => isms_draft_report(target, &report),
            Err(e) => Report::failure("無法產生 ISMS 草稿", &e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Level;

    #[test]
    fn summary_counts_and_reminders() {
        let report = DraftReport {
            converted_new: 5,
            converted_existing: 1,
            skipped_by_major: [("PE".to_string(), 4)].into_iter().collect(),
            missing_versions: 7,
            ..DraftReport::default()
        };
        let r = isms_draft_report(Path::new(r"C:\Docs\d.xlsx"), &report);
        assert_eq!(r.level, Level::Info);
        for part in [
            r"C:\Docs\d.xlsx",
            "轉換 6 個資產（沿用既有 1）",
            "略過 4 列",
            "待補版本 7 個元件",
            "--prune",
            "匯入清冊",
        ] {
            assert!(r.message.contains(part), "{part}：{}", r.message);
        }
        let skipped = DraftReport {
            skipped_rows: vec![cvewatch_core::assets::isms::row::SkipReason {
                row: 9,
                message: "x".into(),
            }],
            ..report
        };
        assert_eq!(
            isms_draft_report(Path::new("d.xlsx"), &skipped).level,
            Level::Warning
        );
    }
}
