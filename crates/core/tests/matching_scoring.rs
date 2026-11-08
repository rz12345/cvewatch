//! 04 任務 4.3：risk-scoring 各等級案例、曝露上調、同級排序與門檻變更後重算。

mod common;

use std::cmp::Ordering;

use common::matching::{panos, world};
use cvewatch_core::assets::model::Exposure;
use cvewatch_core::matching::query::{self, FindingFilter};
use cvewatch_core::matching::scoring::{
    base_priority, priority, rank_cmp, Priority, RankKey, ScoreInputs, ScoringConfig,
};
use serde_json::json;

fn inputs(kev: Option<&str>, epss: Option<f64>, cvss: Option<f64>) -> ScoreInputs {
    ScoreInputs {
        kev_date: kev.map(str::to_string),
        epss,
        cvss,
    }
}

fn base(kev: Option<&str>, epss: Option<f64>, cvss: Option<f64>) -> Priority {
    base_priority(&inputs(kev, epss, cvss), &ScoringConfig::default())
}

// ── Requirement: Priority tiers ─────────────────────────────

#[test]
fn kev_listed_with_low_cvss_is_p1() {
    assert_eq!(base(Some("2026-11-01"), None, Some(5.3)), Priority::P1);
}

#[test]
fn high_epss_without_kev_is_p2() {
    assert_eq!(base(None, Some(0.35), Some(6.1)), Priority::P2);
}

#[test]
fn unscored_cve_is_p4() {
    assert_eq!(base(None, None, None), Priority::P4);
}

#[test]
fn tier_boundaries_with_defaults() {
    assert_eq!(base(None, Some(0.10), None), Priority::P2);
    assert_eq!(base(None, Some(0.099), None), Priority::P3);
    assert_eq!(base(None, None, Some(9.0)), Priority::P2);
    assert_eq!(base(None, None, Some(8.9)), Priority::P3);
    assert_eq!(base(None, Some(0.01), None), Priority::P3);
    assert_eq!(base(None, None, Some(7.0)), Priority::P3);
    assert_eq!(base(None, Some(0.009), Some(6.9)), Priority::P4);
    // 任一條件達門檻即可
    assert_eq!(base(None, Some(0.001), Some(9.8)), Priority::P2);
}

// ── Requirement: Exposure adjustment ────────────────────────

#[test]
fn internet_facing_firewall_is_bumped() {
    let cfg = ScoringConfig::default();
    let p3 = inputs(None, Some(0.02), None);
    assert_eq!(priority(&p3, Exposure::Internet, &cfg), Priority::P2);
    assert_eq!(priority(&p3, Exposure::Dmz, &cfg), Priority::P2);
    assert_eq!(priority(&p3, Exposure::Internal, &cfg), Priority::P3);
    assert_eq!(priority(&p3, Exposure::Mgmt, &cfg), Priority::P3);
    let kev = inputs(Some("2026-11-01"), None, None);
    assert_eq!(priority(&kev, Exposure::Internet, &cfg), Priority::P1);
    assert_eq!(
        priority(&inputs(None, None, None), Exposure::Internet, &cfg),
        Priority::P3
    );
}

#[test]
fn exposure_bump_list_is_configurable() {
    let cfg = ScoringConfig {
        exposure_bump: vec![Exposure::Mgmt],
        ..ScoringConfig::default()
    };
    let p3 = inputs(None, Some(0.02), None);
    assert_eq!(priority(&p3, Exposure::Mgmt, &cfg), Priority::P2);
    assert_eq!(priority(&p3, Exposure::Internet, &cfg), Priority::P3);
}

// ── Requirement: Ordering within a tier ─────────────────────

fn key(kev: Option<&str>, epss: Option<f64>, cvss: Option<f64>, asset_value: u8) -> RankKey {
    RankKey {
        priority: Priority::P2,
        inputs: inputs(kev, epss, cvss),
        asset_value,
    }
}

#[test]
fn two_p2_findings_order_by_epss() {
    let a = key(None, Some(0.4), None, 2);
    let b = key(None, Some(0.2), None, 2);
    assert_eq!(rank_cmp(&a, &b), Ordering::Less);
    assert_eq!(rank_cmp(&b, &a), Ordering::Greater);
}

