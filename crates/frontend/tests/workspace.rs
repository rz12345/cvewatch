//! 不經任何 UI 的 `Workspace` 測試：寫入的事件、操作報告與外部效果。
#![allow(dead_code)]

#[path = "../../core/tests/common/mod.rs"]
mod common;
mod shared;

use std::time::Instant;

use common::matching::{at, world_with, World};
use cvewatch_core::matching::engine::Confidence;
use cvewatch_core::matching::pending_ops::Action;
use cvewatch_core::matching::triage::{IgnoreReason, Status};
use cvewatch_frontend::data::Settings;
use cvewatch_frontend::test_support::{self, Recorder};
use cvewatch_frontend::workspace::{DetailRefresh, TriageRequest};
use cvewatch_frontend::{Context, Hint, Level, Workspace};
use shared::{keyword_only_pending, panos_cves, PENDING_CVE, PROXMOX_CNA};

const NOW: &str = "2026-11-07T00:00:00+00:00";

/// FW-EDGE（三筆 PAN-OS 命中）與 PVE-01、一筆僅關鍵字相符的等待區 CVE（與 TUI 測試共用片段）。
fn world() -> World {
    let mut w = world_with(Some(PROXMOX_CNA));
    w.asset("FW-EDGE", "internet", &[("paloalto-panos", "11.1.2")]);
    w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.2")]);
    panos_cves(&w, false);
    keyword_only_pending(&w);
    w.run_at("2026-11-01T00:00:00+00:00");
    w
}

fn open(w: World) -> (Workspace, Recorder) {
    let rec = Recorder::default();
    let ctx = Context {
        conn: w.conn,
        settings: Settings {
            catalog: w.catalog,
            classes: w.classes,
            scoring: w.settings.scoring.clone(),
            stale_days: w.settings.stale_days,
            reminder_days: cvewatch_core::lifecycle::DEFAULT_REMINDER_DAYS,
        },
        config: cvewatch_core::config::Config::default(),
        config_path: rec.config_path(),
        services: test_support::services(&rec, at(NOW)),
    };
    (Workspace::new(ctx).expect("workspace"), rec)
}

fn event_count(ws: &Workspace, id: i64) -> i64 {
    ws.conn
        .query_row(
            "SELECT COUNT(*) FROM finding_event WHERE finding_id = ?1",
            [id],
            |r| r.get(0),
        )
        .expect("events")
}

fn request(targets: Vec<i64>, status: Status) -> TriageRequest {
    TriageRequest {
        targets,
        status,
        reason: (status == Status::Ignored).then_some(IgnoreReason::RiskAccepted),
        justification: None,
        note: "測試".into(),
    }
}

#[test]
fn batch_ignore_writes_one_event_per_finding() {
    let (mut ws, _rec) = open(world());
    let ids = ws.visible_ids();
    assert_eq!(ids.len(), 3);
    let before: Vec<i64> = ids.iter().map(|id| event_count(&ws, *id)).collect();
    for id in &ids {
        ws.findings.toggle(*id);
    }
    let targets = ws.list_targets(None).expect("targets");
    let r = ws.set_status(request(targets, Status::Ignored), &[]);
    assert_eq!(r.report.level, Level::Info);
    assert_eq!(r.report.message, "已將 3 筆命中設為忽略（接受風險）");
    for (id, n) in ids.iter().zip(before) {
        assert_eq!(event_count(&ws, *id), n + 1, "命中 #{id}");
    }
    assert!(ws.findings.selected.is_empty(), "處理過的命中移出多選");
    assert!(ws.findings.visible.is_empty(), "預設篩選只看未處理");
}

#[test]
fn same_status_writes_no_event_and_says_unchanged() {
    let (mut ws, _rec) = open(world());
    let ids = ws.visible_ids();
    let before: Vec<i64> = ids.iter().map(|id| event_count(&ws, *id)).collect();
    let r = ws.set_status(request(ids.clone(), Status::Open), &[]);
    assert_eq!(r.report.message, "3 筆命中原本就是未處理，未變更");
    let after: Vec<i64> = ids.iter().map(|id| event_count(&ws, *id)).collect();
    assert_eq!(after, before);
}

