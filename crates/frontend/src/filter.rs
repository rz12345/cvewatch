//! Findings 的篩選、即時搜尋與排序（在記憶體中進行；不改變載入的資料），以及資產清單的類別篩選選項。

use std::cmp::Ordering;

use cvewatch_core::assets::{AssetClasses, ClassFilter, MajorClass};
use cvewatch_core::matching::engine::Confidence;
use cvewatch_core::matching::query::FindingRow;
use cvewatch_core::matching::scoring::Priority;
use cvewatch_core::matching::triage::Status;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KevFilter {
    #[default]
    All,
    Only,
    Exclude,
}

impl KevFilter {
    pub fn next(self) -> Self {
        match self {
            KevFilter::All => KevFilter::Only,
            KevFilter::Only => KevFilter::Exclude,
            KevFilter::Exclude => KevFilter::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            KevFilter::All => "全部",
            KevFilter::Only => "只看 KEV",
            KevFilter::Exclude => "排除 KEV",
        }
    }
}

/// 篩選條件；空的清單表示不限。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub statuses: Vec<Status>,
    pub priorities: Vec<Priority>,
    pub asset: Option<String>,
    pub confidence: Option<Confidence>,
    pub kev: KevFilter,
}

impl Default for Filter {
    /// 預設只看未處理。
    fn default() -> Self {
        Filter {
            statuses: vec![Status::Open],
            priorities: Vec::new(),
            asset: None,
            confidence: None,
            kev: KevFilter::All,
        }
    }
}

fn toggled<T: Copy + PartialEq>(items: &[T], value: T, order: &[T]) -> Vec<T> {
    let present = items.contains(&value);
    order
        .iter()
        .copied()
        .filter(|x| {
            if *x == value {
                !present
            } else {
                items.contains(x)
            }
        })
        .collect()
}

impl Filter {
    pub fn with_status_toggled(&self, s: Status) -> Filter {
        Filter {
            statuses: toggled(&self.statuses, s, Status::ALL),
            ..self.clone()
        }
    }

    pub fn with_priority_toggled(&self, p: Priority) -> Filter {
        Filter {
            priorities: toggled(&self.priorities, p, Priority::ALL),
            ..self.clone()
        }
    }

    pub fn with_asset(&self, asset: Option<String>) -> Filter {
        Filter {
            asset,
            ..self.clone()
        }
    }

    pub fn with_confidence(&self, confidence: Option<Confidence>) -> Filter {
        Filter {
            confidence,
            ..self.clone()
        }
    }

    pub fn with_kev(&self, kev: KevFilter) -> Filter {
        Filter {
            kev,
            ..self.clone()
        }
    }

    pub fn matches(&self, r: &FindingRow) -> bool {
        let in_list = |empty: bool, hit: bool| empty || hit;
        in_list(self.statuses.is_empty(), self.statuses.contains(&r.status))
            && in_list(
                self.priorities.is_empty(),
                self.priorities.contains(&r.priority),
            )
            && self
                .asset
                .as_ref()
                .is_none_or(|a| a.eq_ignore_ascii_case(&r.asset_tag))
            && self.confidence.is_none_or(|c| c == r.confidence)
            && match self.kev {
                KevFilter::All => true,
                KevFilter::Only => r.scores.kev_date.is_some(),
                KevFilter::Exclude => r.scores.kev_date.is_none(),
            }
    }

