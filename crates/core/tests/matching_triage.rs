//! 04 任務 5.4：finding-triage — 手動狀態與驗證、重算保留使用者狀態、每條自動規則、清單與矛盾。

mod common;

use common::matching::{at, panos, world, World, T0};
use cvewatch_core::ingest::relevance::Relevance;
use cvewatch_core::matching::engine::Confidence;
use cvewatch_core::matching::findings::{NOTE_KEV_REOPEN, NOTE_OUT_OF_RANGE, NOTE_REJECTED};
use cvewatch_core::matching::query::{self, FindingFilter};
use cvewatch_core::matching::scoring::Priority;
use cvewatch_core::matching::triage::{self, ChangeRequest, Justification, Status};
use serde_json::json;

const CVE: &str = "CVE-2099-40001";
const LATER: &str = "2026-11-02T00:00:00+00:00";

fn affected_world() -> (World, i64, i64) {
    let mut w = world();
    let c = w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.1-69057")])[0];
    w.cve(
        CVE,
        json!([{"vendor": "Synology", "product": "DiskStation Manager (DSM)", "defaultStatus": "unaffected",
                "versions": [{"version": "7.2", "lessThan": "7.2.2-72806", "status": "affected"}]}]),
        json!([]),
    );
    w.run();
    let id = w.finding(c, CVE).expect("命中").id;
    (w, c, id)
}

fn request(
    status: &str,
    reason: Option<&str>,
    justification: Option<&str>,
    note: &str,
) -> ChangeRequest {
    ChangeRequest {
        status: status.into(),
        reason: reason.map(str::to_string),
        justification: justification.map(str::to_string),
        note: note.into(),
    }
}

fn set(w: &World, id: i64, req: &ChangeRequest) {
    let change = triage::validate(req).expect("有效的變更");
    triage::set_status(&w.conn, id, &change, at(LATER)).expect("set");
}

// ── Requirement: Three triage states ────────────────────────

#[test]
fn new_finding_is_open_with_system_event() {
    let (w, _, id) = affected_world();
    let events = w.events(id);
    assert_eq!(events.len(), 1);
    let (actor, from, to, note) = &events[0];
    assert_eq!(
        (actor.as_str(), from.as_deref(), to.as_str()),
        ("system", None, "open")
    );
    assert!(note.contains("CNA"), "{note}");
}

// ── Requirement: Manual state change / Ignore reason is required ─

#[test]
fn ignoring_with_justification() {
    let (w, c, id) = affected_world();
    set(
        &w,
        id,
        &request(
            "ignored",
            Some("not_affected"),
            Some("requires_configuration"),
            "未啟用 GlobalProtect",
        ),
    );
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(f.status, "ignored");
    assert_eq!(f.ignore_reason.as_deref(), Some("not_affected"));
    assert_eq!(f.justification.as_deref(), Some("requires_configuration"));
    let last = w.events(id).pop().unwrap();
    assert_eq!(
        last,
        (
            "user".into(),
            Some("open".into()),
            "ignored".into(),
            "未啟用 GlobalProtect".into()
        )
    );
}

#[test]
fn ignoring_without_reason_lists_reasons() {
    let err = triage::validate(&request("ignored", None, None, "x"))
        .unwrap_err()
        .to_string();
    for reason in ["not_affected", "risk_accepted", "false_positive"] {
        assert!(err.contains(reason), "{err}");
    }
}

#[test]
fn not_affected_without_justification_lists_values() {
    let err = triage::validate(&request("ignored", Some("not_affected"), None, ""))
        .unwrap_err()
        .to_string();
    for j in Justification::ALL.iter().map(|j| j.as_str()) {
        assert!(err.contains(j), "{err}");
    }
    let bad = triage::validate(&request(
        "ignored",
        Some("not_affected"),
        Some("because"),
        "",
    ))
    .unwrap_err()
    .to_string();
    assert!(
        bad.contains("because") && bad.contains("code_not_present"),
        "{bad}"
    );
}

#[test]
fn accepting_risk_needs_no_justification() {
    let (w, c, id) = affected_world();
    set(&w, id, &request("ignored", Some("risk_accepted"), None, ""));
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (
            f.status.as_str(),
            f.ignore_reason.as_deref(),
            f.justification
        ),
        ("ignored", Some("risk_accepted"), None)
    );
}

