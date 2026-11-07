//! 04 任務 3.5：等待區人工判定不相關（`pending resolve --dismiss`）與之後的命中。

mod common;

use common::matching::{affected, at, world_with, World, T0};
use cvewatch_core::matching::pending_ops::{self, Action};
use serde_json::json;

const CVE: &str = "CVE-2099-20001";
const THREE_DAYS_LATER: &str = "2026-11-04T00:00:00+00:00";

/// 種子目錄沒有 Proxmox 的 CNA 寫法，以 catalog.d 補上（使用者實際的做法）。
const PROXMOX_CNA: &str = r#"
[[product]]
key = "proxmox-ve"
cna = [{ vendor = "Proxmox Server Solutions GmbH", product = "Proxmox VE" }]
"#;

fn proxmox_world() -> (World, i64) {
    let w = world_with(Some(PROXMOX_CNA));
    let c = w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.2")])[0];
    w.cve(
        CVE,
        json!([affected(
            "Proxmox Server Solutions GmbH",
            "Proxmox VE",
            None,
            json!([])
        )]),
        json!([]),
    );
    (w, c)
}

#[test]
fn dismissed_pending_cve_is_not_requeued() {
    let (mut w, c) = proxmox_world();
    w.run();
    let scoring = w.settings.scoring.clone();
    let resolution = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Dismiss,
        "非本機使用的功能",
        at(T0),
    )
    .unwrap();
    // code review #4：系統建立的 open probable 命中一併轉為忽略並記錄使用者事件
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(resolution.ignored, vec![f.id]);
    assert_eq!(
        (f.status.as_str(), f.ignore_reason.as_deref()),
        ("ignored", Some("false_positive"))
    );
    let (actor, from, to, note) = w.events(f.id).pop().unwrap();
    assert_eq!(
        (actor.as_str(), from.as_deref(), to.as_str()),
        ("user", Some("open"), "ignored")
    );
    assert!(
        note.contains("人工判定不相關") && note.contains("非本機使用的功能"),
        "{note}"
    );
    w.run_at(THREE_DAYS_LATER);
    assert_eq!(w.pending(CVE).unwrap().2.as_deref(), Some("manual_dismiss"));
    assert!(w.pending_list(false).is_empty());
    assert_eq!(
        w.finding(c, CVE).unwrap().status,
        "ignored",
        "重算後維持忽略"
    );
}

#[test]
fn dismiss_leaves_manual_and_handled_findings_alone() {
    let (mut w, c) = proxmox_world();
    let manual = w.asset("PVE-02", "internal", &[("proxmox-ve", "8.1.0")])[0];
    let handled = w.asset("PVE-03", "internal", &[("proxmox-ve", "8.0.0")])[0];
    w.run();
    w.conn
        .execute(
            "UPDATE finding SET manual = 1 WHERE component_id = ?1",
            [manual],
        )
        .unwrap();
    w.conn
        .execute(
            "UPDATE finding SET status = 'resolved' WHERE component_id = ?1",
            [handled],
        )
        .unwrap();
    let scoring = w.settings.scoring.clone();
    let resolution = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Dismiss,
        "",
        at(T0),
    )
    .unwrap();
    assert_eq!(resolution.ignored, vec![w.finding(c, CVE).unwrap().id]);
    assert_eq!(w.finding(manual, CVE).unwrap().status, "open");
    assert_eq!(w.finding(handled, CVE).unwrap().status, "resolved");
}

/// code review 第四輪 #4：dismiss 之後新出現的 probable 命中直接為忽略（系統事件說明原因）；
/// 資料確定受影響時仍建立未處理的命中。
#[test]
fn dismissed_cve_new_probable_findings_are_ignored() {
    use cvewatch_core::matching::findings::NOTE_DISMISSED;

    let (mut w, _) = proxmox_world();
    w.run();
    let scoring = w.settings.scoring.clone();
    pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Dismiss,
        "",
        at(T0),
    )
    .unwrap();
    let late = w.asset("PVE-09", "internal", &[("proxmox-ve", "8.0.1")])[0];
    w.run_at(THREE_DAYS_LATER);
    let f = w.finding(late, CVE).unwrap();
    assert_eq!(
        (f.status.as_str(), f.ignore_reason.as_deref()),
        ("ignored", Some("false_positive"))
    );
    let (actor, from, to, note) = w.events(f.id).pop().unwrap();
    assert_eq!(
        (actor.as_str(), from, to.as_str(), note.as_str()),
        ("system", None, "ignored", NOTE_DISMISSED)
    );
    // 資料確定受影響 → 新元件的命中照常為未處理
    w.nvd(
        CVE,
        &[(
            "cpe:2.3:a:proxmox:virtual_environment:*:*:*:*:*:*:*:*",
            true,
            None,
            Some("8.2.4"),
        )],
    );
    let confirmed = w.asset("PVE-10", "internal", &[("proxmox-ve", "8.2.0")])[0];
    w.run_at(THREE_DAYS_LATER);
    assert_eq!(w.finding(confirmed, CVE).unwrap().status, "open");
}

