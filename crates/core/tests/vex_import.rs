//! 07 任務 3.5：vex-exchange 匯入 — 往返無損、其他工具樣本、未對應不新建、本機較新不覆蓋、
//! dry-run、事件紀錄、加密檔、匯入後 KEV 重新開啟。

mod common;

use std::cell::RefCell;
use std::path::Path;

use common::matching::{at, World};
use common::vex::{
    finding_id, fixed_meta, req, set, verdict, vex_fixture, vex_world, FW, NOTE_NOT_AFFECTED,
    NOTE_RESOLVED, NOT_AFFECTED_CVE, OPEN_CVE, PKG_CVE, RESOLVED_CVE, RISK_CVE, WEB,
};
use cvewatch_core::matching::findings::NOTE_KEV_REOPEN;
use cvewatch_core::matching::query::ids as finding_ids;
use cvewatch_core::matching::query::FindingFilter;
use cvewatch_core::matching::triage::Status;
use cvewatch_core::vex::export::{build, to_json};
use cvewatch_core::vex::file::export_to_file;
use cvewatch_core::vex::import::{self, ImportOptions, ImportReport, Method};
use serde_json::{json, Value};
use zeroize::Zeroizing;

const IMPORTED: &str = "2026-11-06T00:00:00+00:00";
const AFTER_EXPORT: &str = "2026-11-04T00:00:00+00:00";

fn no_passphrase() -> Result<Zeroizing<String>, cvewatch_core::CoreError> {
    panic!("未加密的檔案不應要求口令")
}

fn run(w: &mut World, path: &Path, options: ImportOptions) -> ImportReport {
    let bom = import::read_file(path, no_passphrase).expect("read");
    let name = import::file_name(path);
    import::import(&mut w.conn, &bom, &name, options, at(IMPORTED), &|_, _| {
        Ok(())
    })
    .expect("import")
}

fn run_json(w: &mut World, doc: &Value, options: ImportOptions) -> ImportReport {
    let bom = import::doc::parse(doc.to_string().as_bytes()).expect("parse");
    import::import(
        &mut w.conn,
        &bom,
        "inline.json",
        options,
        at(IMPORTED),
        &|_, _| Ok(()),
    )
    .expect("import")
}

fn apply() -> ImportOptions {
    ImportOptions::default()
}

fn force() -> ImportOptions {
    ImportOptions {
        force: true,
        ..ImportOptions::default()
    }
}

fn dry_run() -> ImportOptions {
    ImportOptions {
        dry_run: true,
        ..ImportOptions::default()
    }
}

fn export_value(w: &World) -> Value {
    let ids = finding_ids(&w.conn, &FindingFilter::default()).unwrap();
    serde_json::from_str(&to_json(&build(&w.conn, &ids, &fixed_meta()).unwrap()).unwrap()).unwrap()
}

/// 各 vulnerability 的 (affects, id) → analysis 中狀態相關欄位。
fn analyses(doc: &Value) -> Vec<(String, String, Value)> {
    let mut out: Vec<_> = doc["vulnerabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let a = &v["analysis"];
            (
                v["affects"][0]["ref"].as_str().unwrap().to_string(),
                v["id"].as_str().unwrap().to_string(),
                json!([a["state"], a["justification"], a["response"], a["detail"]]),
            )
        })
        .collect();
    out.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    out
}

fn export_file(w: &World, dir: &Path) -> std::path::PathBuf {
    let path = dir.join("cvewatch-export.json");
    let ids = finding_ids(&w.conn, &FindingFilter::default()).unwrap();
    export_to_file(&w.conn, &ids, &fixed_meta(), &path, None).unwrap();
    path
}

fn event_count(w: &World) -> i64 {
    w.conn
        .query_row("SELECT count(*) FROM finding_event", [], |r| r.get(0))
        .unwrap()
}

fn finding_count(w: &World) -> i64 {
    w.conn
        .query_row("SELECT count(*) FROM finding", [], |r| r.get(0))
        .unwrap()
}

