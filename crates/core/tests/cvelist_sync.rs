//! 3.6：以 fixture zip 驗證關聯性篩選、游標推進、超過 25 天重抓 baseline、rescan 與指派單位。

mod common;

use common::ingest::{at, baseline_zip, delta_zip, edit_record, harness, Harness};
use cvewatch_core::ingest::assigner;
use cvewatch_core::ingest::cursor;
use cvewatch_core::ingest::cvelist::search_index;
use cvewatch_core::ingest::cvelist::{self, CvelistCursor, RESCAN_HINT};
use cvewatch_core::ingest::relevance::PackageKey;
use cvewatch_core::ingest::source::SyncOptions;
use rusqlite::Connection;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const NOW: &str = "2026-11-07T03:30:00Z";
const BASELINE: &str = "/gh-dl/cve_2026-11-07_0300Z/2026-11-07_all_CVEs_at_midnight.zip.zip";
const DELTA_0300: &str = "/gh-dl/cve_2026-11-07_0300Z/2026-11-07_delta_CVEs_at_0300Z.zip";
const RELEASES: &str = "/gh-api/repos/CVEProject/cvelistV5/releases";

fn rec(id: &str) -> Vec<u8> {
    common::read_ingest(&format!("cvelist/records/{id}.json"))
}

fn activemq() -> PackageKey {
    PackageKey::from_purl("pkg:maven/org.apache.activemq/activemq-broker").unwrap()
}

fn releases_json() -> serde_json::Value {
    serde_json::from_slice(&common::read_ingest("cvelist/releases.json")).unwrap()
}

/// 在列表最前面加上一個新的每小時 release。
fn with_new_hourly(hhmm: &str) -> serde_json::Value {
    let mut list = releases_json();
    let tag = format!("cve_2026-11-07_{hhmm}Z");
    let entry = serde_json::json!({
        "tag_name": tag, "name": tag, "published_at": "2026-11-07T04:20:00Z",
        "assets": [
            {"name": "2026-11-07_all_CVEs_at_midnight.zip.zip", "size": 1, "browser_download_url": "x"},
            {"name": format!("2026-11-07_delta_CVEs_at_{hhmm}Z.zip"), "size": 1, "browser_download_url": "x"}
        ]
    });
    list.as_array_mut().unwrap().insert(0, entry);
    list
}

async fn mount_releases(h: &Harness, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(RELEASES))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&h.server)
        .await;
}

/// 第一次回傳錄製的列表，之後回傳 `later`（模擬一小時後出現新 release）。
async fn mount_releases_then(h: &Harness, later: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(RELEASES))
        .respond_with(ResponseTemplate::new(200).set_body_json(releases_json()))
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    mount_releases(h, later).await;
}

async fn mount_bytes(h: &Harness, p: &str, body: Vec<u8>, times: u64) {
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .expect(times)
        .mount(&h.server)
        .await;
}

fn baseline_records() -> Vec<Vec<u8>> {
    [
        "CVE-2024-3400",   // PAN-OS：CNA 相符
        "CVE-2026-103479", // WordPress 外掛：無關
        "CVE-2099-50002",  // Proxmox 描述：僅關鍵字
        "CVE-2026-108949", // maven purl：套件相符
        "CVE-2099-50001",  // ESXi（5.0）：CNA 相符
        "CVE-2025-7735",   // TWCERT/CC 指派、與目錄無關
        "CVE-2019-7907",   // REJECTED、與目錄無關
    ]
    .iter()
    .map(|id| rec(id))
    .collect()
}

fn reason(conn: &Connection, id: &str) -> Option<String> {
    conn.query_row(
        "SELECT relevance_reason FROM cve WHERE id = ?1",
        [id],
        |r| r.get(0),
    )
    .ok()
}

fn cvelist_cursor(conn: &Connection) -> CvelistCursor {
    cursor::get(conn, cvelist::SOURCE).unwrap().expect("cursor")
}

