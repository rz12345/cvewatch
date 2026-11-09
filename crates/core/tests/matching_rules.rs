//! 04 任務 2.7：CNA 版本區間、各廠版本比較、信心等級與自動重算的 vuln-matching scenario。

mod common;

use common::matching::{affected, panos, world, T0};
use common::{matching::World, read_ingest};
use cvewatch_core::ingest::cve5;
use cvewatch_core::ingest::relevance::Relevance;
use cvewatch_core::ingest::store::upsert_record;
use serde_json::json;

const CVE: &str = "CVE-2099-10001";

fn fw(w: &World, version: &str) -> i64 {
    w.asset("FW-01", "internet", &[("paloalto-panos", version)])[0]
}

// ── Requirement: CNA affected range evaluation ──────────────

#[test]
fn version_inside_less_than_range_is_affected() {
    let mut w = world();
    let c = fw(&w, "11.1.4-h7");
    w.cve(
        CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.run();
    let f = w.finding(c, CVE).expect("命中");
    assert_eq!(
        (
            f.confidence.as_str(),
            f.decided_by.as_str(),
            f.status.as_str()
        ),
        ("confirmed", "cna", "open")
    );
    let ev = w.evidence(f.id);
    assert_eq!(ev[0].0, "cna");
    assert!(ev[0].2.contains("11.1.0 ≤ v < 11.1.6"), "{}", ev[0].2);
}

#[test]
fn hotfix_listed_in_changes_is_not_affected() {
    let mut w = world();
    let c = fw(&w, "11.1.4-h7");
    w.cve(
        CVE,
        panos(
            json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected",
                      "changes": [{"at": "11.1.4-h4", "status": "unaffected"}]}]),
        ),
        json!([]),
    );
    w.run();
    assert!(w.finding(c, CVE).is_none());
    // 熱修之前仍受影響
    let older = w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.4-h2")])[0];
    w.run();
    assert!(w.finding(older, CVE).is_some());
}

#[test]
fn no_matching_entry_and_no_default_is_unknown() {
    let mut w = world();
    let c = fw(&w, "11.1.4");
    w.cve(
        CVE,
        json!([affected(
            "Palo Alto Networks",
            "PAN-OS",
            None,
            json!([{"version": "10.2.0", "lessThan": "10.2.9", "status": "affected"}])
        )]),
        json!([]),
    );
    w.run();
    let f = w.finding(c, CVE).expect("未知仍建立 probable 命中");
    assert_eq!(f.confidence, "probable");
    let ev = w.evidence(f.id);
    assert_eq!(ev[0].1, "unknown");
    assert!(ev[0].2.contains("未提供 defaultStatus"), "{}", ev[0].2);
    // 有版本資訊但無法判定 → 不進等待區
    assert!(w.pending(CVE).is_none());
}

#[test]
fn default_status_applies_outside_listed_versions() {
    let mut w = world();
    let c = fw(&w, "9.1.3");
    w.cve(
        CVE,
        json!([affected(
            "Palo Alto Networks",
            "PAN-OS",
            Some("affected"),
            json!([{"version": "10.2.0", "lessThan": "10.2.9", "status": "unaffected"}])
        )]),
        json!([]),
    );
    w.run();
    assert_eq!(w.finding(c, CVE).unwrap().confidence, "confirmed");
}

#[test]
fn less_than_or_equal_and_exact_versions() {
    let mut w = world();
    let edge = fw(&w, "11.1.6");
    let exact = w.asset("FW-02", "internal", &[("paloalto-panos", "10.1.14-h9")])[0];
    w.cve(
        CVE,
        panos(json!([
            {"version": "11.1.0", "lessThanOrEqual": "11.1.6", "status": "affected"},
            {"version": "10.1.14-h9", "status": "affected"}
        ])),
        json!([]),
    );
    w.run();
    assert!(w.finding(edge, CVE).is_some());
    assert!(w.finding(exact, CVE).is_some());
}

// ── Requirement: Vendor-specific version comparison ─────────

