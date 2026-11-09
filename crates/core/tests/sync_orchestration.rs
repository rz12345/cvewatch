//! 7.3：單一來源失敗時其他來源成功、失敗來源游標不變、`sync_run` 記錄、結束碼 4；`--source` 只跑指定來源。

mod common;

use common::ingest::{at, baseline_zip, delta_zip, harness, Harness};
use cvewatch_core::config::{CsafDiscovery, CsafSourceConfig, SourcesConfig};
use cvewatch_core::ingest::cursor;
use cvewatch_core::ingest::runlog;
use cvewatch_core::ingest::source::SyncOptions;
use cvewatch_core::ingest::sync;
use cvewatch_core::ExitCode;
use rusqlite::Connection;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const NOW: &str = "2026-11-07T03:30:00Z";

fn rec(id: &str) -> Vec<u8> {
    common::read_ingest(&format!("cvelist/records/{id}.json"))
}

async fn get(h: &Harness, p: &str, status: u16, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(ResponseTemplate::new(status).set_body_bytes(body))
        .mount(&h.server)
        .await;
}

async fn mount_all_but_nvd(h: &Harness) {
    get(
        h,
        "/gh-api/repos/CVEProject/cvelistV5/releases",
        200,
        common::read_ingest("cvelist/releases.json"),
    )
    .await;
    get(
        h,
        "/gh-dl/cve_2026-11-07_0300Z/2026-11-07_all_CVEs_at_midnight.zip.zip",
        200,
        baseline_zip(&[rec("CVE-2024-3400")]),
    )
    .await;
    get(
        h,
        "/gh-dl/cve_2026-11-07_0300Z/2026-11-07_delta_CVEs_at_0300Z.zip",
        200,
        delta_zip(&[rec("CVE-2025-0108")]),
    )
    .await;
    get(
        h,
        "/pa/rss.xml",
        200,
        common::read_ingest("csaf/paloalto/rss.xml"),
    )
    .await;
    get(
        h,
        "/pa/csaf/CVE-2026-0322",
        200,
        common::read_ingest("csaf/paloalto/CVE-2026-0322.json"),
    )
    .await;
    get(h, "/kev.json", 200, common::read_ingest("kev/kev.json")).await;
    get(h, "/epss", 200, common::read_ingest("epss/epss.json")).await;
}

fn sources(h: &Harness) -> SourcesConfig {
    SourcesConfig {
        csaf: vec![CsafSourceConfig {
            id: "paloalto".into(),
            discovery: CsafDiscovery::Rss,
            url: format!("{}/pa/rss.xml", h.server.uri()),
            csaf_url_template: Some(format!("{}/pa/csaf/{{id}}", h.server.uri())),
            enabled: true,
        }],
        ..SourcesConfig::default()
    }
}

fn cursor_exists(conn: &Connection, source: &str) -> bool {
    cursor::get::<serde_json::Value>(conn, source)
        .unwrap()
        .is_some()
}

#[tokio::test]
async fn nvd_failure_does_not_stop_other_sources() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    mount_all_but_nvd(&h).await;
    get(&h, "/nvd", 503, Vec::new()).await;
    let plans = sync::plan(&sources(&h), &[]).unwrap();
    let options = SyncOptions::default();
    let report = sync::run(&h.ctx(&options, at(NOW)), &mut conn, &plans)
        .await
        .unwrap();

    assert_eq!(report.exit_code(), ExitCode::PartialFailure);
    assert_eq!(ExitCode::PartialFailure.code(), 4);
    let failed: Vec<&str> = report.failed().map(|o| o.name.as_str()).collect();
    assert_eq!(failed, vec!["nvd"]);
    let names: Vec<&str> = report.outcomes.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "cvelist",
            "nvd",
            "csaf:paloalto",
            "kev",
            "epss",
            "osv",
            "eol"
        ]
    );

    // 其他來源已更新
    let local: i64 = conn
        .query_row("SELECT count(*) FROM cve", [], |r| r.get(0))
        .unwrap();
    assert!(local >= 3, "cvelist 2 筆 + CSAF 1 筆，實際 {local}");
    let kev: i64 = conn
        .query_row("SELECT count(*) FROM kev", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kev, 3);
    // NVD 游標不變（原本沒有 → 仍沒有），其他來源有游標
    assert!(!cursor_exists(&conn, "nvd"));
    for s in ["cvelist", "csaf:paloalto", "kev", "epss"] {
        assert!(cursor_exists(&conn, s), "{s}");
    }
    // sync_run 記錄失敗原因
    let runs = runlog::history(&conn, runlog::HISTORY_LIMIT).unwrap();
    let nvd = runs.iter().find(|r| r.source == "nvd").unwrap();
    assert_eq!(nvd.status, "failed");
    assert!(
        nvd.error.as_deref().unwrap_or_default().contains("503"),
        "{nvd:?}"
    );
    assert!(nvd.finished_at.is_some());
    assert_eq!(
        runs.iter().filter(|r| r.status == "success").count(),
        6,
        "{runs:?}（eol 沒有要查詢的代稱，仍成功）"
    );
}