/// 首次同步：baseline + 當日最新 delta。
async fn first_sync(h: &Harness, conn: &mut Connection) {
    mount_bytes(h, BASELINE, baseline_zip(&baseline_records()), 1).await;
    mount_bytes(h, DELTA_0300, delta_zip(&[rec("CVE-2025-0108")]), 1).await;
    let options = SyncOptions::default();
    cvelist::sync(&h.ctx(&options, at(NOW)), conn)
        .await
        .unwrap();
}

#[tokio::test]
async fn first_sync_filters_and_sets_cursor() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![activemq()]).await;
    mount_releases(&h, releases_json()).await;
    mount_bytes(&h, BASELINE, baseline_zip(&baseline_records()), 1).await;
    mount_bytes(&h, DELTA_0300, delta_zip(&[rec("CVE-2025-0108")]), 1).await;
    let options = SyncOptions::default();
    let stats = cvelist::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!((stats.added, stats.updated), (5, 0), "{stats:?}");

    assert_eq!(reason(&conn, "CVE-2024-3400").as_deref(), Some("cna"));
    assert_eq!(reason(&conn, "CVE-2099-50001").as_deref(), Some("cna"));
    assert_eq!(
        reason(&conn, "CVE-2099-50002").as_deref(),
        Some("keyword_only")
    );
    assert_eq!(reason(&conn, "CVE-2026-108949").as_deref(), Some("purl"));
    assert_eq!(reason(&conn, "CVE-2025-0108").as_deref(), Some("cna"));
    // 無關的 WordPress 外掛、TWCERT 醫療系統、撤銷紀錄都不寫入
    for id in ["CVE-2026-103479", "CVE-2025-7735", "CVE-2019-7907"] {
        assert_eq!(reason(&conn, id), None, "{id}");
    }
    let c = cvelist_cursor(&conn);
    assert_eq!(c.baseline_date.to_string(), "2026-11-07");
    assert_eq!(c.day.to_string(), "2026-11-07");
    assert_eq!(
        c.last_delta.as_deref(),
        Some("2026-11-07_delta_CVEs_at_0300Z.zip")
    );
    // 搜尋索引含無關的紀錄
    let hits = search_index::search(&h.paths.cvelist_cache_dir, "wordpress")
        .unwrap()
        .unwrap();
    assert!(hits.iter().any(|e| e.id == "CVE-2026-103479"));
}

#[tokio::test]
async fn second_sync_same_day_only_processes_new_delta() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    mount_releases_then(&h, with_new_hourly("0400")).await;
    // baseline 只在第一次下載；0300Z 也只下載一次
    first_sync(&h, &mut conn).await;
    let changed = edit_record(&rec("CVE-2024-3400"), |v| {
        v["cveMetadata"]["dateUpdated"] = "2026-11-07T04:05:00.000Z".into();
    });
    mount_bytes(
        &h,
        "/gh-dl/cve_2026-11-07_0400Z/2026-11-07_delta_CVEs_at_0400Z.zip",
        delta_zip(&[rec("CVE-2025-0108"), changed]),
        1,
    )
    .await;
    let options = SyncOptions::default();
    let stats = cvelist::sync(&h.ctx(&options, at("2026-11-07T04:30:00Z")), &mut conn)
        .await
        .unwrap();
    // 0108 已是相同 dateUpdated（累積 delta 重複），只有 3400 更新
    assert_eq!((stats.added, stats.updated), (0, 1), "{stats:?}");
    assert_eq!(
        cvelist_cursor(&conn).last_delta.as_deref(),
        Some("2026-11-07_delta_CVEs_at_0400Z.zip")
    );
    // 第三次：沒有新 delta → 不下載任何 zip（expect 次數於 drop 時驗證）
    let stats = cvelist::sync(&h.ctx(&options, at("2026-11-07T04:40:00Z")), &mut conn)
        .await
        .unwrap();
    assert_eq!((stats.added, stats.updated), (0, 0));
}