#[test]
fn unparseable_component_version_is_unknown_not_unaffected() {
    let mut w = world();
    let c = fw(&w, "eleven point one");
    w.cve(
        CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.run();
    let f = w.finding(c, CVE).expect("不可判定為不受影響");
    assert_eq!(f.confidence, "probable");
    assert!(w.evidence(f.id)[0].2.contains("無法以 panos 格式解析"));
}

#[test]
fn esxi_builds_and_update_names() {
    let mut w = world();
    let ids = w.asset("ESX-01", "mgmt", &[("vmware-esxi", "24280767")]);
    let fixed = w.asset("ESX-02", "mgmt", &[("vmware-esxi", "8.0 Update 3c")])[0];
    let unknown_build = w.asset("ESX-03", "mgmt", &[("vmware-esxi", "23999999")])[0];
    let bytes = read_ingest("cvelist/records/CVE-2099-50001.json");
    let record = cve5::parse(&bytes).unwrap();
    upsert_record(&w.conn, &record, &bytes, Relevance::Cna, T0).unwrap();
    w.run();
    assert_eq!(
        w.finding(ids[0], "CVE-2099-50001").unwrap().confidence,
        "confirmed"
    );
    // 不在區間內，fixture 的 defaultStatus 為 unknown → 未知（probable），不是不受影響
    let f = w.finding(fixed, "CVE-2099-50001").unwrap();
    assert_eq!(f.confidence, "probable");
    assert!(w.evidence(f.id)[0].2.contains("defaultStatus = unknown"));
    assert_eq!(
        w.finding(unknown_build, "CVE-2099-50001")
            .unwrap()
            .confidence,
        "probable"
    );
}

#[test]
fn dsm_range_from_cna() {
    let mut w = world();
    let old = w.asset(
        "NAS-01",
        "internal",
        &[("synology-dsm", "7.2.2-72806 Update 1")],
    )[0];
    let new = w.asset(
        "NAS-02",
        "internal",
        &[("synology-dsm", "7.2.2-72806 Update 3")],
    )[0];
    w.cve(
        CVE,
        json!([affected(
            "Synology",
            "DiskStation Manager (DSM)",
            Some("unaffected"),
            json!([{"version": "7.2", "lessThan": "7.2.2-72806-3", "status": "affected"}])
        )]),
        json!([]),
    );
    w.run();
    assert!(w.finding(old, CVE).is_some());
    assert!(w.finding(new, CVE).is_none());
}

// ── Requirement: Confidence levels ──────────────────────────

#[test]
fn product_matches_but_no_version_data_is_probable() {
    let mut w = world();
    let c = w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.2-72806")])[0];
    w.cve(
        CVE,
        json!([affected(
            "Synology",
            "DiskStation Manager (DSM)",
            None,
            json!([{"version": "n/a", "status": "affected"}])
        )]),
        json!([]),
    );
    w.run();
    let f = w.finding(c, CVE).expect("probable 命中");
    assert_eq!(f.confidence, "probable");
    assert_eq!(f.decided_by, "cna");
}

// ── Requirement: Matching runs automatically ────────────────

#[test]
fn upgraded_asset_is_no_longer_affected_after_rematch() {
    let mut w = world();
    let c = fw(&w, "11.1.4");
    w.cve(
        CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    w.run();
    assert!(w.finding(c, CVE).unwrap().in_range);
    w.set_version(c, "11.1.6", "2026-11-02T00:00:00+00:00");
    w.run_at("2026-11-02T00:00:00+00:00");
    let f = w.finding(c, CVE).unwrap();
    assert!(!f.in_range);
    assert_eq!(f.status, "resolved");
}

#[test]
fn retired_asset_findings_are_kept_but_not_updated() {
    let mut w = world();
    let c = fw(&w, "11.1.4");
    w.cve(
        CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}])),
        json!([]),
    );
    let report = w.run();
    assert_eq!(report.components, 1);
    w.retire("FW-01");
    w.set_version(c, "11.1.6", "2026-11-02T00:00:00+00:00");
    let report = w.run_at("2026-11-02T00:00:00+00:00");
    assert_eq!(report.components, 0);
    let f = w.finding(c, CVE).expect("保留");
    assert_eq!((f.status.as_str(), f.in_range), ("open", true));
    let last: String = w
        .conn
        .query_row(
            "SELECT last_evaluated FROM finding WHERE id = ?1",
            [f.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(last, T0);
}

/// code review 第二輪 #1：沒寫 build（或 Update）的 DSM 版本在等值與 ≤ 上界時涵蓋整條版本線。
#[test]
fn dsm_version_without_build_covers_the_version_line() {
    let cases = [
        (json!({"version": "7.2.1", "status": "affected"}), true),
        (
            json!({"version": "7.2", "lessThanOrEqual": "7.2.1", "status": "affected"}),
            true,
        ),
        (
            json!({"version": "7.2", "lessThan": "7.2.2", "status": "affected"}),
            true,
        ),
        (
            json!({"version": "7.2", "lessThan": "7.2.1", "status": "affected"}),
            false,
        ),
        (
            json!({"version": "7.2.1-64570", "status": "affected"}),
            false,
        ),
        (
            json!({"version": "7.2.1-69057", "status": "affected"}),
            true,
        ),
    ];
    for (i, (item, expected)) in cases.into_iter().enumerate() {
        let mut w = world();
        let c = w.asset(
            "NAS-01",
            "internal",
            &[("synology-dsm", "7.2.1-69057 Update 3")],
        )[0];
        w.cve(
            CVE,
            json!([affected(
                "Synology",
                "DiskStation Manager (DSM)",
                Some("unaffected"),
                json!([item])
            )]),
            json!([]),
        );
        w.run();
        assert_eq!(w.finding(c, CVE).is_some(), expected, "案例 {i}：{item}");
    }
    // NVD 只寫 x.y.z 的固定版本
    let mut w = world();
    let c = w.asset(
        "NAS-01",
        "internal",
        &[("synology-dsm", "7.2.1-69057 Update 3")],
    )[0];
    w.nvd(
        CVE,
        &[(
            "cpe:2.3:o:synology:diskstation_manager:7.2.1:*:*:*:*:*:*:*",
            true,
            None,
            None,
        )],
    );
    w.run();
    assert_eq!(w.finding(c, CVE).unwrap().decided_by, "nvd");
}

/// code review 第六輪 #1：CNA 以 `ESXi80U3d-24585383` 寫上界時可正常判定。
#[test]
fn esxi_compact_upper_bound_from_cna() {
    let mut w = world();
    let c = w.asset("ESX-01", "mgmt", &[("vmware-esxi", "8.0 U2")])[0];
    w.cve(
        CVE,
        json!([affected(
            "VMware",
            "VMware ESXi",
            Some("unaffected"),
            json!([{"version": "8.0", "lessThan": "ESXi80U3d-24585383", "status": "affected"}])
        )]),
        json!([]),
    );
    w.run();
    assert_eq!(w.finding(c, CVE).unwrap().confidence, "confirmed");
}

/// code review 第七輪 #1：PAN-OS 的 hotfix 修正點只適用同一維護版本線。
/// CVE-2024-3400 的 11.1 項目（完整寫法）：11.1.1 未修補應受影響，11.1.1-h1 已修補。
#[test]
fn panos_changes_apply_per_maintenance_line() {
    let cases = [
        ("11.1.1", true),
        ("11.1.1-h1", false),
        ("11.1.0-h2", true),
        ("11.1.0-h3", false),
        ("11.1.2", true),
        ("11.1.2-h3", false),
    ];
    for (version, expected) in cases {
        let mut w = world();
        let c = fw(&w, version);
        w.cve(
            CVE,
            panos(
                json!([{"version": "11.1.0", "lessThan": "11.1.2-h3", "status": "affected",
                "changes": [
                    {"at": "11.1.2-h3", "status": "unaffected"},
                    {"at": "11.1.1-h1", "status": "unaffected"},
                    {"at": "11.1.0-h3", "status": "unaffected"}
                ]}]),
            ),
            json!([]),
        );
        w.run();
        assert_eq!(w.finding(c, CVE).is_some(), expected, "{version}");
    }
}

/// code review 第十三輪 #1：同一來源多條目時，無法判定的條目不被其他條目的 defaultStatus 蓋過。
#[test]
fn unparseable_entry_outweighs_default_not_affected() {
    let mut w = world();
    let c = w.asset("ESX-01", "mgmt", &[("vmware-esxi", "8.0 U2")])[0];
    w.cve(
        CVE,
        json!([
            affected(
                "VMware",
                "VMware ESXi",
                Some("unaffected"),
                json!([{"version": "7.0", "lessThan": "7.0 U3s", "status": "affected"}])
            ),
            affected(
                "VMware",
                "ESXi",
                None,
                json!([{"version": "8.0", "lessThan": "not-a-version", "status": "affected"}])
            )
        ]),
        json!([]),
    );
    w.run();
    assert_eq!(
        w.finding(c, CVE).unwrap().confidence,
        "probable",
        "未知而非不受影響"
    );
}

/// code review 第十三輪 #3：DSM 排除的下界（> 7.2.1）排除整條 7.2.1 版本線。
#[test]
fn dsm_exclusive_lower_bound_excludes_version_line() {
    let mut w = world();
    let line = w.asset(
        "NAS-01",
        "internal",
        &[("synology-dsm", "7.2.1-69057 Update 3")],
    )[0];
    let next = w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.2-72806")])[0];
    w.conn.execute("INSERT INTO cve (id, state, relevance_reason, updated_at) VALUES (?1, 'PUBLISHED', 'nvd', ?2)",
                   rusqlite::params![CVE, T0]).unwrap();
    w.conn
        .execute(
            "INSERT INTO cve_nvd_match (cve_id, config_group, node_group, node_negate, criteria_cpe, vuln,
                                        ver_start_excl, ver_end_excl)
             VALUES (?1, 0, 0, 0, 'cpe:2.3:o:synology:diskstation_manager:*:*:*:*:*:*:*:*', 1, '7.2.1', '7.2.3')",
            [CVE],
        )
        .unwrap();
    w.run();
    assert!(w.finding(line, CVE).is_none(), "7.2.1 版本線被排除");
    assert!(w.finding(next, CVE).is_some());
}