/// code review 第四輪 #5：dismiss 同步 KEV 狀態（之後不會被當成新列入而重開），且不動退役資產的命中。
#[test]
fn dismiss_syncs_kev_and_covers_retired_assets() {
    let (mut w, c) = proxmox_world();
    let retired = w.asset("PVE-OLD", "internal", &[("proxmox-ve", "7.4.0")])[0];
    w.run();
    w.retire("PVE-OLD");
    w.kev(CVE, "2026-11-02");
    let scoring = w.settings.scoring.clone();
    let resolution = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Dismiss,
        "",
        at(T0),
    )
    .unwrap();
    // 第十三輪 #6：退役資產的命中也一併忽略（之後重新啟用時已是忽略）
    let mut expected = vec![
        w.finding(c, CVE).unwrap().id,
        w.finding(retired, CVE).unwrap().id,
    ];
    expected.sort_unstable();
    assert_eq!(resolution.ignored, expected);
    assert_eq!(w.finding(retired, CVE).unwrap().status, "ignored");
    let report = w.run_at(THREE_DAYS_LATER);
    assert_eq!(report.findings.reopened, 0);
    assert_eq!(w.finding(c, CVE).unwrap().status, "ignored");
}

/// code review 第五輪 #1：因 dismiss 而忽略的命中，資料之後確定受影響時重新開啟；
/// 使用者自己設定的忽略不受此規則影響。
#[test]
fn dismissed_finding_reopens_when_confirmed() {
    use cvewatch_core::matching::findings::NOTE_CONFIRMED_AFTER_DISMISS;
    use cvewatch_core::matching::triage::{self, ChangeRequest};

    let (mut w, c) = proxmox_world();
    let user_ignored = w.asset("PVE-02", "internal", &[("proxmox-ve", "8.2.1")])[0];
    w.run();
    let scoring = w.settings.scoring.clone();
    pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Dismiss,
        "",
        at(T0),
    )
    .unwrap();
    // 使用者另外明確忽略 PVE-02（接受風險）
    let change = triage::validate(&ChangeRequest {
        status: "ignored".into(),
        reason: Some("risk_accepted".into()),
        justification: None,
        note: "已評估".into(),
    })
    .unwrap();
    triage::set_status(
        &w.conn,
        w.finding(user_ignored, CVE).unwrap().id,
        &change,
        at(T0),
    )
    .unwrap();
    w.nvd(
        CVE,
        &[(
            "cpe:2.3:a:proxmox:virtual_environment:*:*:*:*:*:*:*:*",
            true,
            None,
            Some("8.2.4"),
        )],
    );
    w.run_at(THREE_DAYS_LATER);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (f.status.as_str(), f.confidence.as_str(), f.ignore_reason),
        ("open", "confirmed", None)
    );
    assert_eq!(
        w.events(f.id).pop().unwrap().3,
        NOTE_CONFIRMED_AFTER_DISMISS
    );
    assert_eq!(w.finding(user_ignored, CVE).unwrap().status, "ignored");
}

/// code review 第九輪 #3、第十三輪 #6：dismiss 當下就忽略已移除元件的命中，元件恢復後維持忽略。
#[test]
fn dismissal_applies_to_findings_of_restored_components() {
    use cvewatch_core::assets::repo::ComponentRepo;
    let (mut w, c) = proxmox_world();
    w.run();
    ComponentRepo::new(&w.conn).remove(c, at(T0)).unwrap();
    let scoring = w.settings.scoring.clone();
    let resolution = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Dismiss,
        "",
        at("2026-11-02T00:00:00+00:00"),
    )
    .unwrap();
    assert_eq!(resolution.ignored, vec![w.finding(c, CVE).unwrap().id]);
    w.conn
        .execute("UPDATE component SET removed_at = NULL WHERE id = ?1", [c])
        .unwrap();
    w.run_at(THREE_DAYS_LATER);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (f.status.as_str(), f.ignore_reason.as_deref()),
        ("ignored", Some("false_positive"))
    );
    let (actor, _, to, note) = w.events(f.id).pop().unwrap();
    assert_eq!((actor.as_str(), to.as_str()), ("user", "ignored"));
    assert!(note.contains("人工判定不相關"), "{note}");
}

