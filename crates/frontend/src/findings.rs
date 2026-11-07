//! 命中清單模型：篩選、即時搜尋、排序、多選與可見列。
//!
//! 多選在搜尋縮小再放寬後仍保留，但批次變更只套用到可見的所選命中
//! （[`FindingsModel::visible_selection`]）。游標、輸入框等畫面專屬狀態放在前端提供的
//! `view`（[`ListView`]），可見列改變時由模型通知它修正。

use std::collections::{BTreeSet, HashSet};
use std::ops::{Deref, DerefMut};

use cvewatch_core::matching::query::FindingRow;

use crate::data::Snapshot;
use crate::filter::{self, Filter, SortKey};

/// 前端專屬的清單狀態（例如 TUI 的游標與搜尋輸入框）。
pub trait ListView: Default {
    /// 可見列重新計算後呼叫（`len` 為可見列數），例如把游標修正到範圍內。
    fn visible_changed(&mut self, len: usize);
}

/// 沒有畫面專屬狀態（無畫面的測試或尚未需要時）。
impl ListView for () {
    fn visible_changed(&mut self, _len: usize) {}
}

#[derive(Debug, Clone, Default)]
pub struct FindingsModel<V = ()> {
    pub filter: Filter,
    pub search: String,
    pub sort: SortKey,
    pub selected: BTreeSet<i64>,
    /// 符合條件的列（`Snapshot::findings` 的索引），依畫面順序
    pub visible: Vec<usize>,
    /// 前端專屬的清單狀態。程式內請寫 `model.view.<欄位>`：模型日後新增同名欄位時，
    /// 經 `Deref` 的 `model.<欄位>` 會改指向模型的欄位而不報錯。
    pub view: V,
}

/// 讓既有 TUI 測試的 `app.findings.cursor` 等路徑可用（見 [`FindingsModel::view`] 的注意事項）。
impl<V> Deref for FindingsModel<V> {
    type Target = V;

    fn deref(&self) -> &V {
        &self.view
    }
}

impl<V> DerefMut for FindingsModel<V> {
    fn deref_mut(&mut self) -> &mut V {
        &mut self.view
    }
}

impl<V: ListView> FindingsModel<V> {
    /// 依篩選、搜尋與排序重新計算可見列，並通知前端修正畫面狀態。
    pub fn refresh_visible(&mut self, data: &Snapshot) {
        self.visible = filter::visible(
            &data.findings,
            &data.search_keys,
            &self.filter,
            &self.search,
            self.sort,
        );
        self.view.visible_changed(self.visible.len());
    }

    /// 重新載入後移除已不存在的命中。
    pub fn prune_selection(&mut self, data: &Snapshot) {
        let ids: HashSet<i64> = data.findings.iter().map(|f| f.id).collect();
        self.selected.retain(|id| ids.contains(id));
    }
}

/// 以下查詢都以 `.get()` 取列：`data` 換過而尚未 `refresh_visible` 時，越界的索引略過而不 panic。
impl<V> FindingsModel<V> {
    /// 可見列的命中（依畫面順序）。
    fn visible_rows<'a>(&'a self, data: &'a Snapshot) -> impl Iterator<Item = &'a FindingRow> {
        self.visible.iter().filter_map(|i| data.findings.get(*i))
    }

    /// 目前可見的所選命中（依畫面順序）。
    pub fn visible_selection(&self, data: &Snapshot) -> Vec<i64> {
        self.visible_rows(data)
            .map(|r| r.id)
            .filter(|id| self.selected.contains(id))
            .collect()
    }

    /// 目前篩選（含搜尋）的命中 id，依畫面順序。
    pub fn visible_ids(&self, data: &Snapshot) -> Vec<i64> {
        self.visible_rows(data).map(|r| r.id).collect()
    }