#[test]
fn invalid_combinations_are_rejected() {
    for req in [
        request("closed", None, None, ""),
        request("open", Some("risk_accepted"), None, ""),
        request("resolved", None, Some("code_not_present"), ""),
        request(
            "ignored",
            Some("risk_accepted"),
            Some("code_not_present"),
            "",
        ),
        request("ignored", Some("nope"), None, ""),
    ] {
        assert!(triage::validate(&req).is_err(), "{req:?}");
    }
    let (w, _, _) = affected_world();
    let change = triage::validate(&request("open", None, None, "")).unwrap();
    let err = triage::set_status(&w.conn, 999, &change, at(LATER))
        .unwrap_err()
        .to_string();
    assert!(err.contains("999"), "{err}");
}

#[test]
fn reopening_clears_reason() {
    let (w, c, id) = affected_world();
    set(
        &w,
        id,
        &request("ignored", Some("false_positive"), None, ""),
    );
    set(&w, id, &request("open", None, None, "重新檢視"));
    let f = w.finding(c, CVE).unwrap();
    assert_eq!((f.status.as_str(), f.ignore_reason), ("open", None));
    assert_eq!(w.events(id).len(), 3);
}

// ── Requirement: User decisions survive recomputation ───────

#[test]
fn rerunning_match_keeps_user_state() {
    let (mut w, c, id) = affected_world();
    set(
        &w,
        id,
        &request(
            "ignored",
            Some("not_affected"),
            Some("protected_at_perimeter"),
            "管理介面只開內網",
        ),
    );
    w.run_at(LATER);
    w.run_at(LATER);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(f.status, "ignored");
    assert_eq!(f.justification.as_deref(), Some("protected_at_perimeter"));
    assert_eq!(w.events(id).len(), 2, "重算不新增事件");
}

// ── Requirement: Automatic resolution after upgrade ─────────

#[test]
fn firmware_upgrade_resolves_open_finding() {
    let (mut w, c, id) = affected_world();
    w.set_version(c, "7.2.2-72806 Update 1", LATER);
    let report = w.run_at(LATER);
    assert_eq!(report.findings.resolved, 1);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (f.status.as_str(), f.in_range, f.still_vulnerable),
        ("resolved", false, false)
    );
    let (actor, _, to, note) = w.events(id).pop().unwrap();
    assert_eq!((actor.as_str(), to.as_str()), ("system", "resolved"));
    assert!(
        note.contains(NOTE_OUT_OF_RANGE) && note.contains("7.2.2-72806 Update 1"),
        "{note}"
    );
}

// ── Requirement: Automatic ignore on rejected CVE ───────────

#[test]
fn rejected_cve_ignores_open_finding() {
    let (mut w, c, id) = affected_world();
    w.cve_full(
        CVE,
        "REJECTED",
        "2026-11-02T00:00:00Z",
        None,
        json!([]),
        json!([]),
        Relevance::Cna,
    );
    let report = w.run_at(LATER);
    assert_eq!(report.findings.rejected, 1);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (f.status.as_str(), f.ignore_reason.as_deref()),
        ("ignored", Some("false_positive"))
    );
    assert_eq!(w.events(id).pop().unwrap().3, NOTE_REJECTED);
}

#[test]
fn rejected_cve_keeps_user_resolved_state() {
    let (mut w, c, id) = affected_world();
    set(&w, id, &request("resolved", None, None, "已升級"));
    assert!(w.finding(c, CVE).unwrap().still_vulnerable);
    w.cve_full(
        CVE,
        "REJECTED",
        "2026-11-02T00:00:00Z",
        None,
        json!([]),
        json!([]),
        Relevance::Cna,
    );
    w.run_at(LATER);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(f.status, "resolved");
    // code review 第二輪 #4：撤銷後不再顯示「已處理但仍受影響」
    assert!(!f.still_vulnerable && !f.in_range);
}

// ── Requirement: Reopen ignored findings on KEV listing ─────

#[test]
fn ignored_cve_newly_in_kev_is_reopened() {
    let (mut w, c, id) = affected_world();
    set(&w, id, &request("ignored", Some("risk_accepted"), None, ""));
    w.kev(CVE, "2026-11-02");
    let report = w.run_at(LATER);
    assert_eq!(report.findings.reopened, 1);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (f.status.as_str(), f.ignore_reason, f.priority.as_str()),
        ("open", None, "P1")
    );
    assert_eq!(w.events(id).pop().unwrap().3, NOTE_KEV_REOPEN);
    // 使用者再次忽略（已知在 KEV）→ 之後的比對不再重新開啟
    set(
        &w,
        id,
        &request("ignored", Some("risk_accepted"), None, "已評估"),
    );
    w.run_at(LATER);
    assert_eq!(w.finding(c, CVE).unwrap().status, "ignored");
}