#[tokio::test]
async fn long_gap_downloads_baseline_again() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    mount_releases(&h, releases_json()).await;
    let old = CvelistCursor {
        baseline_date: "2026-09-01".parse().unwrap(),
        day: "2026-10-12".parse().unwrap(),
        last_delta: None,
        catalog_hash: h.index.fingerprint(),
    };
    cursor::set(&conn, cvelist::SOURCE, &old, NOW).unwrap();
    first_sync(&h, &mut conn).await;
    assert_eq!(
        cvelist_cursor(&conn).baseline_date.to_string(),
        "2026-11-07"
    );
}

#[tokio::test]
async fn catches_up_day_by_day_with_end_of_day_fallback() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    mount_releases(&h, releases_json()).await;
    let start = CvelistCursor {
        baseline_date: "2026-11-05".parse().unwrap(),
        day: "2026-11-05".parse().unwrap(),
        last_delta: None,
        catalog_hash: h.index.fingerprint(),
    };
    cursor::set(&conn, cvelist::SOURCE, &start, NOW).unwrap();
    mount_bytes(
        &h,
        "/gh-dl/cve_2026-11-05_at_end_of_day/2026-11-05_delta_CVEs_at_end_of_day.zip",
        delta_zip(&[rec("CVE-2024-3400")]),
        1,
    )
    .await;
    // 11-06 的 end_of_day 尚未發布（404）→ 改用 2300Z，游標停在 11-06 下次再試
    Mock::given(method("GET"))
        .and(path(
            "/gh-dl/cve_2026-11-06_at_end_of_day/2026-11-06_delta_CVEs_at_end_of_day.zip",
        ))
        .respond_with(ResponseTemplate::new(404))
        .mount(&h.server)
        .await;
    mount_bytes(
        &h,
        "/gh-dl/cve_2026-11-06_2300Z/2026-11-06_delta_CVEs_at_2300Z.zip",
        delta_zip(&[rec("CVE-2025-0108")]),
        1,
    )
    .await;
    let options = SyncOptions::default();
    let stats = cvelist::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!(stats.added, 2);
    let c = cvelist_cursor(&conn);
    assert_eq!(c.day.to_string(), "2026-11-06");
    assert_eq!(
        c.last_delta.as_deref(),
        Some("2026-11-06_delta_CVEs_at_2300Z.zip")
    );
}

#[tokio::test]
async fn rejected_in_delta_marks_local_cve() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    mount_releases_then(&h, with_new_hourly("0400")).await;
    first_sync(&h, &mut conn).await;
    let rejected = edit_record(&rec("CVE-2024-3400"), |v| {
        v["cveMetadata"]["state"] = "REJECTED".into();
        v["cveMetadata"]["dateUpdated"] = "2026-11-07T04:01:00.000Z".into();
        v["containers"]["cna"]["affected"] = serde_json::json!([]);
    });
    mount_bytes(
        &h,
        "/gh-dl/cve_2026-11-07_0400Z/2026-11-07_delta_CVEs_at_0400Z.zip",
        delta_zip(&[rejected]),
        1,
    )
    .await;
    let options = SyncOptions::default();
    cvelist::sync(&h.ctx(&options, at("2026-11-07T04:30:00Z")), &mut conn)
        .await
        .unwrap();
    let (state, why): (String, String) = conn
        .query_row(
            "SELECT state, relevance_reason FROM cve WHERE id = 'CVE-2024-3400'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "REJECTED");
    // 撤銷後不再有 affected，但已在本機 → 保留並沿用原因
    assert_eq!(why, "cna");
}

const UNIMAX: &str = r#"
[[product]]
key = "unimax-his"
display_name = "UNIMAX HIS"
vendor_display = "UNIMAX"
version_scheme = "dotted"
cna = [{ vendor = "UNIMAX", product = "Hospital Information System" }]
"#;