// ── 14：Cisco 與 Juniper 的 CNA 區間 ─────────────────────────

/// 測試用產品（模擬 catalog.d），不依賴種子目錄的內容。
const VENDOR_SCHEMES: &str = r#"
[[product]]
key = "t-cisco"
display_name = "Test Cisco IOS"
vendor_display = "Cisco"
version_scheme = "cisco"
cna = [{ vendor = "TestCisco", product = "IOS" }]

[[product]]
key = "t-junos"
display_name = "Test Junos"
vendor_display = "Juniper Networks"
version_scheme = "junos"
cna = [{ vendor = "TestJuniper", product = "Junos OS" }]
"#;

#[test]
fn cisco_range_from_another_train_is_unknown() {
    let mut w = common::matching::world_with(Some(VENDOR_SCHEMES));
    let other_train = w.asset("SW-01", "internal", &[("t-cisco", "15.2(4)M3")])[0];
    let inside = w.asset("SW-02", "internal", &[("t-cisco", "15.2(7)E9")])[0];
    let fixed = w.asset("SW-03", "internal", &[("t-cisco", "15.2(7)E11")])[0];
    w.cve(
        CVE,
        json!([affected(
            "TestCisco",
            "IOS",
            Some("unaffected"),
            json!([{"version": "15.2(7)E8", "lessThan": "15.2(7)E11", "status": "affected"}])
        )]),
        json!([]),
    );
    w.run();
    let f = w
        .finding(other_train, CVE)
        .expect("跨 train 不可判定為不受影響");
    assert_eq!(f.confidence, "probable");
    assert_eq!(w.finding(inside, CVE).unwrap().confidence, "confirmed");
    assert!(w.finding(fixed, CVE).is_none());
}