#[test]
fn ignoring_an_already_kev_listed_cve_is_respected() {
    let (mut w, c, id) = affected_world();
    w.kev(CVE, "2026-11-01");
    set(&w, id, &request("ignored", Some("risk_accepted"), None, ""));
    w.run_at(LATER);
    assert_eq!(w.finding(c, CVE).unwrap().status, "ignored");
}

/// code review 第十七輪 #6：不受影響期間由使用者變更狀態時不記錄 KEV 狀態，
/// 回到受影響範圍後仍依「新列入 KEV」重新開啟。
#[test]
fn user_change_while_out_of_range_keeps_kev_reopen() {
    let (mut w, c, id) = affected_world();
    w.set_version(c, "7.2.2-72806", LATER);
    w.run_at(LATER);
    assert_eq!(w.finding(c, CVE).unwrap().status, "resolved");
    w.kev(CVE, "2026-11-03");
    set(
        &w,
        id,
        &request("ignored", Some("risk_accepted"), None, "整理"),
    );
    w.set_version(c, "7.2.1-69057", "2026-11-04T00:00:00+00:00");
    w.run_at("2026-11-04T00:00:00+00:00");
    assert_eq!(w.finding(c, CVE).unwrap().status, "open");
}

// ── Requirement: Warning for resolved but still vulnerable ──

#[test]
fn resolved_without_upgrade_is_flagged() {
    let (mut w, c, id) = affected_world();
    set(&w, id, &request("resolved", None, None, "已套用暫時措施"));
    assert!(
        w.finding(c, CVE).unwrap().still_vulnerable,
        "設定當下即標示"
    );
    let report = w.run_at(LATER);
    assert_eq!(report.findings.still_vulnerable, 1);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!((f.status.as_str(), f.still_vulnerable), ("resolved", true));
    // 實際升版後標示消失
    w.set_version(c, "7.2.2-72806", LATER);
    w.run_at(LATER);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!((f.status.as_str(), f.still_vulnerable), ("resolved", false));
}

// ── Requirement: Finding listing ────────────────────────────

#[test]
fn listing_open_p1_and_p2() {
    let mut w = world();
    let fw = w.asset("FW-01", "internal", &[("paloalto-panos", "11.1.4")])[0];
    let range = json!([{"version": "11.1.0", "lessThan": "11.1.6", "status": "affected"}]);
    for (cve, epss) in [
        ("CVE-2099-40011", 0.5),
        ("CVE-2099-40012", 0.02),
        ("CVE-2099-40013", 0.9),
    ] {
        w.cve(cve, panos(range.clone()), json!([]));
        w.epss(cve, epss);
    }
    w.kev("CVE-2099-40014", "2026-10-30");
    w.cve("CVE-2099-40014", panos(range.clone()), json!([]));
    w.run();
    let ignored = w.finding(fw, "CVE-2099-40013").unwrap().id;
    set(
        &w,
        ignored,
        &request("ignored", Some("risk_accepted"), None, ""),
    );
    let filter = FindingFilter {
        statuses: vec![Status::Open],
        priorities: vec![Priority::P1, Priority::P2],
        ..FindingFilter::default()
    };
    let rows = query::list(&w.conn, &filter).unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.cve_id.as_str()).collect();
    assert_eq!(ids, vec!["CVE-2099-40014", "CVE-2099-40011"]);
    let by_asset = FindingFilter {
        asset_tag: Some("fw-01".into()),
        confidence: Some(Confidence::Confirmed),
        ..FindingFilter::default()
    };
    assert_eq!(query::list(&w.conn, &by_asset).unwrap().len(), 4);
}

#[test]
fn show_includes_evidence_fixes_and_timeline() {
    let mut w = world();
    let c = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4")])[0];
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-7",
        "2026-10-01T00:00:00Z",
        &[("vers:generic/>=11.1.0|<11.1.5", "range", "affected")],
    );
    w.csaf_fix(CVE, "paloalto-panos", "PAN-SA-7", "11.1.5");
    w.run();
    let id = w.finding(c, CVE).unwrap().id;
    let detail = query::show(&w.conn, id).unwrap().unwrap();
    assert_eq!(detail.row.decided_by, "vendor");
    assert_eq!(
        detail.evidence[0].fixed_versions,
        vec!["11.1.5".to_string()]
    );
    assert_eq!(detail.events.len(), 1);
    assert!(detail.first_seen.starts_with(&T0[..10]));
    assert!(query::show(&w.conn, 12345).unwrap().is_none());
}

// ── Requirement: Conflicts are visible（vuln-matching）─────

