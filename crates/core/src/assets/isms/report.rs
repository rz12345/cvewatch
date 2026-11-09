//! 草稿報表（17 design D6）：CLI 摘要、`isms_report` 工作表與 GUI 摘要共用。

use std::collections::BTreeMap;

use super::row::SkipReason;

/// `isms_report` 工作表名稱（匯入時不讀）。
pub const REPORT_SHEET: &str = "isms_report";
/// `isms_report` 工作表的表頭。
pub const REPORT_HEADERS: [&str; 2] = ["項目", "內容"];
/// CLI 每類最多列出的明細筆數。
pub const CLI_DETAIL_LIMIT: usize = 20;

/// 依大類略過時的顯示順序（其他大類依代碼排序接在後面）。
const MAJOR_ORDER: [&str; 3] = ["PE", "IF", "SE"];

/// 匯入草稿時不要使用 `--prune`（GUI 的退役選項）。
pub const PRUNE_REMINDER: &str = "匯入草稿時不要使用 --prune（畫面上不要勾選「將清冊中消失的資產標為退役」）：草稿只含 ISMS 清冊中的資產，其他資產會被標為退役";

/// 補版本後再匯入的提醒。
pub const NEXT_STEP: &str =
    "請在草稿補上所有元件的版本，再以 cvewatch asset import（或畫面的「匯入清冊」）匯入";

/// 同一列有兩行以上比對到同一個目錄產品。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedProducts {
    pub isms_id: String,
    pub key: String,
    pub lines: Vec<String>,
}

/// 比對不到目錄產品的描述行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedLine {
    pub isms_id: String,
    pub text: String,
    pub suggestions: Vec<String>,
}

/// 草稿報表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DraftReport {
    pub converted_new: usize,
    pub converted_existing: usize,
    /// HW、SW 以外的大類 → 列數
    pub skipped_by_major: BTreeMap<String, usize>,
    pub skipped_rows: Vec<SkipReason>,
    pub merged: Vec<MergedProducts>,
    pub unresolved_lines: Vec<UnresolvedLine>,
    /// 版本空白的元件數
    pub missing_versions: usize,
    /// 沒有任何元件的資產（匯入前須補上產品與版本）
    pub no_components: Vec<String>,
    /// 資料庫中 active、但不在這次轉換結果中的資產
    pub not_in_workbook: Vec<String>,
    /// 資產說明名稱的警告（同代碼不同名稱、與其他資產說明衝突）
    pub system_name_conflicts: Vec<String>,
}

/// 報表的一類明細：標題與各筆內容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: &'static str,
    pub entries: Vec<String>,
}

impl DraftReport {
    pub fn converted(&self) -> usize {
        self.converted_new + self.converted_existing
    }

    /// 略過的列數（依大類略過與個別略過）。
    pub fn skipped(&self) -> usize {
        self.skipped_by_major.values().sum::<usize>() + self.skipped_rows.len()
    }

    /// 「轉換 6 個資產（沿用既有 1）、略過 5 列、待補版本 7 個元件」。
    pub fn summary(&self) -> String {
        format!(
            "轉換 {} 個資產（沿用既有 {}）、略過 {} 列、待補版本 {} 個元件",
            self.converted(),
            self.converted_existing,
            self.skipped(),
            self.missing_versions
        )
    }

    /// 「略過 PE 3、IF 5、SE 2」；沒有時為 None。
    pub fn major_line(&self) -> Option<String> {
        if self.skipped_by_major.is_empty() {
            return None;
        }
        let rank = |m: &str| {
            MAJOR_ORDER
                .iter()
                .position(|x| *x == m)
                .unwrap_or(usize::MAX)
        };
        let mut majors: Vec<(&String, &usize)> = self.skipped_by_major.iter().collect();
        majors.sort_by_key(|(m, _)| (rank(m), (*m).clone()));
        let parts: Vec<String> = majors.iter().map(|(m, n)| format!("{m} {n}")).collect();
        Some(format!("略過 {}", parts.join("、")))
    }

    /// 各類明細（沒有內容的類別不列）。
    pub fn sections(&self) -> Vec<Section> {
        let merged = self.merged.iter().map(|m| {
            let lines: Vec<String> = m.lines.iter().map(|l| format!("「{l}」")).collect();
            format!(
                "{}：{} 已合併為一個 {} 元件；兩處版本不同時，請把該資產拆成多個資產",
                m.isms_id,
                lines.join("與"),
                m.key
            )
        });
        let unresolved = self.unresolved_lines.iter().map(|u| {
            let hint = if u.suggestions.is_empty() {
                "可在 catalog.d 為它加入別名".to_string()
            } else {
                format!("相似產品：{}", u.suggestions.join("、"))
            };
            format!("{}：「{}」比對不到目錄產品（{hint}）", u.isms_id, u.text)
        });
        let sections = [
            (
                "略過的列",
                self.skipped_rows.iter().map(|s| s.to_string()).collect(),
            ),
            ("合併的產品", merged.collect()),
            ("未辨識的描述行", unresolved.collect()),
            (
                "沒有元件",
                self.no_components
                    .iter()
                    .map(|t| format!("{t} 沒有任何元件，匯入前須補上產品與版本"))
                    .collect(),
            ),
            ("資產說明", self.system_name_conflicts.clone()),
            (
                "不在 ISMS 清冊",
                self.not_in_workbook
                    .iter()
                    .map(|t| format!("{t} 不在 ISMS 清冊中"))
                    .collect(),
            ),
        ];
        sections
            .into_iter()
            .filter(|(_, entries): &(_, Vec<String>)| !entries.is_empty())
            .map(|(title, entries)| Section { title, entries })
            .collect()
    }