#[test]
fn ordering_keys_in_sequence() {
    // KEV 加入日期新到舊，不在 KEV 的排後
    assert_eq!(
        rank_cmp(
            &key(Some("2026-11-02"), None, None, 1),
            &key(Some("2026-10-01"), Some(0.9), None, 3)
        ),
        Ordering::Less
    );
    assert_eq!(
        rank_cmp(
            &key(Some("2026-01-01"), None, None, 1),
            &key(None, Some(0.9), None, 3)
        ),
        Ordering::Less
    );
    // EPSS 相同 → CVSS 高者在前；無分數排後
    assert_eq!(
        rank_cmp(
            &key(None, Some(0.2), Some(9.8), 1),
            &key(None, Some(0.2), Some(7.5), 3)
        ),
        Ordering::Less
    );
    assert_eq!(
        rank_cmp(
            &key(None, Some(0.2), None, 3),
            &key(None, Some(0.2), Some(1.0), 1)
        ),
        Ordering::Greater
    );
    // 其餘相同 → 資產價值高者在前
    assert_eq!(
        rank_cmp(&key(None, None, None, 3), &key(None, None, None, 1)),
        Ordering::Less
    );
    // 等級優先於一切
    let p1 = RankKey {
        priority: Priority::P1,
        ..key(None, None, None, 1)
    };
    assert_eq!(
        rank_cmp(&p1, &key(Some("2026-11-02"), Some(1.0), Some(10.0), 3)),
        Ordering::Less
    );
}

// ── Requirement: Configurable thresholds（經由比對重算）────

const CVE_A: &str = "CVE-2099-30001";
const CVE_B: &str = "CVE-2099-30002";

#[test]
fn lowering_epss_high_recomputes_on_match() {
    let mut w = world();
    let c = w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.4")])[0];
    w.cve(
        CVE_A,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.epss(CVE_A, 0.07);
    w.run();
    assert_eq!(w.finding(c, CVE_A).unwrap().priority, "P3");
    w.settings.scoring.epss_high = 0.05;
    w.run();
    assert_eq!(w.finding(c, CVE_A).unwrap().priority, "P2");
}

#[test]
fn new_kev_listing_raises_priority_on_rematch() {
    let mut w = world();
    let c = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4")])[0];
    w.cve(
        CVE_A,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.run();
    assert_eq!(
        w.finding(c, CVE_A).unwrap().priority,
        "P3",
        "未評分 P4 + internet 上調"
    );
    w.kev(CVE_A, "2026-11-02");
    w.run();
    assert_eq!(w.finding(c, CVE_A).unwrap().priority, "P1");
}

#[test]
fn finding_list_uses_risk_order() {
    let mut w = world();
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.4")]);
    w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.3")]);
    w.set_asset_value("FW-02", 3);
    for cve in [CVE_A, CVE_B] {
        w.cve(
            cve,
            panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
            json!([]),
        );
    }
    w.epss(CVE_A, 0.2);
    w.epss(CVE_B, 0.4);
    w.cvss(CVE_A, 9.8);
    w.run();
    let rows = query::list(&w.conn, &FindingFilter::default()).unwrap();
    let order: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.cve_id.clone(), r.asset_tag.clone()))
        .collect();
    assert_eq!(
        order,
        vec![
            (CVE_B.to_string(), "FW-02".to_string()),
            (CVE_B.to_string(), "FW-01".to_string()),
            (CVE_A.to_string(), "FW-02".to_string()),
            (CVE_A.to_string(), "FW-01".to_string()),
        ]
    );
    assert_eq!(rows[2].scores.cvss, Some(9.8));
}

/// Scenario: Same scores on different assets — 同一個 CVE 在 FW-01（資產價值 4）與
/// PC-01（資產價值 3）各有一筆 P2 命中，KEV、EPSS、CVSS 都相同時 FW-01 排在前面。
#[test]
fn same_scores_on_different_assets_order_by_asset_value() {
    let mut w = world();
    w.asset("PC-01", "internal", &[("paloalto-panos", "11.1.4")]);
    w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.3")]);
    w.set_asset_value("PC-01", 3);
    w.set_asset_value("FW-01", 4);
    w.cve(
        CVE_A,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.epss(CVE_A, 0.4);
    w.run();
    let rows = query::list(&w.conn, &FindingFilter::default()).unwrap();
    let order: Vec<(&str, Priority)> = rows
        .iter()
        .map(|r| (r.asset_tag.as_str(), r.priority))
        .collect();
    assert_eq!(
        order,
        vec![("FW-01", Priority::P2), ("PC-01", Priority::P2)]
    );
    assert_eq!(rows[0].asset_value, 4);
    assert_eq!(rows[1].asset_value, 3);
}