#[test]
fn listing_conflicts() {
    let mut w = world();
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.6")]);
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-1",
        "2026-10-01T00:00:00Z",
        &[("11.1.6", "exact", "fixed")],
    );
    w.nvd(
        CVE,
        &[(
            "cpe:2.3:o:paloaltonetworks:pan-os:*:*:*:*:*:*:*:*",
            true,
            Some("11.1.0"),
            Some("11.1.7"),
        )],
    );
    w.run();
    let rows = query::conflicts(&w.conn).unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(
        (
            r.asset_tag.as_str(),
            r.component.as_str(),
            r.cve_id.as_str(),
            r.final_source.as_str(),
            r.other_source.as_str()
        ),
        ("FW-01", "paloalto-panos", CVE, "vendor", "nvd")
    );
    // 矛盾解除（NVD 更正）後重算即消失
    w.conn.execute("DELETE FROM cve_nvd_match", []).unwrap();
    w.run();
    assert!(query::conflicts(&w.conn).unwrap().is_empty());
}

/// code review #3：已忽略、但最新資料顯示不受影響時新列入 KEV → 不重新開啟、保留原因；
/// 之後回到受影響範圍才依規則重新開啟。
#[test]
fn kev_listing_does_not_reopen_finding_out_of_range() {
    let (mut w, c, id) = affected_world();
    set(&w, id, &request("ignored", Some("risk_accepted"), None, ""));
    w.set_version(c, "7.2.2-72806", LATER);
    w.kev(CVE, "2026-11-02");
    let report = w.run_at(LATER);
    assert_eq!(report.findings.reopened, 0);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (f.status.as_str(), f.ignore_reason.as_deref()),
        ("ignored", Some("risk_accepted"))
    );
    w.run_at(LATER);
    assert_eq!(w.events(id).len(), 2, "沒有多餘的系統事件");
    // 版本回退到受影響範圍 → 依「新列入 KEV」規則重新開啟
    w.set_version(c, "7.2.1-69057", LATER);
    w.run_at(LATER);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(f.status, "open");
    assert_eq!(w.events(id).pop().unwrap().3, NOTE_KEV_REOPEN);
}

/// code review #2：套件從清單消失時元件軟刪除，命中與處理紀錄保留；再次出現時沿用同一命中。
#[test]
fn removed_package_keeps_triage_and_is_restored() {
    use cvewatch_core::assets::model::ComponentSource;
    use cvewatch_core::assets::repo::{ComponentRepo, PackageUpsert, UpsertOutcome};

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
    let id = w.finding(pkg, "CVE-2023-5678").unwrap().id;
    set(
        &w,
        id,
        &request(
            "ignored",
            Some("not_affected"),
            Some("code_not_present"),
            "未使用受影響函式",
        ),
    );

    ComponentRepo::new(&w.conn).remove(pkg, at(LATER)).unwrap();
    let report = w.run_at(LATER);
    assert_eq!(report.components, 0, "已移除的元件不參與比對");
    let rows = query::list(&w.conn, &FindingFilter::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].component_removed);
    assert_eq!(rows[0].status, Status::Ignored);

    let upsert = PackageUpsert {
        purl: "pkg:deb/debian/openssl",
        purl_full: "pkg:deb/debian/openssl@3.0.11-1",
        version_raw: "3.0.11-1",
        source: ComponentSource::Dpkg,
    };
    let asset_id: i64 = w
        .conn
        .query_row("SELECT asset_id FROM component WHERE id = ?1", [pkg], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        ComponentRepo::new(&w.conn)
            .upsert_package(asset_id, &upsert, at(LATER))
            .unwrap(),
        UpsertOutcome::Inserted
    );
    w.run_at(LATER);
    let f = w.finding(pkg, "CVE-2023-5678").expect("沿用原元件與命中");
    assert_eq!(f.id, id);
    assert_eq!(
        (f.status.as_str(), f.justification.as_deref()),
        ("ignored", Some("code_not_present"))
    );
    assert_eq!(w.events(id).len(), 2, "時間軸完整保留");
    assert!(!query::list(&w.conn, &FindingFilter::default()).unwrap()[0].component_removed);
}