#[tokio::test]
async fn catalog_change_prompts_rescan_and_rescan_adds_cves() {
    let mut conn = common::memory_db();
    let mut h = harness(&conn, None, vec![]).await;
    mount_releases(&h, releases_json()).await;
    // baseline 與當日 delta 各下載兩次：首次與 --rescan（重抓 baseline 後重新套用當日 delta）
    mount_bytes(&h, BASELINE, baseline_zip(&baseline_records()), 2).await;
    mount_bytes(&h, DELTA_0300, delta_zip(&[rec("CVE-2025-0108")]), 2).await;
    let plain = SyncOptions::default();
    cvelist::sync(&h.ctx(&plain, at(NOW)), &mut conn)
        .await
        .unwrap();

    h.set_catalog(&conn, Some(UNIMAX));
    let stats = cvelist::sync(&h.ctx(&plain, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert!(stats.notes.iter().any(|n| n == RESCAN_HINT), "{stats:?}");
    assert_eq!(reason(&conn, "CVE-2025-7735"), None);

    let rescan = SyncOptions {
        rescan: true,
        since: None,
    };
    let stats = cvelist::sync(&h.ctx(&rescan, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!(stats.added, 1, "{stats:?}");
    assert_eq!(reason(&conn, "CVE-2025-7735").as_deref(), Some("cna"));
    // rescan 後不再提示；之後的 sync 不重新下載
    let stats = cvelist::sync(&h.ctx(&plain, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert!(stats.notes.is_empty(), "{stats:?}");

    // TWCERT/CC 指派的 CVE 標示為國內通報來源
    let assigner: String = conn
        .query_row(
            "SELECT assigner_short FROM cve WHERE id = 'CVE-2025-7735'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        assigner::display(Some(&assigner)),
        "TWCERT/CC（國內通報來源）"
    );
}

#[tokio::test]
async fn backfills_stub_cves_from_single_record() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    mount_releases(&h, releases_json()).await;
    mount_bytes(&h, BASELINE, baseline_zip(&[]), 1).await;
    mount_bytes(&h, DELTA_0300, delta_zip(&[]), 1).await;
    cvewatch_core::ingest::store::ensure_stub(
        &conn,
        "CVE-2024-3400",
        cvewatch_core::ingest::relevance::Relevance::Csaf,
        None,
        NOW,
    )
    .unwrap();
    mount_bytes(
        &h,
        "/raw/cves/2024/3xxx/CVE-2024-3400.json",
        rec("CVE-2024-3400"),
        1,
    )
    .await;
    let options = SyncOptions::default();
    let stats = cvelist::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!(stats.updated, 1, "{stats:?}");
    let has_raw: bool = conn
        .query_row(
            "SELECT raw_cna_json IS NOT NULL FROM cve WHERE id = 'CVE-2024-3400'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(has_raw);
    // 內容相符 → 原因由 csaf 改為 cna
    assert_eq!(reason(&conn, "CVE-2024-3400").as_deref(), Some("cna"));
}

#[test]
fn baseline_fixture_uses_data_descriptor() {
    let zip = baseline_zip(&[rec("CVE-2024-3400")]);
    let flags = u16::from_le_bytes([zip[6], zip[7]]);
    assert_ne!(
        flags & 0x08,
        0,
        "外層 entry 應帶 data descriptor，與實際 baseline 相同"
    );
}

#[tokio::test]
async fn unpublished_stubs_rotate_instead_of_blocking_backfill() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    mount_releases(&h, releases_json()).await;
    mount_bytes(&h, BASELINE, baseline_zip(&[]), 1).await;
    mount_bytes(&h, DELTA_0300, delta_zip(&[]), 1).await;
    for id in ["CVE-2000-0001", "CVE-2024-3400"] {
        cvewatch_core::ingest::store::ensure_stub(
            &conn,
            id,
            cvewatch_core::ingest::relevance::Relevance::Csaf,
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
    }
    // CVE-2000-0001 尚未發布（404，wiremock 預設）
    mount_bytes(
        &h,
        "/raw/cves/2024/3xxx/CVE-2024-3400.json",
        rec("CVE-2024-3400"),
        1,
    )
    .await;
    let options = SyncOptions::default();
    cvelist::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    let rotated: String = conn
        .query_row(
            "SELECT updated_at FROM cve WHERE id = 'CVE-2000-0001'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rotated, at(NOW).to_rfc3339(), "404 的 stub 排到最後");
    assert_eq!(reason(&conn, "CVE-2024-3400").as_deref(), Some("cna"));
}
