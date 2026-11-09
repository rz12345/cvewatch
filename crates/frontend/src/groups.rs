//! 資產清單依資產說明分組（16 spec GUI asset list grouped by system、design D8）。
//!
//! 組依代碼排序，未歸屬任何資產說明的資產在最後一組；沒有可顯示資產的組不輸出。

use std::collections::BTreeMap;

use crate::data::{AssetEntry, AssetListFilter, Snapshot};

/// 未歸屬資產說明的組標題。
pub const UNASSIGNED_GROUP: &str = "未歸屬資產說明";

/// 一組資產。
#[derive(Debug, Clone)]
pub struct AssetGroup<'a> {
    /// 資產說明代碼；未歸屬為 None
    pub code: Option<String>,
    /// 「NET 網路服務」或「未歸屬資產說明」
    pub title: String,
    pub assets: Vec<&'a AssetEntry>,
    /// 組內未處理命中的總數
    pub open_findings: usize,
}

impl AssetGroup<'_> {
    /// 「NET 網路服務（2）」。
    pub fn heading(&self) -> String {
        format!("{}（{}）", self.title, self.assets.len())
    }

    /// 畫面元件的穩定識別（收合狀態以此記錄）。
    pub fn key(&self) -> &str {
        self.code.as_deref().unwrap_or("")
    }
}

impl Snapshot {
    /// 依資產說明分組的資產清單（組內維持 [`Snapshot::asset_list`] 的順序）。
    pub fn asset_groups(&self, filter: &AssetListFilter) -> Vec<AssetGroup<'_>> {
        // 一次掃描：依代碼分組（BTreeMap 依代碼排序），未歸屬另外收集，最後放在後面
        let mut by_code: BTreeMap<&str, Vec<&AssetEntry>> = BTreeMap::new();
        let mut unassigned: Vec<&AssetEntry> = Vec::new();
        for a in self.asset_list(filter) {
            match a.asset.record.system_code.as_deref() {
                Some(code) => by_code.entry(code).or_default().push(a),
                None => unassigned.push(a),
            }
        }
        let mut groups: Vec<AssetGroup<'_>> = by_code
            .into_iter()
            .map(|(code, assets)| self.group(Some(code), assets))
            .collect();
        if !unassigned.is_empty() {
            groups.push(self.group(None, unassigned));
        }
        groups
    }

    fn group<'a>(&self, code: Option<&str>, assets: Vec<&'a AssetEntry>) -> AssetGroup<'a> {
        AssetGroup {
            code: code.map(String::from),
            title: code.map_or_else(|| UNASSIGNED_GROUP.to_string(), |c| self.systems.label(c)),
            open_findings: assets.iter().map(|a| a.open_findings).sum(),
            assets,
        }
    }
}