fn last_event(w: &World, asset: &str, cve: &str) -> (String, String, String, Option<String>) {
    w.conn
        .query_row(
            "SELECT actor, to_status, note, import_file FROM finding_event
             WHERE finding_id = ?1 ORDER BY id DESC LIMIT 1",
            [finding_id(w, asset, cve)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
}

fn cves(items: &[import::Item]) -> Vec<&str> {
    items.iter().map(|i| i.vuln.as_str()).collect()
}

// ── Requirement: Lossless round trip ─────────────────────────

#[test]
fn restore_after_reset_is_lossless() {
    let mut w = vex_world();
    let dir = tempfile::tempdir().unwrap();
    let original = export_value(&w);
    let path = export_file(&w, dir.path());
    for (asset, cve) in [
        (FW, RESOLVED_CVE),
        (FW, NOT_AFFECTED_CVE),
        (FW, RISK_CVE),
        (WEB, PKG_CVE),
    ] {
        set(&w, asset, cve, req("open", None, None, ""), AFTER_EXPORT);
    }
    let report = run(&mut w, &path, force());
    assert_eq!(report.plan.apply.len(), 4, "{report:#?}");
    assert_eq!(cves(&report.plan.unchanged), [OPEN_CVE]);
    assert_eq!(
        verdict(&w, FW, NOT_AFFECTED_CVE),
        (
            "ignored".into(),
            Some("not_affected".into()),
            Some("requires_configuration".into())
        )
    );
    assert_eq!(
        analyses(&export_value(&w)),
        analyses(&original),
        "狀態、原因、理由與備註完全還原"
    );
}

#[test]
fn reimporting_own_export_changes_nothing() {
    let mut w = vex_world();
    let dir = tempfile::tempdir().unwrap();
    let path = export_file(&w, dir.path());
    let before = event_count(&w);
    let report = run(&mut w, &path, apply());
    assert_eq!(report.plan.unchanged.len(), 5);
    assert!(report.plan.apply.is_empty() && report.plan.local_newer.is_empty());
    assert_eq!(event_count(&w), before, "相同的判斷不寫入事件");
}

#[test]
fn imported_detail_survives_re_export_without_file_prefix() {
    let mut w = vex_world();
    let dir = tempfile::tempdir().unwrap();
    let path = export_file(&w, dir.path());
    set(
        &w,
        FW,
        RESOLVED_CVE,
        req("open", None, None, ""),
        AFTER_EXPORT,
    );
    run(&mut w, &path, force());
    let (actor, to, note, file) = last_event(&w, FW, RESOLVED_CVE);
    assert_eq!((actor.as_str(), to.as_str()), ("import", "resolved"));
    assert_eq!(note, format!("cvewatch-export.json：{NOTE_RESOLVED}"));
    assert_eq!(file.as_deref(), Some("cvewatch-export.json"));
    let again = export_value(&w);
    let v = again["vulnerabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == RESOLVED_CVE)
        .unwrap();
    assert_eq!(v["analysis"]["detail"], NOTE_RESOLVED);
}

// ── Requirement: Newer local decisions win ───────────────────

#[test]
fn local_change_after_export_is_kept() {
    let mut w = vex_world();
    let dir = tempfile::tempdir().unwrap();
    let path = export_file(&w, dir.path());
    set(
        &w,
        FW,
        NOT_AFFECTED_CVE,
        req("resolved", None, None, "已升級"),
        AFTER_EXPORT,
    );
    let report = run(&mut w, &path, apply());
    assert_eq!(cves(&report.plan.local_newer), [NOT_AFFECTED_CVE]);
    assert!(report.plan.apply.is_empty());
    assert_eq!(
        verdict(&w, FW, NOT_AFFECTED_CVE).0,
        "resolved",
        "維持已處理"
    );
}

#[test]
fn missing_timestamps_count_as_older_than_local_decisions() {
    let mut w = vex_world();
    let doc = json!({
        "bomFormat": "CycloneDX", "specVersion": "1.6",
        "vulnerabilities": [
            {"id": RESOLVED_CVE, "analysis": {"state": "in_triage"},
             "affects": [{"ref": "cvewatch:FW-01:paloalto-panos"}]},
            {"id": OPEN_CVE, "analysis": {"state": "resolved"},
             "affects": [{"ref": "cvewatch:FW-01:paloalto-panos"}]}
        ]
    });
    let report = run_json(&mut w, &doc, apply());
    assert_eq!(
        cves(&report.plan.local_newer),
        [RESOLVED_CVE],
        "本機有人為判定"
    );
    assert_eq!(
        cves(
            &report
                .plan
                .apply
                .iter()
                .map(|p| p.item.clone())
                .collect::<Vec<_>>()
        ),
        [OPEN_CVE]
    );
}

// ── Requirement: Import with component matching ──────────────

#[test]
fn dependency_track_sample_matches_package_by_purl() {
    let mut w = vex_world();
    let findings = finding_count(&w);
    let report = run(&mut w, &vex_fixture("dependency-track.json"), apply());
    let applied = &report.plan.apply;
    assert_eq!(applied.len(), 1, "{report:#?}");
    assert_eq!(applied[0].item.method, Some(Method::Purl));
    assert_eq!(applied[0].item.target, "WEB-01:pkg:deb/debian/openssl");
    assert_eq!(
        verdict(&w, WEB, PKG_CVE),
        (
            "ignored".into(),
            Some("not_affected".into()),
            Some("code_not_reachable".into())
        )
    );
    let mut unmatched = cves(&report.plan.unmatched);
    unmatched.sort();
    assert_eq!(
        unmatched,
        ["CVE-2023-45853", "CVE-2026-7777"],
        "GHSA 以 references 的 CVE 別名對應"
    );
    assert_eq!(finding_count(&w), findings, "未對應不新建命中");
}

#[test]
fn cve_bin_tool_sample_uses_cpe_and_name_version() {
    let mut w = vex_world();
    let report = run(&mut w, &vex_fixture("cve-bin-tool.json"), apply());
    let p = &report.plan;
    assert_eq!(p.apply.len(), 1, "{report:#?}");
    assert_eq!(
        (p.apply[0].item.vuln.as_str(), p.apply[0].item.method),
        (OPEN_CVE, Some(Method::Cpe))
    );
    assert_eq!(cves(&p.local_newer), [RISK_CVE]);
    assert_eq!(p.local_newer[0].method, Some(Method::NameVersion));
    assert_eq!(cves(&p.unmatched), ["CVE-2026-9999"]);
    assert_eq!(cves(&p.incomplete), [NOT_AFFECTED_CVE]);
    assert!(p.incomplete[0].note.contains("缺少理由"));
    assert_eq!(verdict(&w, FW, OPEN_CVE).0, "ignored");
    assert_eq!(
        verdict(&w, FW, RISK_CVE).1.as_deref(),
        Some("risk_accepted"),
        "本機較新不覆蓋"
    );
}

#[test]
fn unknown_cvewatch_bom_ref_does_not_fall_back_to_other_assets() {
    let mut w = vex_world();
    let doc = json!({
        "bomFormat": "CycloneDX", "specVersion": "1.6",
        "components": [{"bom-ref": "cvewatch:FW-99:paloalto-panos", "name": "PAN-OS", "version": "11.1.4-h7",
                        "cpe": "cpe:2.3:o:paloaltonetworks:pan-os:11.1.4-h7:*:*:*:*:*:*:*"}],
        "vulnerabilities": [{"id": OPEN_CVE, "analysis": {"state": "resolved"},
                             "affects": [{"ref": "cvewatch:FW-99:paloalto-panos"}]}]
    });
    let report = run_json(&mut w, &doc, apply());
    assert_eq!(report.plan.unmatched.len(), 1);
    assert_eq!(report.plan.unmatched[0].method, Some(Method::BomRef));
    assert_eq!(verdict(&w, FW, OPEN_CVE).0, "open");
}

#[test]
fn conflicting_statements_for_one_finding_are_not_applied() {
    let mut w = vex_world();
    let doc = json!({
        "bomFormat": "CycloneDX", "specVersion": "1.6",
        "vulnerabilities": [
            {"id": OPEN_CVE, "analysis": {"state": "resolved"}, "affects": [{"ref": "cvewatch:FW-01:paloalto-panos"}]},
            {"id": OPEN_CVE, "analysis": {"state": "false_positive"},
             "affects": [{"ref": "cpe:2.3:o:paloaltonetworks:pan-os:11.1.4-h7:*:*:*:*:*:*:*"}]}
        ]
    });
    let report = run_json(&mut w, &doc, apply());
    assert_eq!(report.plan.incomplete.len(), 1);
    assert!(report.plan.incomplete[0].note.contains("不同的判斷"));
    assert_eq!(verdict(&w, FW, OPEN_CVE).0, "open");
}

// ── Requirement: Import status mapping and validation ────────

#[test]
fn missing_analysis_or_justification_is_incomplete() {
    let mut w = vex_world();
    let doc = json!({
        "bomFormat": "CycloneDX", "specVersion": "1.4",
        "vulnerabilities": [
            {"id": OPEN_CVE, "affects": [{"ref": "cvewatch:FW-01:paloalto-panos"}]},
            {"id": OPEN_CVE, "analysis": {"state": "not_affected"},
             "affects": [{"ref": "cvewatch:FW-01:paloalto-panos"}]},
            {"id": OPEN_CVE, "analysis": {"state": "resolved"}}
        ]
    });
    let report = run_json(&mut w, &doc, apply());
    let notes: Vec<_> = report
        .plan
        .incomplete
        .iter()
        .map(|i| i.note.as_str())
        .collect();
    assert_eq!(notes.len(), 3);
    assert!(notes[0].contains("缺少 analysis"));
    assert!(notes[1].contains("缺少理由"));
    assert!(notes[2].contains("affects"));
    assert_eq!(verdict(&w, FW, OPEN_CVE).0, "open");
}

// ── Requirement: Import dry-run and audit trail ──────────────

#[test]
fn dry_run_reports_four_categories_without_writing() {
    let mut w = vex_world();
    let (events, findings) = (event_count(&w), finding_count(&w));
    let report = run(&mut w, &vex_fixture("cve-bin-tool.json"), dry_run());
    assert!(!report.applied);
    let p = &report.plan;
    assert_eq!(
        (
            p.apply.len(),
            p.unmatched.len(),
            p.local_newer.len(),
            p.incomplete.len()
        ),
        (1, 1, 1, 1)
    );
    assert_eq!(
        (event_count(&w), finding_count(&w)),
        (events, findings),
        "資料庫不變"
    );
    assert_eq!(verdict(&w, FW, OPEN_CVE).0, "open");
}

#[test]
fn applied_change_records_import_event_with_file_name() {
    let mut w = vex_world();
    run(&mut w, &vex_fixture("dependency-track.json"), apply());
    let (actor, to, note, file) = last_event(&w, WEB, PKG_CVE);
    assert_eq!((actor.as_str(), to.as_str()), ("import", "ignored"));
    assert_eq!(
        note,
        "dependency-track.json：主機上沒有程式呼叫 DH_generate_key"
    );
    assert_eq!(file.as_deref(), Some("dependency-track.json"));
}

#[test]
fn after_callback_receives_changed_findings_in_transaction() {
    let mut w = vex_world();
    let bom = import::read_file(&vex_fixture("dependency-track.json"), no_passphrase).unwrap();
    let seen = RefCell::new(Vec::new());
    import::import(
        &mut w.conn,
        &bom,
        "dt.json",
        apply(),
        at(IMPORTED),
        &|_, changed| {
            seen.borrow_mut()
                .extend(changed.iter().map(|(id, c)| (*id, c.status)));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        seen.into_inner(),
        [(finding_id(&w, WEB, PKG_CVE), Status::Ignored)]
    );
    let failing = import::import(
        &mut w.conn,
        &bom,
        "dt.json",
        force(),
        at(IMPORTED),
        &|_, _| Err(cvewatch_core::CoreError::Invalid("queue failed".into())),
    );
    // 已相同的項目不呼叫；沒有變更時不會失敗
    assert!(failing.is_ok());
}

#[test]
fn callback_failure_rolls_back_import() {
    let mut w = vex_world();
    let bom = import::read_file(&vex_fixture("cve-bin-tool.json"), no_passphrase).unwrap();
    let events = event_count(&w);
    let result = import::import(
        &mut w.conn,
        &bom,
        "x.json",
        apply(),
        at(IMPORTED),
        &|_, _| Err(cvewatch_core::CoreError::Invalid("queue failed".into())),
    );
    assert!(result.is_err());
    assert_eq!(event_count(&w), events);
    assert_eq!(verdict(&w, FW, OPEN_CVE).0, "open");
}

// ── Requirement: Automatic rules still apply after import ────

#[test]
fn imported_ignore_reopens_when_cve_enters_kev() {
    let mut w = vex_world();
    run(&mut w, &vex_fixture("cve-bin-tool.json"), apply());
    assert_eq!(verdict(&w, FW, OPEN_CVE).0, "ignored");
    w.kev(OPEN_CVE, "2026-11-06");
    w.run_at("2026-11-07T00:00:00+00:00");
    assert_eq!(verdict(&w, FW, OPEN_CVE).0, "open");
    let (actor, to, note, _) = last_event(&w, FW, OPEN_CVE);
    assert_eq!((actor.as_str(), to.as_str()), ("system", "open"));
    assert!(note.contains(NOTE_KEV_REOPEN));
}

// ── 檔案 ─────────────────────────────────────────────────────

#[test]
fn encrypted_file_asks_for_passphrase() {
    let w = vex_world();
    let dir = tempfile::tempdir().unwrap();
    let ids = finding_ids(&w.conn, &FindingFilter::default()).unwrap();
    let out = export_to_file(
        &w.conn,
        &ids,
        &fixed_meta(),
        &dir.path().join("a.json"),
        Some("pw"),
    )
    .unwrap();
    let asked = RefCell::new(0);
    let bom = import::read_file(&out.path, || {
        *asked.borrow_mut() += 1;
        Ok(Zeroizing::new("pw".to_string()))
    })
    .unwrap();
    assert_eq!((*asked.borrow(), bom.vulnerabilities.len()), (1, 5));
    let wrong = import::read_file(&out.path, || Ok(Zeroizing::new("nope".to_string())));
    assert!(wrong.unwrap_err().to_string().contains("口令錯誤"));
    assert_eq!(import::file_name(&out.path), "a.json.age");
}

#[test]
fn unreadable_or_invalid_files_fail_with_clear_message() {
    let dir = tempfile::tempdir().unwrap();
    let missing = import::read_file(&dir.path().join("none.json"), no_passphrase).unwrap_err();
    assert!(missing.to_string().contains("無法讀取"));
    let path = dir.path().join("spdx.json");
    std::fs::write(&path, r#"{"spdxVersion":"SPDX-2.3"}"#).unwrap();
    let err = import::read_file(&path, no_passphrase).unwrap_err();
    assert!(err.to_string().contains("CycloneDX"));
}

#[test]
fn not_affected_detail_from_export_is_note_text() {
    let w = vex_world();
    let v = export_value(&w);
    assert!(v.to_string().contains(NOTE_NOT_AFFECTED));
}

#[test]
fn import_events_are_labelled_in_timeline() {
    use cvewatch_core::matching::query;
    let mut w = vex_world();
    run(&mut w, &vex_fixture("dependency-track.json"), apply());
    let detail = query::show(&w.conn, finding_id(&w, WEB, PKG_CVE))
        .unwrap()
        .unwrap();
    let last = detail.events.last().unwrap();
    assert_eq!(query::actor_label(&last.actor), "匯入");
    assert_eq!(query::actor_label("user"), "使用者");
    assert_eq!(query::actor_label("system"), "系統");
}

// ── code review 第 1 輪 ──────────────────────────────────────

fn single(cve: &str, reference: &str, analysis: Value) -> Value {
    json!({
        "bomFormat": "CycloneDX", "specVersion": "1.6",
        "vulnerabilities": [{"id": cve, "analysis": analysis, "affects": [{"ref": reference}]}]
    })
}

const FW_REF: &str = "cvewatch:FW-01:paloalto-panos";

#[test]
fn untriaged_peer_export_does_not_reopen_local_decisions() {
    let mut w = vex_world();
    // 同事在本機判定之後才同步、從未處理：in_triage 且 lastUpdated 較新
    let doc = single(
        NOT_AFFECTED_CVE,
        FW_REF,
        json!({"state": "in_triage", "lastUpdated": "2026-11-05T00:00:00Z"}),
    );
    let report = run_json(&mut w, &doc, apply());
    assert_eq!(cves(&report.plan.local_newer), [NOT_AFFECTED_CVE]);
    assert!(report.plan.local_newer[0].note.contains("in_triage"));
    assert_eq!(verdict(&w, FW, NOT_AFFECTED_CVE).0, "ignored");
    let forced = run_json(&mut w, &doc, force());
    assert_eq!(forced.plan.apply.len(), 1, "--force 仍可覆蓋");
    assert_eq!(verdict(&w, FW, NOT_AFFECTED_CVE).0, "open");
}

#[test]
fn imported_decision_keeps_source_time_for_later_conflicts() {
    let mut w = vex_world();
    let older = single(
        OPEN_CVE,
        FW_REF,
        json!({"state": "resolved", "lastUpdated": "2026-10-01T00:00:00Z"}),
    );
    run_json(&mut w, &older, apply());
    assert_eq!(verdict(&w, FW, OPEN_CVE).0, "resolved");
    // 另一份在 10-15 判定、比匯入的 10-01 新，即使匯入發生在 11-06 也應套用
    let newer = single(
        OPEN_CVE,
        FW_REF,
        json!({"state": "false_positive", "lastUpdated": "2026-10-15T00:00:00Z"}),
    );
    let report = run_json(&mut w, &newer, apply());
    assert_eq!(report.plan.apply.len(), 1, "{report:#?}");
    let exported = export_value(&w);
    let v = exported["vulnerabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == OPEN_CVE)
        .unwrap();
    assert_eq!(
        v["analysis"]["lastUpdated"], "2026-10-15T00:00:00Z",
        "再匯出沿用來源判定時間"
    );
    // 更舊的檔案不覆蓋
    let report = run_json(&mut w, &older, apply());
    assert_eq!(cves(&report.plan.local_newer), [OPEN_CVE]);
}

#[test]
fn detail_only_update_is_applied_without_notification() {
    let mut w = vex_world();
    let doc = single(
        NOT_AFFECTED_CVE,
        FW_REF,
        json!({"state": "not_affected", "justification": "requires_configuration",
               "detail": "改由 WAF 阻擋", "lastUpdated": "2026-11-05T00:00:00Z"}),
    );
    let bom = import::doc::parse(doc.to_string().as_bytes()).unwrap();
    let notified = RefCell::new(0);
    let report = import::import(
        &mut w.conn,
        &bom,
        "d.json",
        apply(),
        at(IMPORTED),
        &|_, _| {
            *notified.borrow_mut() += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(report.plan.apply.len(), 1);
    assert_eq!(*notified.borrow(), 0, "狀態未變更不通知");
    let (actor, _, note, _) = last_event(&w, FW, NOT_AFFECTED_CVE);
    assert_eq!(
        (actor.as_str(), note.as_str()),
        ("import", "d.json：改由 WAF 阻擋")
    );
}

#[test]
fn bom_ref_asset_tag_prefers_exact_case_and_rejects_ambiguity() {
    let mut w = vex_world();
    w.asset("fw-01", "internal", &[("paloalto-panos", "11.1.4-h7")]);
    w.run_at("2026-11-01T12:00:00+00:00");
    let doc = single(OPEN_CVE, FW_REF, json!({"state": "resolved"}));
    let report = run_json(&mut w, &doc, apply());
    assert_eq!(report.plan.apply.len(), 1);
    assert_eq!(report.plan.apply[0].item.target, "FW-01:paloalto-panos");
    assert_eq!(
        verdict(&w, "fw-01", OPEN_CVE).0,
        "open",
        "大小寫不同的資產不受影響"
    );
    let ambiguous = single(
        RISK_CVE,
        "cvewatch:Fw-01:paloalto-panos",
        json!({"state": "resolved"}),
    );
    let report = run_json(&mut w, &ambiguous, apply());
    assert_eq!(report.plan.unmatched.len(), 1, "對應到多個資產時不套用");
}

#[test]
fn bom_ref_asset_tag_case_is_tolerated_when_unambiguous() {
    let mut w = vex_world();
    let doc = single(
        OPEN_CVE,
        "cvewatch:fw-01:paloalto-panos",
        json!({"state": "resolved"}),
    );
    let report = run_json(&mut w, &doc, apply());
    assert_eq!(report.plan.apply.len(), 1);
    assert_eq!(verdict(&w, FW, OPEN_CVE).0, "resolved");
}

// ── code review 第 2 輪 ──────────────────────────────────────

#[test]
fn kev_reopen_after_export_keeps_finding_open_on_reimport() {
    let mut w = vex_world();
    let dir = tempfile::tempdir().unwrap();
    let path = export_file(&w, dir.path());
    // 匯出後 CVE 列入 KEV，比對自動重新開啟被忽略的命中
    w.kev(NOT_AFFECTED_CVE, "2026-11-04");
    w.run_at("2026-11-05T00:00:00+00:00");
    assert_eq!(verdict(&w, FW, NOT_AFFECTED_CVE).0, "open");
    let report = run(&mut w, &path, apply());
    assert!(cves(&report.plan.local_newer).contains(&NOT_AFFECTED_CVE));
    assert_eq!(
        verdict(&w, FW, NOT_AFFECTED_CVE).0,
        "open",
        "不以舊檔再次忽略 KEV 命中"
    );
}

#[test]
fn forced_older_import_does_not_block_files_newer_than_it() {
    let mut w = vex_world();
    // 本機 11-02 判定為已處理；以 --force 匯入 10-20 的判定後，10-25 的檔案應可套用
    let old = single(
        RESOLVED_CVE,
        FW_REF,
        json!({"state": "false_positive", "lastUpdated": "2026-10-20T00:00:00Z"}),
    );
    run_json(&mut w, &old, force());
    let newer = single(
        RESOLVED_CVE,
        FW_REF,
        json!({"state": "resolved", "lastUpdated": "2026-10-25T00:00:00Z"}),
    );
    let report = run_json(&mut w, &newer, apply());
    assert_eq!(report.plan.apply.len(), 1, "{report:#?}");
}

#[test]
fn undated_import_does_not_block_dated_files() {
    let mut w = vex_world();
    let undated = single(OPEN_CVE, FW_REF, json!({"state": "resolved"}));
    run_json(&mut w, &undated, apply());
    let dated = single(
        OPEN_CVE,
        FW_REF,
        json!({"state": "false_positive", "lastUpdated": "2026-10-01T00:00:00Z"}),
    );
    let report = run_json(&mut w, &dated, apply());
    assert_eq!(report.plan.apply.len(), 1, "時間未知的匯入判定視為最舊");
}

#[test]
fn redmine_notes_are_one_per_cve_across_assets() {
    use cvewatch_core::notify::settings::NotifyConfig;
    use cvewatch_core::notify::triage_notes;
    let mut w = vex_world();
    w.asset("FW-02", "internal", &[("paloalto-panos", "11.1.4-h7")]);
    w.run_at("2026-11-01T12:00:00+00:00");
    w.conn
        .execute(
            "INSERT INTO redmine_issue (cve_id, issue_id, created_at) VALUES (?1, 7, ?2)",
            rusqlite::params![OPEN_CVE, IMPORTED],
        )
        .unwrap();
    let mut cfg = NotifyConfig::default();
    cfg.redmine.enabled = true;
    cfg.redmine.add_note_on_triage = true;
    let doc = json!({
        "bomFormat": "CycloneDX", "specVersion": "1.6",
        "vulnerabilities": [
            {"id": OPEN_CVE, "analysis": {"state": "resolved", "detail": "已升級"},
             "affects": [{"ref": "cvewatch:FW-01:paloalto-panos"}]},
            {"id": OPEN_CVE, "analysis": {"state": "false_positive", "detail": "誤判"},
             "affects": [{"ref": "cvewatch:FW-02:paloalto-panos"}]}
        ]
    });
    let bom = import::doc::parse(doc.to_string().as_bytes()).unwrap();
    let now = at(IMPORTED);
    import::import(&mut w.conn, &bom, "n.json", apply(), now, &|tx, changed| {
        triage_notes::queue_notes_each(tx, &cfg, changed, now).map(|_| ())
    })
    .unwrap();
    let notes: Vec<String> = w
        .conn
        .prepare("SELECT payload_json FROM outbox WHERE channel = 'redmine'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(notes.len(), 1, "一個 CVE 一則註記：{notes:?}");
    assert!(notes[0].contains("FW-01") && notes[0].contains("FW-02"));
}

// ── code review 第 3 輪 ──────────────────────────────────────

fn dismissed(w: &World, asset: &str, cve: &str) -> bool {
    w.conn
        .query_row(
            "SELECT dismissed FROM finding WHERE id = ?1",
            [finding_id(w, asset, cve)],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn detail_only_update_keeps_pending_dismissal_flag() {
    let mut w = vex_world();
    w.conn
        .execute(
            "UPDATE finding SET status = 'ignored', ignore_reason = 'false_positive', dismissed = 1
             WHERE id = ?1",
            [finding_id(&w, FW, OPEN_CVE)],
        )
        .unwrap();
    let same = single(
        OPEN_CVE,
        FW_REF,
        json!({"state": "false_positive", "detail": "等待區已判定不相關", "lastUpdated": "2026-11-05T00:00:00Z"}),
    );
    assert_eq!(run_json(&mut w, &same, apply()).plan.apply.len(), 1);
    assert!(dismissed(&w, FW, OPEN_CVE), "只更新 detail 時保留標記");
    let changed = single(
        OPEN_CVE,
        FW_REF,
        json!({"state": "resolved", "lastUpdated": "2026-11-05T00:00:00Z"}),
    );
    run_json(&mut w, &changed, apply());
    assert!(
        !dismissed(&w, FW, OPEN_CVE),
        "判定改變時與手動變更相同，清除標記"
    );
}

#[test]
fn future_dated_file_time_is_treated_as_unknown() {
    let mut w = vex_world();
    let future = single(
        OPEN_CVE,
        FW_REF,
        json!({"state": "resolved", "lastUpdated": "2027-01-01T00:00:00Z"}),
    );
    run_json(&mut w, &future, apply());
    let decided: Option<String> = w
        .conn
        .query_row(
            "SELECT decided_at FROM finding_event WHERE finding_id = ?1 ORDER BY id DESC LIMIT 1",
            [finding_id(&w, FW, OPEN_CVE)],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(decided, None, "未來時間視為未知");
    // 匯入之後使用者的變更優先於同一份（時間錯誤的）檔案
    set(
        &w,
        FW,
        OPEN_CVE,
        req("open", None, None, "重新評估"),
        "2026-11-07T00:00:00+00:00",
    );
    let report = run_json(&mut w, &future, apply());
    assert_eq!(cves(&report.plan.local_newer), [OPEN_CVE]);
}