    /// 可見列第 `pos` 列的命中。
    pub fn row_at<'a>(&self, data: &'a Snapshot, pos: usize) -> Option<&'a FindingRow> {
        self.visible.get(pos).and_then(|i| data.findings.get(*i))
    }

    /// 切換命中的選取狀態。
    pub fn toggle(&mut self, id: i64) {
        if !self.selected.remove(&id) {
            self.selected.insert(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use cvewatch_core::matching::dashboard::{Dashboard, DataMarker};
    use cvewatch_core::matching::engine::Confidence;
    use cvewatch_core::matching::scoring::{Priority, ScoreInputs};
    use cvewatch_core::matching::triage::Status;

    fn row(id: i64, cve: &str) -> FindingRow {
        FindingRow {
            id,
            asset_tag: "PA-460".into(),
            asset_status: "active".into(),
            component: "paloalto-panos".into(),
            version: "11.1.2".into(),
            cve_id: cve.into(),
            title: None,
            priority: Priority::P2,
            confidence: Confidence::Confirmed,
            decided_by: "vendor".into(),
            status: Status::Open,
            still_vulnerable: false,
            scores: ScoreInputs::default(),
            criticality: 3,
            component_removed: false,
            manual: false,
            asset_name: "防火牆".into(),
            description: None,
            first_seen: "2026-11-01T00:00:00+00:00".into(),
        }
    }

    fn snapshot(rows: Vec<FindingRow>) -> Snapshot {
        Snapshot {
            dashboard: Dashboard::default(),
            search_keys: rows.iter().map(filter::search_key).collect(),
            findings: rows,
            assets: Vec::new(),
            unresolved: Vec::new(),
            pending: Vec::new(),
            sync_runs: Vec::new(),
            marker: DataMarker::default(),
            loaded_at: Utc::now(),
        }
    }

    /// 記錄最後一次通知的可見列數。
    #[derive(Default)]
    struct Len(Option<usize>);

    impl ListView for Len {
        fn visible_changed(&mut self, len: usize) {
            self.0 = Some(len);
        }
    }

    #[test]
    fn hidden_selection_is_kept_but_not_applied() {
        let data = snapshot(vec![
            row(1, "CVE-2026-0001"),
            row(2, "CVE-2026-0002"),
            row(3, "CVE-2026-0003"),
        ]);
        let mut m = FindingsModel::<Len>::default();
        m.refresh_visible(&data);
        assert_eq!(m.view.0, Some(3));
        m.toggle(1);
        m.toggle(3);
        m.search = "0003".into();
        m.refresh_visible(&data);
        assert_eq!(m.view.0, Some(1));
        assert_eq!(m.visible_selection(&data), [3], "被搜尋隱藏的 #1 不套用");
        m.search.clear();
        m.refresh_visible(&data);
        assert_eq!(m.visible_selection(&data), [1, 3], "放寬搜尋後仍保持選取");
    }

    /// 前端的 `view` 型別經 `Deref` 以 `model.<欄位>` 存取（TUI：`cursor`、`search_input`）。
    /// 這裡完整解構模型：新增欄位時無法編譯，提醒確認沒有和前端 `view` 同名的欄位
    /// （同名時 `Deref` 的路徑會靜默改指向模型）；TUI 端另有同樣的檢查。
    #[test]
    fn model_fields_are_reviewed_against_view_names() {
        let FindingsModel {
            filter: _,
            search: _,
            sort: _,
            selected: _,
            visible: _,
            view: (),
        } = FindingsModel::<()>::default();
    }

    #[test]
    fn stale_visible_indices_do_not_panic() {
        let mut m = FindingsModel::<()>::default();
        m.refresh_visible(&snapshot(vec![
            row(1, "CVE-2026-0001"),
            row(2, "CVE-2026-0002"),
        ]));
        m.toggle(2);
        let smaller = snapshot(vec![row(1, "CVE-2026-0001")]);
        assert_eq!(m.visible_ids(&smaller), [1]);
        assert!(m.visible_selection(&smaller).is_empty());
        assert!(m.row_at(&smaller, 1).is_none());
    }

    #[test]
    fn prune_removes_findings_gone_after_reload() {
        let mut m = FindingsModel::<()>::default();
        m.toggle(1);
        m.toggle(2);
        m.toggle(2);
        m.toggle(3);
        m.prune_selection(&snapshot(vec![row(3, "CVE-2026-0003")]));
        assert_eq!(m.selected.into_iter().collect::<Vec<_>>(), [3]);
    }
}
