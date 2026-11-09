//! 04 任務 3.2：NVD configurations、原廠 CSAF 與 OSV 的判定，以及來源優先序與矛盾。

mod common;

use common::matching::{affected, panos, world, T0};
use common::{matching::World, read_ingest};
use cvewatch_core::ingest::cve5;
use cvewatch_core::ingest::relevance::Relevance;
use cvewatch_core::ingest::store::upsert_record;
use serde_json::json;

const CVE: &str = "CVE-2099-10001";
const PVE_CPE: &str = "cpe:2.3:a:proxmox:virtual_environment:*:*:*:*:*:*:*:*";
const PANOS_CPE: &str = "cpe:2.3:o:paloaltonetworks:pan-os:*:*:*:*:*:*:*:*";

fn fw(w: &World, version: &str) -> i64 {
    w.asset("FW-01", "internet", &[("paloalto-panos", version)])[0]
}

// ── Requirement: NVD configuration evaluation ───────────────

#[test]
fn nvd_range_with_excluding_end() {
    let mut w = world();
    let c = w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.2")])[0];
    w.nvd(CVE, &[(PVE_CPE, true, None, Some("8.2.4"))]);
    w.run();
    let f = w.finding(c, CVE).expect("命中");
    assert_eq!(
        (f.confidence.as_str(), f.decided_by.as_str()),
        ("confirmed", "nvd")
    );
}

#[test]
fn nvd_fixed_versions_with_update_field() {
    let mut w = world();
    let hit = fw(&w, "10.2.4-h4");
    let miss = w.asset("FW-02", "internal", &[("paloalto-panos", "10.2.4-h5")])[0];
    w.nvd(
        CVE,
        &[(
            "cpe:2.3:o:paloaltonetworks:pan-os:10.2.4:h4:*:*:*:*:*:*",
            true,
            None,
            None,
        )],
    );
    w.run();
    assert!(w.finding(hit, CVE).is_some());
    assert!(w.finding(miss, CVE).is_none());
}

#[test]
fn nvd_platform_conditions_are_ignored_and_noted() {
    let mut w = world();
    let c = w.asset("IDRAC-01", "mgmt", &[("dell-idrac9", "7.00.00.00")])[0];
    w.nvd_config(
        CVE,
        Some("AND"),
        &[
            (
                "cpe:2.3:o:dell:idrac9_firmware:*:*:*:*:*:*:*:*",
                true,
                None,
                Some("7.10.30.00"),
            ),
            (
                "cpe:2.3:h:dell:poweredge_xyz-200:-:*:*:*:*:*:*:*",
                false,
                None,
                None,
            ),
        ],
    );
    w.run();
    let f = w.finding(c, CVE).expect("命中");
    let ev = w.evidence(f.id);
    assert!(ev[0].2.contains("平台條件"), "{}", ev[0].2);
}

#[test]
fn nvd_for_other_products_is_not_evidence() {
    let mut w = world();
    let c = fw(&w, "11.1.4");
    w.nvd(CVE, &[(PVE_CPE, true, None, Some("8.2.4"))]);
    w.run();
    assert!(w.finding(c, CVE).is_none());
}

// ── Requirement: Evidence precedence / Conflicts are visible ─

#[test]
fn vendor_advisory_overrides_nvd_and_records_conflict() {
    let mut w = world();
    let c = fw(&w, "11.1.6");
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-1",
        "2026-10-01T00:00:00Z",
        &[("11.1.6", "exact", "fixed")],
    );
    w.nvd(CVE, &[(PANOS_CPE, true, Some("11.1.0"), Some("11.1.7"))]);
    w.run();
    assert!(w.finding(c, CVE).is_none());
    assert_eq!(
        w.conflicts(),
        vec![(c, CVE.to_string(), "vendor".to_string(), "nvd".to_string())]
    );
}