/// code review 第十輪 #1：dismiss 之後使用者重新開啟的命中，下次比對不會被改回忽略。
#[test]
fn user_reopen_after_dismiss_is_kept() {
    use cvewatch_core::matching::triage::{self, ChangeRequest};

    let (mut w, c) = proxmox_world();
    w.run();
    let scoring = w.settings.scoring.clone();
    pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Dismiss,
        "",
        at("2026-11-02T00:00:00+00:00"),
    )
    .unwrap();
    let id = w.finding(c, CVE).unwrap().id;
    let change = triage::validate(&ChangeRequest {
        status: "open".into(),
        note: "仍要追蹤".into(),
        ..ChangeRequest::default()
    })
    .unwrap();
    triage::set_status(&w.conn, id, &change, at("2026-11-02T12:00:00+00:00")).unwrap();
    w.run_at(THREE_DAYS_LATER);
    w.run_at(THREE_DAYS_LATER);
    assert_eq!(w.finding(c, CVE).unwrap().status, "open");
}

/// code review 第十輪 #2：dismiss 之後才列入 KEV 的 CVE，新出現的 probable 命中先忽略、再依 KEV 規則重新開啟。
#[test]
fn kev_after_dismissal_reopens_new_findings() {
    let (mut w, _) = proxmox_world();
    w.run();
    let scoring = w.settings.scoring.clone();
    pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Dismiss,
        "",
        at(T0),
    )
    .unwrap();
    w.kev(CVE, "2026-11-03");
    let late = w.asset("PVE-09", "internal", &[("proxmox-ve", "8.0.1")])[0];
    w.run_at(THREE_DAYS_LATER);
    // 判定不相關之後才列入 KEV → 新命中直接為未處理（不先忽略再等下一輪重開）
    let f = w.finding(late, CVE).unwrap();
    assert_eq!(f.status, "open");
    assert!(w.events(f.id)[0].3.contains("已列入 KEV"));
    // 判定前就已在 KEV → 新命中照樣忽略
    let (mut w2, _) = proxmox_world();
    w2.kev(CVE, "2026-10-01");
    w2.run();
    let scoring2 = w2.settings.scoring.clone();
    pending_ops::resolve(
        &mut w2.conn,
        &w2.catalog,
        &scoring2,
        CVE,
        &Action::Dismiss,
        "",
        at(T0),
    )
    .unwrap();
    let late2 = w2.asset("PVE-09", "internal", &[("proxmox-ve", "8.0.1")])[0];
    w2.run_at(THREE_DAYS_LATER);
    assert_eq!(w2.finding(late2, CVE).unwrap().status, "ignored");
}

/// code review 第十二輪 #1、#2：恢復的元件帶回的命中，若 CVE 在判定不相關之後（含同一天）才列入 KEV，維持未處理。
#[test]
fn restored_finding_stays_open_when_kev_listed_after_dismissal() {
    use cvewatch_core::assets::repo::ComponentRepo;

    let (mut w, c) = proxmox_world();
    w.run();
    ComponentRepo::new(&w.conn).remove(c, at(T0)).unwrap();
    let scoring = w.settings.scoring.clone();
    let dismissed_at = "2026-11-02T01:00:00+00:00";
    pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Dismiss,
        "",
        at(dismissed_at),
    )
    .unwrap();
    w.kev(CVE, "2026-11-02"); // 同一天
    w.conn
        .execute("UPDATE component SET removed_at = NULL WHERE id = ?1", [c])
        .unwrap();
    w.run_at(THREE_DAYS_LATER);
    assert_eq!(w.finding(c, CVE).unwrap().status, "open");
}

/// code review 第十八輪 #1：判定不相關時本機還沒有 KEV 紀錄、之後同步進來（date_added 早於判定日）：
/// 既有的忽略命中與新命中都依「新列入 KEV」重新開啟，結果一致。
#[test]
fn kev_synced_after_dismissal_reopens_old_and_new_findings_alike() {
    let (mut w, c) = proxmox_world();
    w.run();
    let scoring = w.settings.scoring.clone();
    pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Dismiss,
        "",
        at(T0),
    )
    .unwrap();
    assert_eq!(w.finding(c, CVE).unwrap().status, "ignored");
    w.kev(CVE, "2026-10-01");
    let late = w.asset("PVE-09", "internal", &[("proxmox-ve", "8.0.1")])[0];
    w.run_at(THREE_DAYS_LATER);
    assert_eq!(w.finding(c, CVE).unwrap().status, "open");
    assert_eq!(w.finding(late, CVE).unwrap().status, "open");
}