#[tokio::test]
async fn only_selected_source_runs() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    get(&h, "/kev.json", 200, common::read_ingest("kev/kev.json")).await;
    let plans = sync::plan(&sources(&h), &["kev".to_string()]).unwrap();
    let options = SyncOptions::default();
    let report = sync::run(&h.ctx(&options, at(NOW)), &mut conn, &plans)
        .await
        .unwrap();
    assert_eq!(report.exit_code(), ExitCode::Success);
    let runs = runlog::history(&conn, runlog::HISTORY_LIMIT).unwrap();
    let sources: Vec<&str> = runs.iter().map(|r| r.source.as_str()).collect();
    assert_eq!(sources, vec!["kev"]);
    assert!(cursor_exists(&conn, "kev"));
    for s in ["cvelist", "nvd", "csaf:paloalto", "epss"] {
        assert!(!cursor_exists(&conn, s), "{s} 游標不應變動");
    }
    let received = h.server.received_requests().await.unwrap();
    assert!(received.iter().all(|r| r.url.path() == "/kev.json"));
}

#[tokio::test]
async fn history_lists_latest_runs_first() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    get(&h, "/kev.json", 200, common::read_ingest("kev/kev.json")).await;
    let plans = sync::plan(&sources(&h), &["kev".to_string()]).unwrap();
    let options = SyncOptions::default();
    for _ in 0..25 {
        sync::run(&h.ctx(&options, at(NOW)), &mut conn, &plans)
            .await
            .unwrap();
    }
    let runs = runlog::history(&conn, runlog::HISTORY_LIMIT).unwrap();
    assert_eq!(runs.len(), 20);
    assert!(runs.windows(2).all(|w| w[0].id > w[1].id));
}

/// 04 code review 第十六輪：來源在寫入交易內、資料有變動時才記錄「比對待補」；
/// 失敗的來源交易已回復不記錄，資料沒變的同步也不記錄。
#[tokio::test]
async fn sync_marks_match_stale_only_when_source_data_changes() {
    use cvewatch_core::db::meta;
    use cvewatch_core::matching::run::{match_is_stale, MATCH_STALE_KEY};

    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    Mock::given(method("GET"))
        .and(path("/kev.json"))
        // 404 不重試，只讓第一次同步失敗
        .respond_with(ResponseTemplate::new(404))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&h.server)
        .await;
    get(&h, "/kev.json", 200, common::read_ingest("kev/kev.json")).await;
    let plans = sync::plan(&sources(&h), &["kev".to_string()]).unwrap();
    let options = SyncOptions::default();
    let ctx = h.ctx(&options, at(NOW));

    let failed = sync::run(&ctx, &mut conn, &plans).await.unwrap();
    assert_eq!(failed.exit_code(), ExitCode::PartialFailure);
    assert!(
        !match_is_stale(&conn).unwrap(),
        "失敗的來源沒有寫入，不記錄"
    );

    sync::run(&ctx, &mut conn, &plans).await.unwrap();
    assert!(match_is_stale(&conn).unwrap(), "KEV 有新資料時記錄");

    meta::delete(&conn, MATCH_STALE_KEY).unwrap();
    sync::run(&ctx, &mut conn, &plans).await.unwrap();
    assert!(!match_is_stale(&conn).unwrap(), "KEV 內容沒有變動時不記錄");
}