#[test]
fn only_nvd_has_an_answer() {
    let mut w = world();
    let c = fw(&w, "11.1.4");
    // 原廠公告未涵蓋此版本線、CNA 不在任何項目且無 defaultStatus → 皆未知
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-1",
        "2026-10-01T00:00:00Z",
        &[("vers:generic/>=10.2.0|<10.2.9", "range", "affected")],
    );
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
    w.nvd(CVE, &[(PANOS_CPE, true, Some("11.1.0"), Some("11.1.6"))]);
    w.run();
    let f = w.finding(c, CVE).expect("命中");
    assert_eq!(
        (f.confidence.as_str(), f.decided_by.as_str()),
        ("confirmed", "nvd")
    );
    let sources: Vec<(String, String)> = w.evidence(f.id).into_iter().map(|e| (e.0, e.1)).collect();
    assert_eq!(
        sources,
        vec![
            ("vendor".into(), "unknown".into()),
            ("cna".into(), "unknown".into()),
            ("nvd".into(), "affected".into())
        ]
    );
}

/// 實際紀錄 CVE-2024-3400（fixture 只保留前 6 個 versions）：CNA 對 11.1.1 判定不受影響
/// （defaultStatus = unaffected），CISA ADP 判定受影響 → 不建立命中但記錄矛盾。
#[test]
fn real_record_cna_versus_adp_conflict() {
    let mut w = world();
    let mid = fw(&w, "11.1.1");
    let older = w.asset("FW-02", "internal", &[("paloalto-panos", "11.0.2")])[0];
    let bytes = read_ingest("cvelist/records/CVE-2024-3400.json");
    let record = cve5::parse(&bytes).unwrap();
    upsert_record(&w.conn, &record, &bytes, Relevance::Cna, T0).unwrap();
    w.run();
    assert!(w.finding(mid, "CVE-2024-3400").is_none());
    assert!(w
        .conflicts()
        .contains(&(mid, "CVE-2024-3400".into(), "cna".into(), "adp".into())));
    let f = w.finding(older, "CVE-2024-3400").expect("11.0.2 受影響");
    assert_eq!(f.decided_by, "cna");
}

// ── 原廠 CSAF ───────────────────────────────────────────────

#[test]
fn csaf_lower_bound_ranges_act_as_change_points() {
    let mut w = world();
    let affected_fw = fw(&w, "11.1.4-h7");
    let fixed_fw = w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.4-h40")])[0];
    let tie_fw = w.asset("FW-03", "internal", &[("paloalto-panos", "12.2.5")])[0];
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-2",
        "2026-10-01T00:00:00Z",
        &[
            ("vers:generic/>=11.1.0", "range", "affected"),
            ("vers:generic/>=11.1.4-h37", "range", "fixed"),
            ("vers:generic/>=12.2.4", "range", "affected"),
            ("vers:generic/>=12.2.4", "range", "fixed"),
        ],
    );
    w.csaf_fix(CVE, "paloalto-panos", "PAN-SA-2", "11.1.4-h37");
    w.run();
    let f = w.finding(affected_fw, CVE).expect("受影響");
    assert_eq!(f.decided_by, "vendor");
    assert!(
        w.evidence(f.id)[0].2.contains("11.1.4-h37"),
        "修正版本應在證據中"
    );
    assert!(w.finding(fixed_fw, CVE).is_none());
    // 同樣具體但結論相反 → 未知 → probable
    assert_eq!(w.finding(tie_fw, CVE).unwrap().confidence, "probable");
}

#[test]
fn csaf_latest_advisory_wins() {
    let mut w = world();
    let c = fw(&w, "11.1.4");
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-OLD",
        "2026-01-01T00:00:00Z",
        &[("11.1.4", "exact", "affected")],
    );
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-NEW",
        "2026-06-01T00:00:00Z",
        &[("11.1.4", "exact", "not_affected")],
    );
    w.run();
    assert!(w.finding(c, CVE).is_none());
}

// ── OSV 套件 ────────────────────────────────────────────────

