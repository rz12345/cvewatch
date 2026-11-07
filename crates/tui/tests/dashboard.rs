//! 05 任務 2.3：Dashboard 與 Sync log 快照（含一個失敗來源）。

mod support;

use support::{app, keys, render, seeded_world};

#[test]
fn dashboard_shows_counts_and_failed_source() {
    let (app, _) = app(seeded_world());
    let d = &app.data.dashboard;
    let osv = d.sources.iter().find(|s| s.source == "osv").expect("osv");
    assert!(osv.failed());
    assert_eq!(d.pending, 1);
    assert!(d.kev_open >= 1);
    let out = render(&app);
    assert!(out.contains("osv"), "{out}");
    assert!(
        out.contains("✗ 失敗  11-06 22:10"),
        "OSV 標示失敗並顯示時間：\n{out}"
    );
    insta::assert_snapshot!(out);
}

#[test]
fn sync_log_lists_recent_runs_newest_first() {
    let (mut app, _) = app(seeded_world());
    keys(&mut app, "5");
    assert_eq!(app.data.sync_runs[0].source, "osv");
    insta::assert_snapshot!(render(&app));
}

#[test]
fn header_flags_earlier_failure_even_if_last_source_succeeded() {
    let w = seeded_world();
    support::sync_run(&w, "kev", "2026-11-06T22:20:00+00:00", "success", None);
    let (app, _) = app(w);
    assert_eq!(app.data.dashboard.last_finished().unwrap().source, "kev");
    let out = render(&app);
    assert!(
        out.contains("last sync 11-06 22:20 ✗ 1 個來源失敗"),
        "{out}"
    );
}
