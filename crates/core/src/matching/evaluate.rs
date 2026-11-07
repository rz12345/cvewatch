//! 對每個（active 元件 × CVE）收集各來源證據並彙整判定。

use std::collections::{BTreeSet, HashMap};

use super::engine::{decide, Decision};
use super::evidence::{cna, csaf, nvd, osv, ComponentVersion, Evidence, ProductIds, Source};
use super::load::{ActiveComponent, CveInfo, MatchData, Target};
use super::version::Schemes;
use crate::catalog::Catalog;
use crate::ingest::cve5::CveState;

/// 一組（元件 × CVE）的判定。
#[derive(Debug, Clone, PartialEq)]
pub struct PairDecision {
    pub component_id: i64,
    pub cve_id: String,
    pub decision: Decision,
}

/// 產品識別與 scheme；目錄中已移除的產品或未知 scheme 不參與比對。
pub struct Products<'a> {
    ids: HashMap<String, ProductIds>,
    schemes: &'a Schemes,
}

impl<'a> Products<'a> {
    pub fn new(catalog: &Catalog, schemes: &'a Schemes) -> Self {
        let ids = catalog
            .products()
            .iter()
            .map(|p| (p.key.clone(), ProductIds::from_product(p)))
            .collect();
        Products { ids, schemes }
    }

    /// 已發布的 CVE 中提到此產品者（CNA／ADP affected、NVD vulnerable CPE、原廠 CSAF）。
    fn mentions(&self, data: &MatchData, key: &str, cve_id: &str) -> bool {
        let Some(ids) = self.ids.get(key) else {
            return false;
        };
        let by_affected = data
            .affected
            .get(cve_id)
            .is_some_and(|es| es.iter().any(|e| e.matches(ids)));
        let by_nvd = data
            .nvd
            .get(cve_id)
            .is_some_and(|rows| rows.iter().any(|r| r.is_vulnerable_for(ids)));
        let by_csaf = data.csaf.get(cve_id).is_some_and(|m| m.contains_key(key));
        by_affected || by_nvd || by_csaf
    }

    /// 產品元件的版本（以產品的 scheme 解析一次）；產品不在目錄或沒有比較器時為 None。
    pub fn version_of<'c>(
        &'c self,
        component: &'c ActiveComponent,
    ) -> Option<ComponentVersion<'c>> {
        let ids = self.ids.get(component.product_key()?)?;
        let scheme = self.schemes.get(&ids.scheme)?;
        Some(ComponentVersion::new(scheme, &component.version_raw))
    }

    /// 目錄中有此產品且其版本格式有比較器。
    pub fn knows(&self, key: &str) -> bool {
        self.ids
            .get(key)
            .is_some_and(|ids| self.schemes.get(&ids.scheme).is_some())
    }
}

/// 產品元件 × 單一 CVE。沒有任何來源提到此產品時為 None。
pub fn evaluate_product(
    data: &MatchData,
    products: &Products<'_>,
    component: &ActiveComponent,
    cve_id: &str,
) -> Option<Decision> {
    let version = products.version_of(component)?;
    evaluate_with(data, products, component, &version, cve_id)
}

/// 已解析元件版本的產品元件 × 單一 CVE。
fn evaluate_with(
    data: &MatchData,
    products: &Products<'_>,
    component: &ActiveComponent,
    version: &ComponentVersion<'_>,
    cve_id: &str,
) -> Option<Decision> {
    let key = component.product_key()?;
    let ids = products.ids.get(key)?;
    let mut evidence: Vec<Evidence> = Vec::new();
    if let Some(rows) = data.csaf.get(cve_id).and_then(|m| m.get(key)) {
        let fixes = data
            .csaf_fix
            .get(cve_id)
            .and_then(|m| m.get(key))
            .map_or(&[][..], Vec::as_slice);
        evidence.extend(csaf::evaluate(rows, fixes, version));
    }
    let entries = data.affected.get(cve_id).map_or(&[][..], Vec::as_slice);
    for source in [Source::Cna, Source::Adp] {
        let matched: Vec<&cna::AffectedEntry> = entries
            .iter()
            .filter(|e| e.source == source && e.matches(ids))
            .collect();
        evidence.extend(cna::evaluate(source, &matched, version));
    }
    if let Some(rows) = data.nvd.get(cve_id) {
        evidence.extend(nvd::evaluate(rows, ids, version));
    }
    decide(evidence)
}

fn evaluate_package(
    data: &MatchData,
    component: &ActiveComponent,
    cve_id: &str,
) -> Option<Decision> {
    let hits = data.osv.get(&component.id).map_or(&[][..], Vec::as_slice);
    decide(vec![osv::evaluate(cve_id, hits)])
}

/// 全部判定。`existing` 為既有命中（套件元件即使 OSV 不再列出也要重新判定）。
pub fn evaluate_all(
    data: &MatchData,
    products: &Products<'_>,
    existing: &BTreeSet<(i64, String)>,
    rejected: &BTreeSet<String>,
) -> Vec<PairDecision> {
    let by_product = cves_by_product(data, products);
    let mut out = Vec::new();
    for component in &data.components {
        let decided: Vec<(&str, Option<Decision>)> = match &component.target {
            Target::Product(key) => match products.version_of(component) {
                // 元件版本每次比對只解析一次
                Some(version) => by_product
                    .get(key.as_str())
                    .into_iter()
                    .flatten()
                    .map(|cve| {
                        let decision = evaluate_with(data, products, component, &version, &cve.id);
                        (cve.id.as_str(), decision)
                    })
                    .collect(),
                None => Vec::new(),
            },
            Target::Package(_) => package_cves(data, component, existing)
                .into_iter()
                .filter(|c| !rejected.contains(*c))
                .map(|cve| (cve, evaluate_package(data, component, cve)))
                .collect(),
        };
        out.extend(decided.into_iter().filter_map(|(cve_id, decision)| {
            decision.map(|decision| PairDecision {
                component_id: component.id,
                cve_id: cve_id.to_string(),
                decision,
            })
        }));
    }
    out
}

/// 依產品索引：每個使用中的產品只看提到它的已發布 CVE（多個元件同產品時不重複掃描）。
fn cves_by_product<'a>(
    data: &'a MatchData,
    products: &Products<'_>,
) -> HashMap<&'a str, Vec<&'a CveInfo>> {
    let keys: BTreeSet<&str> = data
        .components
        .iter()
        .filter_map(|c| c.product_key())
        .collect();
    keys.into_iter()
        .map(|key| {
            let cves = data
                .cves
                .iter()
                .filter(|c| c.state == CveState::Published && products.mentions(data, key, &c.id))
                .collect();
            (key, cves)
        })
        .collect()
}

/// 套件元件要判定的 CVE：OSV 命中的 CVE，加上既有命中（OSV 不再列出時要判定為不受影響）。
fn package_cves<'a>(
    data: &'a MatchData,
    component: &ActiveComponent,
    existing: &'a BTreeSet<(i64, String)>,
) -> BTreeSet<&'a str> {
    let mut cves: BTreeSet<&str> = data
        .osv
        .get(&component.id)
        .into_iter()
        .flatten()
        .flat_map(|h| h.cve_ids.iter().map(String::as_str))
        .collect();
    // 既有命中依 (component_id, cve_id) 排序，只取此元件的範圍
    let own = (component.id, String::new())..(component.id + 1, String::new());
    cves.extend(existing.range(own).map(|(_, cve)| cve.as_str()));
    cves
}