#[test]
fn osv_hits_decide_package_components() {
    let mut w = world();
    w.asset("WEB-01", "dmz", &[]);
    let pkg = w.package(
        "WEB-01",
        "pkg:deb/debian/openssl",
        "3.0.11-1",
        "2026-10-01T00:00:00+00:00",
    );
    w.osv_hit(
        pkg,
        "DEBIAN-CVE-2023-5678",
        &["CVE-2023-5678"],
        "2026-10-02T00:00:00+00:00",
    );
    w.osv_synced("2026-10-02T00:00:00+00:00");
    w.run();
    let f = w.finding(pkg, "CVE-2023-5678").expect("OSV 命中");
    assert_eq!(
        (f.confidence.as_str(), f.decided_by.as_str()),
        ("confirmed", "osv")
    );
    // 升版後、OSV 尚未重查 → 此套件本次不比對，命中維持原狀（不以舊結果改寫）
    w.set_version(pkg, "3.0.13-1", "2026-10-03T00:00:00+00:00");
    let report = w.run();
    assert_eq!(report.awaiting_osv, 1);
    let f = w.finding(pkg, "CVE-2023-5678").unwrap();
    assert_eq!(
        (f.status.as_str(), f.confidence.as_str(), f.in_range),
        ("open", "confirmed", true)
    );
    // OSV 重查後已無命中 → 不受影響 → 自動 resolved
    w.conn.execute("DELETE FROM osv_hit", []).unwrap();
    w.osv_synced("2026-10-04T00:00:00+00:00");
    w.run();
    assert_eq!(w.finding(pkg, "CVE-2023-5678").unwrap().status, "resolved");
}

/// code review 第三輪 #2：`{version: n/a, status: affected}` 不退回 `defaultStatus: unaffected`，
/// CNA 判定為未知，由 NVD 的明確結論建立命中（不是被記成矛盾而漏報）。
#[test]
fn placeholder_affected_entry_does_not_fall_back_to_default() {
    let mut w = world();
    let c = w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.2")])[0];
    let extra = r#"
[[product]]
key = "proxmox-ve"
cna = [{ vendor = "Proxmox Server Solutions GmbH", product = "Proxmox VE" }]
"#;
    w.catalog = common::ingest::catalog_with(Some(extra));
    w.cve(
        CVE,
        json!([affected(
            "Proxmox Server Solutions GmbH",
            "Proxmox VE",
            Some("unaffected"),
            json!([{"version": "n/a", "status": "affected"}])
        )]),
        json!([]),
    );
    w.nvd(CVE, &[(PVE_CPE, true, None, Some("8.2.4"))]);
    w.run();
    let f = w.finding(c, CVE).expect("不可漏報");
    assert_eq!(
        (f.confidence.as_str(), f.decided_by.as_str()),
        ("confirmed", "nvd")
    );
    assert!(w.conflicts().is_empty());
    assert!(
        w.evidence(f.id)[0].2.contains("未提供版本"),
        "{:?}",
        w.evidence(f.id)
    );
}

/// code review 第三輪 #3：CSAF 產品層級（`*`）狀態為最後順位的原廠判定。
#[test]
fn csaf_product_level_status_is_last_resort() {
    // 產品層級 not_affected 優先於 NVD → 不建立命中並記錄矛盾
    let mut w = world();
    let c = fw(&w, "11.1.4");
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-3",
        "2026-10-01T00:00:00Z",
        &[("*", "exact", "not_affected")],
    );
    w.nvd(CVE, &[(PANOS_CPE, true, Some("11.1.0"), Some("11.1.6"))]);
    w.run();
    assert!(w.finding(c, CVE).is_none());
    assert_eq!(
        w.conflicts(),
        vec![(c, CVE.to_string(), "vendor".to_string(), "nvd".to_string())]
    );

    // 只有產品層級 affected → 原廠判定受影響
    let mut w = world();
    let c = fw(&w, "11.1.4");
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-3",
        "2026-10-01T00:00:00Z",
        &[("*", "exact", "affected")],
    );
    w.run();
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (f.confidence.as_str(), f.decided_by.as_str()),
        ("confirmed", "vendor")
    );
    assert!(w.evidence(f.id)[0].2.contains("產品層級"));

    // 版本列有涵蓋時優先於產品層級
    let mut w = world();
    let c = fw(&w, "11.1.4");
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-3",
        "2026-10-01T00:00:00Z",
        &[("*", "exact", "affected"), ("11.1.4", "exact", "fixed")],
    );
    w.run();
    assert!(w.finding(c, CVE).is_none());
}

