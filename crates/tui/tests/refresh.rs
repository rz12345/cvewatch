//! 05 任務 5.4：新資料提示（不自動重排）、`R` 重新載入、`S` 背景 sync 與結束碼 3 的提示。

mod support;

use std::time::{Duration, Instant};

use cvewatch_core::matching::run::{run_match, MatchSettings};
use cvewatch_tui::app::NEW_DATA_POLL;
use rusqlite::params;
use support::common::matching::at;
use support::{app, insert_run, keys, render, seeded_world};

#[test]
fn scheduled_sync_shows_hint_but_keeps_list_until_r() {
    let (mut app, _) = app(seeded_world());
    keys(&mut app, "2j");
    let start = Instant::now();
    app.on_tick(start);
    assert!(!app.new_data);
    let before = app.findings.visible.clone();
    let cursor = app.findings.cursor;
    // 排程 sync 完成：新命中與一筆結束的 sync_run
    app.conn
        .execute(
            "UPDATE finding SET priority = 'P4' WHERE cve_id = 'CVE-2026-0001'",
            params![],
        )
        .unwrap();
    insert_run(
        &app.conn,
        "kev",
        "2026-11-07T01:00:00+00:00",
        "success",
        None,
    );
    app.on_tick(start + Duration::from_secs(1));
    assert!(!app.new_data, "5 秒內不重複查詢");
    app.on_tick(start + NEW_DATA_POLL);
    assert!(app.new_data);
    assert_eq!(app.findings.visible, before, "清單不自動重排");
    assert_eq!(app.findings.cursor, cursor);
    let out = render(&app);
    assert!(out.contains("有新資料，按 R 重新整理"), "{out}");
    keys(&mut app, "R");
    assert!(!app.new_data);
    assert_eq!(app.message.as_deref(), Some("已重新載入"));
    let first = &app.data.findings[app.findings.visible[0]];
    assert_ne!(first.cve_id, "CVE-2026-0001", "重新載入後依新資料排序");
}

#[test]
fn running_sync_rows_do_not_count_as_new_data_until_finished() {
    let (mut app, _) = app(seeded_world());
    let t = Instant::now();
    app.on_tick(t);
    app.conn
        .execute(
            "INSERT INTO sync_run (started_at, source, status) VALUES ('2026-11-07T01:00:00+00:00', 'nvd', 'running')",
            params![],
        )
        .unwrap();
    app.on_tick(t + NEW_DATA_POLL);
    assert!(!app.new_data);
}

#[test]
fn background_sync_already_running_exit_code_3() {
    let (mut app, rec) = app(seeded_world());
    keys(&mut app, "S");
    assert_eq!(rec.launches.get(), 1);
    assert!(app.sync.is_some());
    assert!(render(&app).contains("sync 執行中"));
    keys(&mut app, "S");
    assert_eq!(rec.launches.get(), 1, "執行中不重複啟動");
    assert_eq!(app.message.as_deref(), Some("背景 sync 仍在執行"));
    *rec.exit.borrow_mut() = Some(Some(3));
    app.on_tick(Instant::now());
    assert!(app.sync.is_none());
    let msg = app.message.clone().unwrap();
    assert!(msg.contains("已有 sync 正在執行"), "{msg}");
    insta::assert_snapshot!(render(&app));
}

#[test]
fn background_sync_success_then_new_data() {
    let (mut app, rec) = app(seeded_world());
    let t = Instant::now();
    app.on_tick(t);
    keys(&mut app, "S");
    insert_run(
        &app.conn,
        "cvelist",
        "2026-11-07T02:00:00+00:00",
        "success",
        None,
    );
    *rec.exit.borrow_mut() = Some(Some(0));
    app.on_tick(t + NEW_DATA_POLL);
    assert_eq!(app.message.as_deref(), Some("背景 sync 完成"));
    assert!(app.new_data);
}

#[test]
fn rematch_after_reload_is_detected() {
    let (mut app, _) = app(seeded_world());
    let t = Instant::now();
    app.on_tick(t);
    // 排程 sync 的最後一個來源結束 → 提示 → 使用者按 R
    insert_run(
        &app.conn,
        "osv",
        "2026-11-07T01:00:00+00:00",
        "success",
        None,
    );
    app.on_tick(t + NEW_DATA_POLL);
    assert!(app.new_data);
    keys(&mut app, "R");
    assert!(!app.new_data);
    // 之後 sync 結尾的自動比對才完成（不寫 sync_run）
    run_match(
        &mut app.ws.conn,
        &app.ws.settings.catalog,
        &MatchSettings::default(),
        at("2026-11-07T01:01:00+00:00"),
    )
    .unwrap();
    app.on_tick(t + NEW_DATA_POLL * 2);
    assert!(app.new_data, "比對完成也算新資料");
}

#[test]
fn own_sync_exit_checks_new_data_immediately() {
    let (mut app, rec) = app(seeded_world());
    let t = Instant::now();
    app.on_tick(t);
    keys(&mut app, "S");
    insert_run(
        &app.conn,
        "kev",
        "2026-11-07T01:00:00+00:00",
        "success",
        None,
    );
    *rec.exit.borrow_mut() = Some(Some(0));
    app.on_tick(t + Duration::from_secs(1));
    assert!(app.new_data, "不等下一次 5 秒輪詢");
}