    /// 一行摘要，例如「狀態 未處理｜等級 全部｜資產 全部｜信心 全部｜KEV 全部」。
    pub fn summary(&self) -> String {
        let list = |items: Vec<&str>| {
            if items.is_empty() {
                "全部".to_string()
            } else {
                items.join("/")
            }
        };
        format!(
            "狀態 {}｜等級 {}｜資產 {}｜信心 {}｜KEV {}",
            list(self.statuses.iter().map(|s| s.label()).collect()),
            list(self.priorities.iter().map(|p| p.as_str()).collect()),
            self.asset.as_deref().unwrap_or("全部"),
            self.confidence.map_or("全部", |c| c.as_str()),
            self.kev.label()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// 風險（P1→P4 及同級排序規則，與 CLI 相同）
    #[default]
    Risk,
    Cvss,
    Epss,
    FirstSeen,
    Asset,
}

impl SortKey {
    pub fn next(self) -> Self {
        match self {
            SortKey::Risk => SortKey::Cvss,
            SortKey::Cvss => SortKey::Epss,
            SortKey::Epss => SortKey::FirstSeen,
            SortKey::FirstSeen => SortKey::Asset,
            SortKey::Asset => SortKey::Risk,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Risk => "風險",
            SortKey::Cvss => "CVSS 高→低",
            SortKey::Epss => "EPSS 高→低",
            SortKey::FirstSeen => "首次發現 新→舊",
            SortKey::Asset => "資產",
        }
    }
}

/// 搜尋鍵的欄位分隔（資料中的控制字元已正規化為空白，不會出現）。
const FIELD_SEPARATOR: char = '\u{1f}';

/// 小寫並把連續空白（含換行、tab）合併成一個空格：跨越換行的片語也能搜尋。
fn normalize(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// 搜尋用鍵：CVE ID、資產編號／名稱、標題與描述（正規化後）以分隔字元串接（載入時建立一次）。
pub fn search_key(r: &FindingRow) -> String {
    [
        Some(r.cve_id.as_str()),
        Some(r.asset_tag.as_str()),
        Some(r.asset_name.as_str()),
        r.title.as_deref(),
        r.description.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(normalize)
    .collect::<Vec<_>>()
    .join(&FIELD_SEPARATOR.to_string())
}

/// 不分大小寫比對（`key` 為 [`search_key`]）；查詢不跨欄位比對。
pub fn matches_search(key: &str, query: &str) -> bool {
    let q = normalize(query);
    q.is_empty() || key.split(FIELD_SEPARATOR).any(|field| field.contains(&q))
}

/// 分數高到低，沒有分數的排最後。
fn desc_score(a: Option<f64>, b: Option<f64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => y.total_cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// 符合條件的列索引（`rows` 已依風險排序；其他排序的同值以風險順序排列）。
/// `keys[i]` 為 `rows[i]` 的 [`search_key`]。
pub fn visible(
    rows: &[FindingRow],
    keys: &[String],
    filter: &Filter,
    search: &str,
    sort: SortKey,
) -> Vec<usize> {
    let mut out: Vec<usize> = rows
        .iter()
        .zip(keys)
        .enumerate()
        .filter(|(_, (r, key))| filter.matches(r) && matches_search(key, search))
        .map(|(i, _)| i)
        .collect();
    let by = |a: &usize, b: &usize| {
        let (x, y) = (&rows[*a], &rows[*b]);
        match sort {
            SortKey::Risk => Ordering::Equal,
            SortKey::Cvss => desc_score(x.scores.cvss, y.scores.cvss),
            SortKey::Epss => desc_score(x.scores.epss, y.scores.epss),
            SortKey::FirstSeen => y.first_seen.cmp(&x.first_seen),
            SortKey::Asset => x.asset_tag.to_lowercase().cmp(&y.asset_tag.to_lowercase()),
        }
        .then(a.cmp(b))
    };
    out.sort_by(by);
    out
}

/// 資產清單的類別篩選選項：全部、兩個大類（「實體（HW）」「軟體（SW）」），再依代碼列出各子類別
/// （「HW01 伺服器」）。
pub fn class_filter_choices(classes: &AssetClasses) -> Vec<(Option<ClassFilter>, String)> {
    let majors = MajorClass::ALL.into_iter().map(|m| {
        (
            Some(ClassFilter::Major(m)),
            format!("{}（{}）", m.label(), m.as_str()),
        )
    });
    let codes = classes
        .all()
        .iter()
        .map(|c| (Some(ClassFilter::Code(c.code.clone())), c.label()));
    std::iter::once((None, "全部類別".to_string()))
        .chain(majors)
        .chain(codes)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_filter_choices_list_majors_then_codes() {
        let classes = AssetClasses::builtin();
        let choices = class_filter_choices(&classes);
        let labels: Vec<&str> = choices.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(
            labels[..4],
            ["全部類別", "實體（HW）", "軟體（SW）", "HW01 伺服器"]
        );
        assert_eq!(choices.len(), 3 + classes.all().len());
        let hw = choices[1].0.clone().unwrap();
        assert!(hw.matches(&cvewatch_core::assets::ClassCode::parse("HW05").unwrap()));
        assert!(!hw.matches(&cvewatch_core::assets::ClassCode::parse("SW02").unwrap()));
    }
    use cvewatch_core::matching::scoring::ScoreInputs;

    fn row(id: i64, cve: &str, asset: &str, status: Status, priority: Priority) -> FindingRow {
        FindingRow {
            id,
            asset_tag: asset.into(),
            asset_status: "active".into(),
            component: "paloalto-panos".into(),
            version: "11.1.2".into(),
            cve_id: cve.into(),
            title: Some("GlobalProtect command injection".into()),
            priority,
            confidence: Confidence::Confirmed,
            decided_by: "vendor".into(),
            status,
            still_vulnerable: false,
            scores: ScoreInputs::default(),
            asset_value: 3,
            component_removed: false,
            manual: false,
            asset_name: format!("{asset} 防火牆"),
            description: Some("A command injection in PAN-OS".into()),
            first_seen: format!("2026-11-0{id}T00:00:00+00:00"),
        }
    }

    fn rows() -> Vec<FindingRow> {
        let mut kev = row(1, "CVE-2026-0001", "PA-460", Status::Open, Priority::P1);
        kev.scores.kev_date = Some("2026-10-01".into());
        kev.scores.cvss = Some(9.8);
        let mut probable = row(2, "CVE-2025-1234", "PVE-01", Status::Open, Priority::P3);
        probable.confidence = Confidence::Probable;
        probable.scores.epss = Some(0.5);
        probable.title = Some("Proxmox VE flaw".into());
        probable.description = Some("Proxmox VE privilege escalation".into());
        let resolved = row(3, "CVE-2026-0002", "PA-460", Status::Resolved, Priority::P2);
        let ignored = row(4, "CVE-2024-3400", "R6515", Status::Ignored, Priority::P4);
        vec![kev, probable, resolved, ignored]
    }

    fn vis(rows: &[FindingRow], f: &Filter, search: &str, sort: SortKey) -> Vec<usize> {
        let keys: Vec<String> = rows.iter().map(search_key).collect();
        visible(rows, &keys, f, search, sort)
    }

    fn ids(rows: &[FindingRow], idx: &[usize]) -> Vec<i64> {
        idx.iter().map(|i| rows[*i].id).collect()
    }

    #[test]
    fn default_filter_shows_only_open() {
        let r = rows();
        let v = vis(&r, &Filter::default(), "", SortKey::Risk);
        assert_eq!(ids(&r, &v), [1, 2]);
    }

    #[test]
    fn search_matches_cve_asset_and_description() {
        let r = rows();
        let all = Filter::default().with_status_toggled(Status::Open);
        assert!(all.statuses.is_empty(), "取消唯一的狀態後為不限");
        assert_eq!(ids(&r, &vis(&r, &all, "2026-0", SortKey::Risk)), [1, 3]);
        assert_eq!(ids(&r, &vis(&r, &all, "pve-01", SortKey::Risk)), [2]);
        assert_eq!(
            ids(&r, &vis(&r, &all, "防火牆", SortKey::Risk)),
            [1, 2, 3, 4]
        );
        assert_eq!(ids(&r, &vis(&r, &all, "ESCALATION", SortKey::Risk)), [2]);
        assert_eq!(ids(&r, &vis(&r, &all, "  ", SortKey::Risk)).len(), 4);
    }

    #[test]
    fn filters_combine() {
        let r = rows();
        let f = Filter::default()
            .with_status_toggled(Status::Resolved)
            .with_asset(Some("pa-460".into()));
        assert_eq!(f.statuses, [Status::Open, Status::Resolved]);
        assert_eq!(ids(&r, &vis(&r, &f, "", SortKey::Risk)), [1, 3]);
        let kev = f.with_kev(KevFilter::Only);
        assert_eq!(ids(&r, &vis(&r, &kev, "", SortKey::Risk)), [1]);
        let no_kev = f.with_kev(KevFilter::Exclude);
        assert_eq!(ids(&r, &vis(&r, &no_kev, "", SortKey::Risk)), [3]);
        let probable = Filter::default().with_confidence(Some(Confidence::Probable));
        assert_eq!(ids(&r, &vis(&r, &probable, "", SortKey::Risk)), [2]);
        let p1 = Filter::default().with_priority_toggled(Priority::P1);
        assert_eq!(ids(&r, &vis(&r, &p1, "", SortKey::Risk)), [1]);
        assert!(p1.with_priority_toggled(Priority::P1).priorities.is_empty());
    }

    #[test]
    fn toggling_keeps_canonical_order() {
        let f = Filter::default()
            .with_status_toggled(Status::Ignored)
            .with_status_toggled(Status::Resolved);
        assert_eq!(
            f.statuses,
            [Status::Open, Status::Resolved, Status::Ignored]
        );
    }

    #[test]
    fn sort_options() {
        let r = rows();
        let all = Filter::default().with_status_toggled(Status::Open);
        let order = |s| ids(&r, &vis(&r, &all, "", s));
        assert_eq!(order(SortKey::Risk), [1, 2, 3, 4]);
        assert_eq!(order(SortKey::Cvss), [1, 2, 3, 4]);
        assert_eq!(order(SortKey::Epss), [2, 1, 3, 4]);
        assert_eq!(order(SortKey::FirstSeen), [4, 3, 2, 1]);
        assert_eq!(order(SortKey::Asset), [1, 3, 2, 4]);
        assert_eq!(SortKey::Asset.next(), SortKey::Risk);
    }

    #[test]
    fn search_spans_line_breaks_but_not_fields() {
        let mut r = rows().remove(0);
        r.description = Some("allows remote\ncode   execution".into());
        let key = search_key(&r);
        assert!(matches_search(&key, "remote code execution"));
        assert!(matches_search(&key, "  REMOTE   code "));
        // 欄位之間不相連：CVE ID 結尾 + 資產編號開頭不算命中
        assert!(!matches_search(&key, "0001 pa-460"));
    }

    #[test]
    fn summary_lists_conditions() {
        assert_eq!(
            Filter::default().summary(),
            "狀態 未處理｜等級 全部｜資產 全部｜信心 全部｜KEV 全部"
        );
    }
}