/// code review 第四輪 #1：CPE update 欄位 `*`（任何 update）涵蓋整條版本線，`-`（無）只有該版本。
#[test]
fn nvd_cpe_any_update_covers_version_line() {
    let mut w = world();
    let hotfix = fw(&w, "11.1.4-h7");
    w.nvd(
        CVE,
        &[(
            "cpe:2.3:o:paloaltonetworks:pan-os:11.1.4:*:*:*:*:*:*:*",
            true,
            None,
            None,
        )],
    );
    let esxi = w.asset("ESX-01", "mgmt", &[("vmware-esxi", "8.0 U3c")])[0];
    w.nvd(
        "CVE-2099-10002",
        &[("cpe:2.3:o:vmware:esxi:8.0:*:*:*:*:*:*:*", true, None, None)],
    );
    let ga_only = w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.4-h7")])[0];
    w.nvd(
        "CVE-2099-10003",
        &[(
            "cpe:2.3:o:paloaltonetworks:pan-os:11.1.4:-:*:*:*:*:*:*",
            true,
            None,
            None,
        )],
    );
    w.run();
    assert_eq!(w.finding(hotfix, CVE).unwrap().decided_by, "nvd");
    assert!(w.finding(esxi, "CVE-2099-10002").is_some());
    assert!(
        w.finding(ga_only, "CVE-2099-10003").is_none(),
        "`-` 只有 GA 版"
    );
    assert!(w.finding(esxi, CVE).is_none());
}

/// code review 第四輪 #3：DSM CPE 的純數字 update 欄位是 build。
#[test]
fn nvd_dsm_build_in_update_field() {
    let mut w = world();
    let c = w.asset(
        "NAS-01",
        "internal",
        &[("synology-dsm", "7.2.1-69057 Update 3")],
    )[0];
    let other = w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.1-64570")])[0];
    w.nvd(
        CVE,
        &[(
            "cpe:2.3:o:synology:diskstation_manager:7.2.1:69057:*:*:*:*:*:*",
            true,
            None,
            None,
        )],
    );
    w.run();
    assert_eq!(w.finding(c, CVE).unwrap().confidence, "confirmed");
    assert!(w.finding(other, CVE).is_none());
}

/// code review 第四輪 #7：只有 `!=` 的 vers 區間＝除了排除的版本以外都在區間內。
#[test]
fn csaf_vers_with_only_exclusions() {
    let mut w = world();
    let included = fw(&w, "11.1.2");
    let excluded = w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.4")])[0];
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-4",
        "2026-10-01T00:00:00Z",
        &[("vers:generic/!=11.1.4", "range", "affected")],
    );
    w.run();
    assert_eq!(w.finding(included, CVE).unwrap().decided_by, "vendor");
    assert_eq!(
        w.finding(excluded, CVE).unwrap().confidence,
        "probable",
        "未涵蓋 → 未知"
    );
}

/// code review 第五輪 #2：最新公告以實際時間比較（時區不同的寫法）。
#[test]
fn csaf_latest_advisory_compares_timestamps() {
    let mut w = world();
    let c = fw(&w, "11.1.4");
    // 01:00Z（寫成 +08:00）較舊；08:00Z 較新
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-A",
        "2024-04-12T09:00:00+08:00",
        &[("11.1.4", "exact", "affected")],
    );
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-B",
        "2024-04-12T08:00:00Z",
        &[("11.1.4", "exact", "fixed")],
    );
    w.run();
    assert!(
        w.finding(c, CVE).is_none(),
        "應採用較新的 PAN-SA-B（fixed）"
    );
}

/// code review 第六輪 #3：DSM CPE update 欄位為 `build-update` 寫法。
#[test]
fn nvd_dsm_build_and_update_in_update_field() {
    let mut w = world();
    let c = w.asset(
        "NAS-01",
        "internal",
        &[("synology-dsm", "7.1.1-42962 Update 1")],
    )[0];
    w.nvd(
        CVE,
        &[(
            "cpe:2.3:o:synology:diskstation_manager:7.1.1:42962-1:*:*:*:*:*:*",
            true,
            None,
            None,
        )],
    );
    w.run();
    assert_eq!(w.finding(c, CVE).unwrap().decided_by, "nvd");
}