/// code review 第二輪 #3：產品暫時不在目錄中 → 元件本次不比對、命中維持不變；目錄恢復後照常。
#[test]
fn product_missing_from_catalog_leaves_findings_untouched() {
    use cvewatch_core::catalog::{Catalog, CatalogSource};

    let (mut w, c, id) = affected_world();
    let seed = w.catalog.clone();
    w.catalog = Catalog::from_sources(&[CatalogSource {
        name: "only-panos.toml".into(),
        text: "[[product]]
key = \"paloalto-panos\"
display_name = \"PAN-OS\"
               vendor_display = \"Palo Alto Networks\"
version_scheme = \"panos\"
"
        .into(),
    }])
    .unwrap();
    let report = w.run_at(LATER);
    assert_eq!(report.skipped_components, 1);
    assert_eq!(report.skipped_products, vec!["synology-dsm".to_string()]);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!((f.status.as_str(), f.in_range), ("open", true));
    assert_eq!(w.events(id).len(), 1, "沒有自動轉換事件");
    w.catalog = seed;
    let report = w.run_at(LATER);
    assert_eq!(report.skipped_components, 0);
    assert_eq!(w.finding(c, CVE).unwrap().status, "open");
}

/// code review 第二輪 #8、第二十輪 #2：資料庫中的 affected JSON 損壞時不中斷比對；該 CVE 本次不比對，
/// 命中維持原狀（不以空清單退回 defaultStatus 而誤判為不受影響並自動處理）。
#[test]
fn corrupt_stored_json_does_not_abort_matching() {
    let (mut w, c, _) = affected_world();
    w.conn
        .execute("UPDATE cve_affected SET versions_json = '{'", [])
        .unwrap();
    let report = w.run_at(LATER);
    assert_eq!(report.unreadable_cves, 1);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (f.status.as_str(), f.confidence.as_str(), f.in_range),
        ("open", "confirmed", true)
    );
}

/// code review 第二輪 #5：自動轉為已處理只在「由受影響變為不受影響」時觸發；
/// 使用者手動重新開啟已不受影響的命中後，重算維持未處理。
#[test]
fn user_reopened_out_of_range_finding_stays_open() {
    let (mut w, c, id) = affected_world();
    w.set_version(c, "7.2.2-72806", LATER);
    w.run_at(LATER);
    assert_eq!(w.finding(c, CVE).unwrap().status, "resolved");
    set(
        &w,
        id,
        &request("open", None, None, "版本資料有誤，持續追蹤"),
    );
    let report = w.run_at(LATER);
    assert_eq!(report.findings.resolved, 0);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!((f.status.as_str(), f.in_range), ("open", false));
    assert_eq!(w.events(id).last().unwrap().0, "user", "沒有新的系統事件");
    // 之後回到受影響範圍、再升版 → 依規則再次自動處理
    w.set_version(c, "7.2.1-69057", LATER);
    w.run_at(LATER);
    w.set_version(c, "7.2.2-72806", LATER);
    w.run_at(LATER);
    assert_eq!(w.finding(c, CVE).unwrap().status, "resolved");
}

/// code review 第八輪 #5：justification 與 reason 一樣不分大小寫。
#[test]
fn justification_is_case_insensitive() {
    let change = triage::validate(&request(
        "ignored",
        Some("NOT_AFFECTED"),
        Some("Code_Not_Present"),
        "",
    ))
    .unwrap();
    assert_eq!(change.justification, Some(Justification::CodeNotPresent));
}

#[test]
fn set_status_many_skips_unchanged_only_when_asked() {
    let (mut w, c, id) = affected_world();
    let events = |w: &World| w.events(id).len();
    let open = triage::validate(&request("open", None, None, "")).unwrap();
    let skipped = triage::set_status_many(&mut w.conn, &[id], &open, at(LATER), true).unwrap();
    assert_eq!((skipped.changed.len(), skipped.unchanged), (0, vec![id]));
    let before = events(&w);
    // CLI：不略過，同狀態也記錄事件（保留備註）
    let recorded = triage::set_status_many(&mut w.conn, &[id], &open, at(LATER), false).unwrap();
    assert_eq!(recorded.changed, [id]);
    assert_eq!(events(&w), before + 1);
    let accept = triage::validate(&request("ignored", Some("risk_accepted"), None, "")).unwrap();
    assert_eq!(
        triage::set_status_many(&mut w.conn, &[id], &accept, at(LATER), true)
            .unwrap()
            .changed,
        [id]
    );
    // 同為忽略但原因不同 → 視為變更
    let fp = triage::validate(&request("ignored", Some("false_positive"), None, "")).unwrap();
    assert_eq!(
        triage::set_status_many(&mut w.conn, &[id], &fp, at(LATER), true)
            .unwrap()
            .changed,
        [id]
    );
    // 找不到的命中：整批回復
    assert!(triage::set_status_many(&mut w.conn, &[id, 999_999], &open, at(LATER), true).is_err());
    assert_eq!(w.finding(c, CVE).unwrap().status, "ignored");
}
