//! ISMS 匯入報表（17 design D6、18 design D6）：CLI 預覽與完成摘要（GUI 預覽顯示同一份輸出）。

use std::collections::BTreeMap;

use super::row::SkipReason;

/// 依大類略過時的顯示順序（其他大類依代碼排序接在後面）。
const MAJOR_ORDER: [&str; 3] = ["PE", "IF", "SE"];

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

/// ISMS 匯入報表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IsmsReport {
    pub converted_new: usize,
    pub converted_existing: usize,
    /// HW、SW 以外的大類 → 列數
    pub skipped_by_major: BTreeMap<String, usize>,
    pub skipped_rows: Vec<SkipReason>,
    pub merged: Vec<MergedProducts>,
    pub unresolved_lines: Vec<UnresolvedLine>,
    /// 這次新增的版本待補目錄產品元件數（既有的待補元件不計）
    pub pending_components: usize,
    /// 資料庫中 active、但不在這次匯入結果中的資產（維持 active）
    pub not_in_workbook: Vec<String>,
    /// 同一系統代碼出現不同資產說明的警告
    pub system_name_conflicts: Vec<String>,
}

/// 報表的一類明細：標題與各筆內容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: &'static str,
    pub entries: Vec<String>,
}

impl IsmsReport {
    pub fn converted(&self) -> usize {
        self.converted_new + self.converted_existing
    }

    /// 略過的列數（依大類略過與個別略過）。
    pub fn skipped(&self) -> usize {
        self.skipped_by_major.values().sum::<usize>() + self.skipped_rows.len()
    }

    /// 併入一列的描述比對結果（合併的產品、未辨識的描述行、新增的待補元件數）。
    pub(super) fn absorb(&mut self, other: IsmsReport) {
        self.merged.extend(other.merged);
        self.unresolved_lines.extend(other.unresolved_lines);
        self.pending_components += other.pending_components;
    }

    /// 「ISMS 清冊：可匯入 6 列（既有資產 1）、略過 11 列；這次新加入的版本待補元件 7 個」。
    pub fn summary(&self) -> String {
        format!(
            "ISMS 清冊：可匯入 {} 列（既有資產 {}）、略過 {} 列；這次新加入的版本待補元件 {} 個",
            self.converted(),
            self.converted_existing,
            self.skipped(),
            self.pending_components
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
                "{}：{} 已合併為一個 {} 元件；兩處版本不同時，請匯入後在畫面另外新增資產",
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
            format!(
                "{}：「{}」比對不到目錄產品，匯入為版本待補的未辨識產品（{hint}）",
                u.isms_id, u.text
            )
        });
        let sections = [
            (
                "略過的列",
                self.skipped_rows.iter().map(|s| s.to_string()).collect(),
            ),
            ("合併的產品", merged.collect()),
            ("未辨識的描述行", unresolved.collect()),
            ("資產說明", self.system_name_conflicts.clone()),
            (
                "不在 ISMS 清冊",
                self.not_in_workbook
                    .iter()
                    .map(|t| format!("{t} 不在 ISMS 清冊中（維持使用中）"))
                    .collect(),
            ),
        ];
        sections
            .into_iter()
            .filter(|(_, entries): &(_, Vec<String>)| !entries.is_empty())
            .map(|(title, entries)| Section { title, entries })
            .collect()
    }

    /// CLI 輸出：摘要、依大類略過與各類明細（全部列出：沒有另外的報表檔可查）。
    pub fn cli_lines(&self) -> Vec<String> {
        let mut out = vec![self.summary()];
        out.extend(self.major_line());
        for section in self.sections() {
            out.push(format!("{}（{}）：", section.title, section.entries.len()));
            out.extend(section.entries.iter().map(|e| format!("  {e}")));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> IsmsReport {
        IsmsReport {
            converted_new: 5,
            converted_existing: 1,
            skipped_by_major: [("SE".into(), 2), ("IF".into(), 5), ("PE".into(), 3)]
                .into_iter()
                .collect(),
            skipped_rows: vec![SkipReason {
                row: 44,
                message: "ISMS 編號「WEB-SE-003」的大類 SE 與資產類別 SW01 不符".into(),
            }],
            pending_components: 7,
            not_in_workbook: vec!["PVE-01".into()],
            ..IsmsReport::default()
        }
    }

    #[test]
    fn summary_and_majors() {
        let r = report();
        assert_eq!(
            r.summary(),
            "ISMS 清冊：可匯入 6 列（既有資產 1）、略過 11 列；這次新加入的版本待補元件 7 個"
        );
        assert_eq!(r.major_line().as_deref(), Some("略過 PE 3、IF 5、SE 2"));
        assert_eq!(IsmsReport::default().major_line(), None);
    }

    #[test]
    fn assets_missing_from_the_isms_workbook() {
        let lines = report().cli_lines();
        assert!(
            lines
                .iter()
                .any(|l| l == "  PVE-01 不在 ISMS 清冊中（維持使用中）"),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.contains("--prune")), "{lines:?}");
    }

    #[test]
    fn cli_lists_every_entry() {
        let r = IsmsReport {
            not_in_workbook: (0..25).map(|i| format!("A-{i:02}")).collect(),
            ..IsmsReport::default()
        };
        let lines = r.cli_lines();
        assert!(lines.contains(&"不在 ISMS 清冊（25）：".to_string()));
        assert_eq!(lines.iter().filter(|l| l.starts_with("  A-")).count(), 25);
    }

    #[test]
    fn merged_and_unresolved_entries() {
        let r = IsmsReport {
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
            ..IsmsReport::default()
        };
        let sections = r.sections();
        assert_eq!(sections[0].title, "合併的產品");
        assert!(sections[0].entries[0]
            .contains("「Core: Aruba XYZ-100 *2」與「Edge: Aruba XYZ-100 *9」"));
        assert!(sections[0].entries[0].contains("在畫面另外新增資產"));
        assert!(sections[1].entries[0].contains("「Zyxel ABCD」比對不到目錄產品"));
    }
}
