//! 4.3：NVD 分頁、429 重試、增量游標、API key header、失敗時資料與游標不變。

mod common;

use common::ingest::{at, harness, Harness};
use cvewatch_core::ingest::cursor;
use cvewatch_core::ingest::nvd::{self, NvdCursor};
use cvewatch_core::ingest::source::SyncOptions;
use rusqlite::Connection;
use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, ResponseTemplate};

const NOW: &str = "2026-11-07T03:30:00Z";
const PANOS: &str = "cpe:2.3:o:paloaltonetworks:pan-os";

fn page(n: u8) -> serde_json::Value {
    serde_json::from_slice(&common::read_ingest(&format!("nvd/page{n}.json"))).unwrap()
}

fn empty_page() -> serde_json::Value {
    serde_json::json!({"resultsPerPage": 0, "startIndex": 0, "totalResults": 0,
                       "format": "NVD_CVE", "version": "2.0", "vulnerabilities": []})
}

/// 其他 CPE 一律回傳空結果。
async fn mount_default(h: &Harness) {
    Mock::given(method("GET"))
        .and(path("/nvd"))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_page()))
        .with_priority(10)
        .mount(&h.server)
        .await;
}

async fn mount_panos_pages(h: &Harness) {
    Mock::given(method("GET"))
        .and(path("/nvd"))
        .and(query_param("virtualMatchString", PANOS))
        .and(query_param("startIndex", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(1)))
        .expect(1)
        .mount(&h.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/nvd"))
        .and(query_param("virtualMatchString", PANOS))
        .and(query_param("startIndex", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(2)))
        .expect(1)
        .mount(&h.server)
        .await;
}

fn ids(conn: &Connection) -> Vec<String> {
    let mut stmt = conn.prepare("SELECT id FROM cve ORDER BY id").unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[tokio::test]
async fn paginates_and_stores_nvd_data() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    mount_default(&h).await;
    mount_panos_pages(&h).await;
    let options = SyncOptions::default();
    let stats = nvd::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!((stats.added, stats.updated), (3, 0), "{stats:?}");
    assert_eq!(ids(&conn).len(), 3);

    let first = page(1)["vulnerabilities"][0]["cve"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (reason, status): (String, String) = conn
        .query_row(
            "SELECT relevance_reason, nvd_vuln_status FROM cve WHERE id = ?1",
            [&first],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((reason.as_str(), status.as_str()), ("nvd", "Analyzed"));
    let matches: i64 = conn
        .query_row(
            "SELECT count(*) FROM cve_nvd_match WHERE cve_id = ?1 AND vuln = 1
             AND criteria_cpe LIKE 'cpe:2.3:o:paloaltonetworks:pan-os:%'",
            [&first],
            |r| r.get(0),
        )
        .unwrap();
    assert!(matches >= 1);
    let nvd_metrics: i64 = conn
        .query_row(
            "SELECT count(*) FROM cve_metric WHERE cve_id = ?1 AND source = 'nvd'",
            [&first],
            |r| r.get(0),
        )
        .unwrap();
    assert!(nvd_metrics >= 1);
    // 首次查詢不帶時間參數，之後游標記錄本次時間
    let c: NvdCursor = cursor::get(&conn, nvd::SOURCE).unwrap().unwrap();
    assert_eq!(c.cpes["o:paloaltonetworks:pan-os"], at(NOW));
}

#[tokio::test]
async fn existing_cve_is_updated_not_added() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    mount_default(&h).await;
    mount_panos_pages(&h).await;
    let first = page(1)["vulnerabilities"][0]["cve"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    conn.execute(
        "INSERT INTO cve (id, state, relevance_reason, updated_at) VALUES (?1, 'PUBLISHED', 'cna', 't')",
        [&first],
    )
    .unwrap();
    let options = SyncOptions::default();
    let stats = nvd::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!((stats.added, stats.updated), (2, 1));
    let reason: String = conn
        .query_row(
            "SELECT relevance_reason FROM cve WHERE id = ?1",
            [&first],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reason, "cna", "既有原因不被 NVD 覆寫");
}

#[tokio::test]
async fn retries_after_429() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    mount_default(&h).await;
    Mock::given(method("GET"))
        .and(path("/nvd"))
        .and(query_param("virtualMatchString", PANOS))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&h.server)
        .await;
    mount_panos_pages(&h).await;
    let options = SyncOptions::default();
    let stats = nvd::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!(stats.added, 3);
}

#[tokio::test]
async fn incremental_query_uses_cursor_and_api_key() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    let mut previous = NvdCursor::default();
    previous.cpes.insert(
        "o:paloaltonetworks:pan-os".into(),
        at("2026-11-01T00:00:00Z"),
    );
    cursor::set(&conn, nvd::SOURCE, &previous, NOW).unwrap();
    // 有游標的 CPE 帶時間區間；沒有游標的其他 CPE 不帶
    Mock::given(method("GET"))
        .and(path("/nvd"))
        .and(query_param("virtualMatchString", PANOS))
        .and(query_param("lastModStartDate", "2026-11-01T00:00:00.000Z"))
        .and(query_param("lastModEndDate", "2026-11-07T03:30:00.000Z"))
        .and(header("apiKey", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_page()))
        .expect(1)
        .mount(&h.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/nvd"))
        .and(query_param("virtualMatchString", "cpe:2.3:o:vmware:esxi"))
        .and(query_param_is_missing("lastModStartDate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_page()))
        .expect(1)
        .mount(&h.server)
        .await;
    mount_default(&h).await;
    let key = String::from("test-key");
    let options = SyncOptions::default();
    let mut ctx = h.ctx(&options, at(NOW));
    ctx.nvd_api_key = Some(&key);
    nvd::sync(&ctx, &mut conn).await.unwrap();
}

#[tokio::test]
async fn unavailable_nvd_keeps_cursor_and_data() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    let previous = NvdCursor::default();
    cursor::set(&conn, nvd::SOURCE, &previous, "before").unwrap();
    // pan-os 第一頁成功，第二頁持續 503
    Mock::given(method("GET"))
        .and(path("/nvd"))
        .and(query_param("virtualMatchString", PANOS))
        .and(query_param("startIndex", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(1)))
        .mount(&h.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/nvd"))
        .and(query_param("startIndex", "2"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&h.server)
        .await;
    mount_default(&h).await;
    let options = SyncOptions::default();
    let err = nvd::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("503"), "{err}");
    assert!(ids(&conn).is_empty(), "整個來源回復，第一頁也不寫入");
    let c: NvdCursor = cursor::get(&conn, nvd::SOURCE).unwrap().unwrap();
    assert_eq!(c, previous);
}