    /// 結尾的提醒：補版本後再匯入、不要使用 `--prune`（一律提醒：草稿只含 ISMS 清冊的資產）。
    pub fn reminders(&self) -> [&'static str; 2] {
        [NEXT_STEP, PRUNE_REMINDER]
    }

    /// CLI 輸出：摘要、各類前 [`CLI_DETAIL_LIMIT`] 筆明細與提醒。
    pub fn cli_lines(&self) -> Vec<String> {
        let mut out = vec![self.summary()];
        out.extend(self.major_line());
        for section in self.sections() {
            out.push(format!("{}（{}）：", section.title, section.entries.len()));
            out.extend(
                section
                    .entries
                    .iter()
                    .take(CLI_DETAIL_LIMIT)
                    .map(|e| format!("  {e}")),
            );
            if section.entries.len() > CLI_DETAIL_LIMIT {
                out.push(format!(
                    "  …還有 {} 筆，請查看草稿的 {REPORT_SHEET} 工作表",
                    section.entries.len() - CLI_DETAIL_LIMIT
                ));
            }
        }
        out.extend(self.reminders().iter().map(|r| r.to_string()));
        out
    }

    /// `isms_report` 工作表的資料列（項目、內容；每筆一列）。
    pub fn sheet_rows(&self) -> Vec<Vec<String>> {
        let mut out = vec![vec!["摘要".to_string(), self.summary()]];
        out.extend(self.major_line().map(|l| vec!["依大類略過".to_string(), l]));
        for section in self.sections() {
            out.extend(
                section
                    .entries
                    .into_iter()
                    .map(|e| vec![section.title.to_string(), e]),
            );
        }
        out.extend(
            self.reminders()
                .into_iter()
                .map(|r| vec!["提醒".to_string(), r.to_string()]),
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> DraftReport {
        DraftReport {
            converted_new: 5,
            converted_existing: 1,
            skipped_by_major: [("SE".into(), 2), ("IF".into(), 5), ("PE".into(), 3)]
                .into_iter()
                .collect(),
            skipped_rows: vec![SkipReason {
                row: 44,
                message: "ISMS 編號「WEB-SE-003」的大類 SE 與資產類別 SW01 不符".into(),
            }],
            missing_versions: 7,
            not_in_workbook: vec!["PVE-01".into()],
            ..DraftReport::default()
        }
    }

    #[test]
    fn summary_and_majors() {
        let r = report();
        assert_eq!(
            r.summary(),
            "轉換 6 個資產（沿用既有 1）、略過 11 列、待補版本 7 個元件"
        );
        assert_eq!(r.major_line().as_deref(), Some("略過 PE 3、IF 5、SE 2"));
        assert_eq!(DraftReport::default().major_line(), None);
    }

    #[test]
    fn assets_missing_from_the_isms_workbook() {
        let lines = report().cli_lines();
        assert!(
            lines.iter().any(|l| l == "  PVE-01 不在 ISMS 清冊中"),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("不要使用 --prune")),
            "{lines:?}"
        );
        let rows = report().sheet_rows();
        assert!(rows.contains(&vec![
            "不在 ISMS 清冊".into(),
            "PVE-01 不在 ISMS 清冊中".into()
        ]));
        assert!(rows
            .iter()
            .any(|r| r[0] == "提醒" && r[1].contains("--prune")));
    }

    #[test]
    fn cli_lists_at_most_twenty_per_section() {
        let r = DraftReport {
            not_in_workbook: (0..25).map(|i| format!("A-{i:02}")).collect(),
            ..DraftReport::default()
        };
        let lines = r.cli_lines();
        assert!(lines.contains(&"不在 ISMS 清冊（25）：".to_string()));
        assert_eq!(lines.iter().filter(|l| l.starts_with("  A-")).count(), 20);
        assert!(lines
            .iter()
            .any(|l| l.contains("還有 5 筆") && l.contains(REPORT_SHEET)));
        assert_eq!(
            r.sheet_rows()
                .iter()
                .filter(|r| r[0] == "不在 ISMS 清冊")
                .count(),
            25
        );
    }

    #[test]
    fn merged_and_unresolved_entries() {
        let r = DraftReport {
            merged: vec![MergedProducts {
                isms_id: "NET-HW-004".into(),
                key: "hpe-aoscx".into(),
                lines: vec![
                    "Core: Aruba XYZ-100 *2".into(),
                    "Edge: Aruba XYZ-100 *9".into(),
                ],
            }],
            unresolved_lines: vec![UnresolvedLine {
                isms_id: "NET-HW-001".into(),
                text: "Zyxel ABCD".into(),
                suggestions: vec![],
            }],
            ..DraftReport::default()
        };
        let sections = r.sections();
        assert_eq!(sections[0].title, "合併的產品");
        assert!(sections[0].entries[0]
            .contains("「Core: Aruba XYZ-100 *2」與「Edge: Aruba XYZ-100 *9」"));
        assert!(sections[1].entries[0].contains("「Zyxel ABCD」比對不到目錄產品"));
    }
}
