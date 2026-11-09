//! 18 任務 2.1～2.3：版本待補的元件不參與比對、既有命中保留、Dashboard 的待補計數與來源矛盾。

mod common;

use chrono::Utc;
use common::matching::{affected, panos, world, World, T0};
use cvewatch_core::assets::{pending_counts, PendingCounts};
use cvewatch_core::matching::dashboard;
use cvewatch_core::matching::query::conflicts;
use rusqlite::params;
use serde_json::json;

const CVE: &str = "CVE-2099-10001";
const LATER: &str = "2026-11-02T00:00:00+00:00";
const LATEST: &str = "2026-11-03T00:00:00+00:00";

fn last_evaluated(w: &World, finding_id: i64) -> String {
    w.conn
        .query_row(
            "SELECT last_evaluated FROM finding WHERE id = ?1",
            [finding_id],
            |r| r.get(0),
        )
        .unwrap()
}

fn dsm_cve(w: &World, id: &str, versions: serde_json::Value) {
    w.cve(
        id,
        json!([affected(
            "Synology",
            "DiskStation Manager (DSM)",
            None,
            versions
        )]),
        json!([]),
    );
}

#[test]
fn pending_component_is_skipped() {
    let mut w = world();
    let c = w.asset("NET-HW-003", "internal", &[("synology-dsm", "")])[0];
    for i in 0..4 {
        dsm_cve(
            &w,
            &format!("CVE-2099-2000{i}"),
            json!([{"version": "7.0", "lessThan": "7.2.2-72806-3", "status": "affected"}]),
        );
    }
    // 沒有版本資料的 CVE：一般元件會產生 probable 命中
    dsm_cve(
        &w,
        "CVE-2099-20009",
        json!([{"version": "n/a", "status": "affected"}]),
    );
    let report = w.run();
    assert_eq!(report.components, 0);
    let findings: i64 = w
        .conn
        .query_row(
            "SELECT count(*) FROM finding WHERE component_id = ?1",
            [c],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(findings, 0, "版本待補的元件沒有任何命中（含 probable）");
}

#[test]
fn known_version_becomes_pending_then_filled() {
    let mut w = world();
    let c = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4")])[0];
    w.cve(
        CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.run();
    let open = w.finding(c, CVE).expect("命中");
    assert_eq!(open.status, "open");

    w.set_version(c, "", LATER);
    let report = w.run_at(LATER);
    assert_eq!(report.components, 0);
    let kept = w.finding(c, CVE).expect("保留");
    assert_eq!((kept.status.as_str(), kept.in_range), ("open", true));
    assert_eq!(last_evaluated(&w, kept.id), T0, "待補期間不更新");

    w.set_version(c, "11.1.4", LATEST);
    w.run_at(LATEST);
    let again = w.finding(c, CVE).expect("沿用原命中");
    assert_eq!(again.id, open.id);
    assert_eq!(again.status, "open");
    assert_eq!(last_evaluated(&w, again.id), LATEST);
}

/// code review：版本待補期間，只和該產品有關的等待區 CVE 不會被當成不再相關而移出。
#[test]
fn waiting_cve_stays_while_the_version_is_pending() {
    let mut w = world();
    let c = w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.2-72806")])[0];
    dsm_cve(&w, CVE, json!([{"version": "n/a", "status": "affected"}]));
    w.run();
    let (reason, _, resolved) = w.pending(CVE).expect("進入等待區");
    assert_eq!((reason.as_str(), resolved), ("no_version_data", None));

    w.set_version(c, "", LATER);
    w.run_at(LATER);
    let (_, _, resolved) = w.pending(CVE).expect("仍在等待區");
    assert_eq!(resolved, None, "待補期間不移出");
}

#[test]
fn pending_counts_cover_active_catalog_components_only() {
    let w = world();
    w.asset(
        "NET-HW-003",
        "internal",
        &[("synology-dsm", ""), ("dell-idrac9", "")],
    );
    w.asset("NET-HW-004", "internal", &[("paloalto-panos", "")]);
    w.asset("NAS-01", "internal", &[("synology-dsm", "7.2")]);
    w.asset("NAS-02", "internal", &[("synology-dsm", "")]);
    w.retire("NAS-02");
    // 版本待補的未辨識產品不計入
    w.conn
        .execute(
            "INSERT INTO asset_unresolved (asset_id, product_raw, version_raw, seen_at)
             VALUES ((SELECT id FROM asset WHERE asset_tag = 'NAS-01'), 'Zyxel ABCD', '', ?1)",
            params![T0],
        )
        .unwrap();
    let counts = pending_counts(&w.conn).unwrap();
    assert_eq!(
        counts,
        PendingCounts {
            components: 3,
            assets: 2
        }
    );
    assert_eq!(
        counts.summary().as_deref(),
        Some("版本待補 3 個元件（2 個資產），未納入比對")
    );
    let d = dashboard::load(&w.conn, 14, Utc::now()).unwrap();
    assert_eq!(d.pending_versions, counts);
}

#[test]
fn conflicts_of_pending_components_are_not_counted() {
    let w = world();
    let pending = w.asset("NET-HW-003", "internal", &[("synology-dsm", "")])[0];
    let known = w.asset("NAS-01", "internal", &[("synology-dsm", "7.2")])[0];
    for c in [pending, known] {
        w.conn
            .execute(
                "INSERT INTO match_conflict (component_id, cve_id, final_source, final_verdict,
                                             other_source, other_verdict, at)
                 VALUES (?1, ?2, 'cna', 'not_affected', 'nvd', 'affected', ?3)",
                params![c, CVE, T0],
            )
            .unwrap();
    }
    let d = dashboard::load(&w.conn, 14, Utc::now()).unwrap();
    assert_eq!(d.conflicts, 1);
    let listed = conflicts(&w.conn).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].asset_tag, "NAS-01");
}