#[test]
fn cisco_exact_versions_from_other_trains_are_not_equal() {
    let mut w = common::matching::world_with(Some(VENDOR_SCHEMES));
    let c = w.asset("SW-01", "internal", &[("t-cisco", "15.2(7)E8")])[0];
    w.cve(
        CVE,
        json!([affected(
            "TestCisco",
            "IOS",
            Some("unaffected"),
            json!([
                {"version": "15.2(4)M3", "status": "affected"},
                {"version": "15.0(2)SE11", "status": "affected"}
            ])
        )]),
        json!([]),
    );
    w.run();
    assert!(
        w.finding(c, CVE).is_none(),
        "逐版列舉的其他 train 不涵蓋此版本"
    );
}

#[test]
fn junos_ranges_across_release_lines() {
    let mut w = common::matching::world_with(Some(VENDOR_SCHEMES));
    let c = w.asset("RT-01", "internal", &[("t-junos", "21.4R3-S5")])[0];
    w.cve(
        CVE,
        json!([affected(
            "TestJuniper",
            "Junos OS",
            Some("unaffected"),
            json!([{"version": "22.2", "lessThan": "22.2R3-S3", "status": "affected"}])
        )]),
        json!([]),
    );
    w.run();
    assert!(w.finding(c, CVE).is_none(), "21.4 早於 22.2，不在區間");

    const OTHER: &str = "CVE-2099-10002";
    w.cve(
        OTHER,
        json!([affected(
            "TestJuniper",
            "Junos OS",
            Some("unaffected"),
            json!([
                {"version": "0", "lessThan": "21.4R3-S6", "status": "affected"},
                {"version": "22.2", "lessThan": "22.2R3-S3", "status": "affected"}
            ])
        )]),
        json!([]),
    );
    w.run();
    assert_eq!(w.finding(c, OTHER).unwrap().confidence, "confirmed");
}