/// code review 第七輪 #1：CSAF 只有下界的修正點同樣只適用同一維護版本線。
#[test]
fn csaf_hotfix_fix_points_apply_per_maintenance_line() {
    let mut w = world();
    let later_line = fw(&w, "11.1.5");
    let same_line = w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.4-h40")])[0];
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-5",
        "2026-10-01T00:00:00Z",
        &[
            ("vers:generic/>=11.1.0", "range", "affected"),
            ("vers:generic/>=11.1.4-h37", "range", "fixed"),
        ],
    );
    w.run();
    assert!(
        w.finding(later_line, CVE).is_some(),
        "11.1.5 不在 11.1.4 版本線"
    );
    assert!(w.finding(same_line, CVE).is_none());
}

/// code review 第九輪 #2：退役資產重新啟用時，其套件等下次 OSV 同步後再比對（不使用退役前的結果）。
#[test]
fn reactivated_asset_packages_wait_for_osv() {
    use cvewatch_core::assets::repo::AssetRepo;

    let mut w = world();
    w.asset("WEB-01", "dmz", &[]);
    let pkg = w.package(
        "WEB-01",
        "pkg:deb/debian/openssl",
        "3.0.11-1",
        "2026-10-01T00:00:00+00:00",
    );
    w.osv_synced("2026-10-02T00:00:00+00:00");
    let asset_id: i64 = w
        .conn
        .query_row("SELECT asset_id FROM component WHERE id = ?1", [pkg], |r| {
            r.get(0)
        })
        .unwrap();
    AssetRepo::new(&w.conn)
        .mark_components_updated(asset_id, common::matching::at("2026-11-01T00:00:00+00:00"))
        .unwrap();
    let report = w.run();
    assert_eq!(report.awaiting_osv, 1);
}

/// code review 第十六輪 #4：OSV 命中的 JSON 損壞時，該套件與等待 OSV 一樣不比對，命中維持原狀
/// （不以空清單判定成「不再列出」而自動 resolved）。
#[test]
fn corrupt_osv_hit_keeps_findings_until_next_osv_sync() {
    let mut w = world();
    w.asset("WEB-01", "dmz", &[]);
    let pkg = w.package(
        "WEB-01",
        "pkg:deb/debian/openssl",
        "3.0.11-1",
        "2026-10-01T00:00:00+00:00",
    );
    w.osv_hit(
        pkg,
        "DEBIAN-CVE-2023-5678",
        &["CVE-2023-5678"],
        "2026-10-02T00:00:00+00:00",
    );
    w.osv_synced("2026-10-02T00:00:00+00:00");
    w.run();
    assert!(w.finding(pkg, "CVE-2023-5678").is_some());
    w.conn
        .execute("UPDATE osv_hit SET cve_ids_json = '{損壞'", [])
        .unwrap();
    let report = w.run();
    assert_eq!(report.awaiting_osv, 1);
    let f = w.finding(pkg, "CVE-2023-5678").unwrap();
    assert_eq!((f.status.as_str(), f.in_range), ("open", true));
}

/// code review 第十三輪 #2：原廠公告有無法解析的版本列時，不以其他區間下定論。
#[test]
fn csaf_unparseable_rows_prevent_range_verdict() {
    let mut w = world();
    let c = fw(&w, "11.1.4-h8");
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-6",
        "2026-10-01T00:00:00Z",
        &[
            ("vers:generic/>=11.1.0", "range", "fixed"),
            ("vers:generic/>=11.1.4-hotfix7", "range", "affected"),
        ],
    );
    w.cve(
        CVE,
        panos(json!([{"version": "11.1.0", "lessThan": "11.1.5", "status": "affected"}])),
        json!([]),
    );
    w.run();
    let f = w.finding(c, CVE).expect("原廠未知 → 由 CNA 判定受影響");
    assert_eq!(f.decided_by, "cna");
}