#[test]
fn hidden_selection_is_reported_not_changed() {
    let (mut ws, _rec) = open(world());
    let id = ws.visible_ids()[0];
    ws.findings.toggle(id);
    ws.findings.search = "no-such-finding".into();
    ws.refresh_visible();
    let r = ws.list_targets(Some(id)).expect_err("hidden");
    assert_eq!(r.message, "已選的 1 筆命中目前都未顯示，未變更");
    assert_eq!(r.hint, Some(Hint::RevealSelection), "操作方式由前端說明");
}

#[test]
fn applying_pending_creates_probable_finding() {
    let (mut ws, _rec) = open(world());
    assert_eq!(ws.data.pending.len(), 1);
    assert_eq!(
        ws.pending_targets().expect("targets"),
        ["FW-EDGE", "PVE-01"]
    );
    let r = ws.resolve_pending(
        PENDING_CVE,
        &Action::Apply("PVE-01".into()),
        "人工確認",
        &[],
    );
    assert!(
        r.report.message.starts_with(&format!(
            "{PENDING_CVE} 已移出等待區並套用到 PVE-01（命中 #"
        )),
        "{:?}",
        r.report
    );
    assert!(ws.data.pending.is_empty(), "重新載入後等待區已空");
    let hit = ws
        .data
        .findings
        .iter()
        .find(|f| f.cve_id == PENDING_CVE)
        .expect("finding");
    assert_eq!(hit.asset_tag, "PVE-01");
    assert_eq!(hit.confidence, Confidence::Probable);
}

#[test]
fn second_sync_while_running_is_refused() {
    let (mut ws, rec) = open(world());
    assert_eq!(ws.start_sync().message, "已在背景啟動 sync");
    assert_eq!(ws.start_sync().message, "背景 sync 仍在執行");
    assert_eq!(rec.launches.get(), 1);
    // 排程 sync 正在執行：子程序以結束碼 3 結束
    *rec.exit.borrow_mut() = Some(Some(3));
    let r = ws.tick(Instant::now()).expect("finished");
    assert!(r.message.starts_with("已有 sync 正在執行"), "{r:?}");
    assert_eq!(r.level, Level::Warning);
    assert!(ws.sync.is_none());
}

#[test]
fn finished_sync_reports_success_and_detects_new_data() {
    let (mut ws, rec) = open(world());
    let _ = ws.start_sync();
    assert!(ws.tick(Instant::now()).is_none(), "仍在執行");
    ws.conn
        .execute(
            "INSERT INTO sync_run (started_at, finished_at, source, status, added_count, updated_count)
             VALUES ('2026-11-07T00:30:00+00:00', '2026-11-07T00:31:00+00:00', 'kev', 'success', 1, 0)",
            [],
        )
        .expect("sync_run");
    *rec.exit.borrow_mut() = Some(Some(0));
    let r = ws.tick(Instant::now()).expect("finished");
    assert_eq!(
        (r.message.as_str(), r.level),
        ("背景 sync 完成", Level::Info)
    );
    assert!(ws.new_data, "結束時立即檢查新資料");
}

#[test]
fn unsafe_url_is_rejected() {
    let (ws, rec) = open(world());
    let r = ws.open_url("file:///C:/Windows/system32/calc.exe");
    assert_eq!(
        r.message,
        "拒絕開啟非 http(s) 連結：file:///C:/Windows/system32/calc.exe"
    );
    assert_eq!(r.level, Level::Warning);
    assert!(rec.opened.borrow().is_empty());
    assert!(ws
        .open_url("https://www.cve.org/")
        .message
        .starts_with("已開啟"));
    assert_eq!(rec.opened.borrow().len(), 1);
}

#[test]
fn reload_reports_open_details_that_are_gone() {
    let (mut ws, _rec) = open(world());
    let id = ws.visible_ids()[0];
    let kept = ws.visible_ids()[1];
    ws.conn
        .execute("DELETE FROM finding WHERE id = ?1", [id])
        .expect("delete");
    let r = ws.reload(&[id, kept]);
    assert_eq!(
        r.report.message,
        format!("已重新載入；命中 #{id} 已不存在，已關閉詳情")
    );
    assert!(matches!(r.details[0], DetailRefresh::Gone));
    assert!(
        matches!(&r.details[1], DetailRefresh::Updated(d) if d.finding.row.id == kept),
        "{:?}",
        r.details[1]
    );
}
