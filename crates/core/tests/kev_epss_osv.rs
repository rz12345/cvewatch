//! 5.4：KEV（條件式請求）、EPSS（批次、每日一次）、OSV（ecosystem 轉換、詳細資料快取）。

mod common;

use common::ingest::{at, harness, Harness};
use cvewatch_core::db::meta;
use cvewatch_core::ingest::relevance::Relevance;
use cvewatch_core::ingest::source::SyncOptions;
use cvewatch_core::ingest::store::ensure_stub;
use cvewatch_core::ingest::{epss, kev, osv};
use cvewatch_core::matching::run::{match_is_stale, MATCH_STALE_KEY};
use rusqlite::{params, Connection};
use wiremock::matchers::{header_exists, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const NOW: &str = "2026-11-07T03:30:00Z";
const LAST_MODIFIED: &str = "Fri, 06 Nov 2026 19:44:45 GMT";

fn json(rel: &str) -> serde_json::Value {
    serde_json::from_slice(&common::read_ingest(rel)).unwrap()
}

fn stub(conn: &Connection, id: &str) {
    ensure_stub(conn, id, Relevance::Cna, None, NOW).unwrap();
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

// ── KEV ─────────────────────────────────────────────────────

async fn mount_kev(h: &Harness) {
    Mock::given(method("GET"))
        .and(path("/kev.json"))
        // wiremock 的 header() 會以逗號切分值，HTTP 日期含逗號，因此只比對存在與否，值另外檢查
        .and(header_exists("If-Modified-Since"))
        .respond_with(ResponseTemplate::new(304))
        .with_priority(1)
        .mount(&h.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/kev.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json("kev/kev.json"))
                .insert_header("Last-Modified", LAST_MODIFIED)
                .insert_header("ETag", "\"abc\""),
        )
        .expect(1)
        .mount(&h.server)
        .await;
}

#[tokio::test]
async fn kev_marks_local_cves_and_uses_conditional_request() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    mount_kev(&h).await;
    stub(&conn, "CVE-2024-3400");
    let options = SyncOptions::default();
    let stats = kev::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!((stats.added, stats.updated), (3, 0));
    let (added, due, ransomware): (String, String, bool) = conn
        .query_row(
            "SELECT k.date_added, k.due_date, k.known_ransomware
             FROM cve c JOIN kev k ON k.cve_id = c.id WHERE c.id = 'CVE-2024-3400'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (added.as_str(), due.as_str(), ransomware),
        ("2024-04-12", "2024-04-19", true)
    );

    // 第二次：帶 If-Modified-Since → 304，資料不變
    let stats = kev::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!(stats.notes, vec![kev::NOT_MODIFIED_NOTE.to_string()]);
    assert_eq!(count(&conn, "SELECT count(*) FROM kev"), 3);
    let requests = h.server.received_requests().await.unwrap();
    let last = requests.last().unwrap();
    assert_eq!(
        last.headers.get("If-Modified-Since").unwrap(),
        LAST_MODIFIED
    );
    assert_eq!(last.headers.get("If-None-Match").unwrap(), "\"abc\"");
}

#[tokio::test]
async fn kev_failure_keeps_previous_list() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    conn.execute(
        "INSERT INTO kev (cve_id, date_added) VALUES ('CVE-2000-0001', '2020-01-01')",
        [],
    )
    .unwrap();
    Mock::given(method("GET"))
        .and(path("/kev.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>maintenance</html>"))
        .mount(&h.server)
        .await;
    let options = SyncOptions::default();
    assert!(kev::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .is_err());
    assert_eq!(count(&conn, "SELECT count(*) FROM kev"), 1);
}

// ── EPSS ────────────────────────────────────────────────────

#[tokio::test]
async fn epss_scores_once_per_day() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    for id in ["CVE-2024-3400", "CVE-2025-0108", "CVE-2099-50001"] {
        stub(&conn, id);
    }
    conn.execute(
        "UPDATE cve SET state = 'REJECTED' WHERE id = 'CVE-2099-50001'",
        [],
    )
    .unwrap();
    Mock::given(method("GET"))
        .and(path("/epss"))
        .and(query_param("cve", "CVE-2024-3400,CVE-2025-0108"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json("epss/epss.json")))
        .expect(2)
        .mount(&h.server)
        .await;
    let options = SyncOptions::default();
    let stats = epss::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!(stats.added, 2);
    let (score, pct, fetched): (f64, f64, String) = conn
        .query_row(
            "SELECT score, percentile, fetched_on FROM epss WHERE cve_id = 'CVE-2024-3400'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert!((score - 0.99999).abs() < 1e-9 && (pct - 1.0).abs() < 1e-9);
    assert_eq!(fetched, "2026-11-07");

    assert!(match_is_stale(&conn).unwrap(), "新分數影響風險等級");
    meta::delete(&conn, MATCH_STALE_KEY).unwrap();
    // 同一天：不再查詢
    let stats = epss::sync(&h.ctx(&options, at("2026-11-07T09:00:00Z")), &mut conn)
        .await
        .unwrap();
    assert_eq!((stats.added, stats.updated), (0, 0));
    // 隔天：重新查詢並更新
    let stats = epss::sync(&h.ctx(&options, at("2026-11-08T01:00:00Z")), &mut conn)
        .await
        .unwrap();
    assert_eq!(stats.updated, 2);
    // 第十八輪 #2：分數沒變不需要重新比對
    assert!(!match_is_stale(&conn).unwrap());
}

#[tokio::test]
async fn epss_batches_at_most_100_ids() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    for n in 0..150 {
        stub(&conn, &format!("CVE-2099-{:05}", 10000 + n));
    }
    Mock::given(method("GET"))
        .and(path("/epss"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
        .expect(2)
        .mount(&h.server)
        .await;
    let options = SyncOptions::default();
    epss::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    // 沒有分數的 CVE 當天不再查詢
    epss::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    let requests = h.server.received_requests().await.unwrap();
    let longest = requests
        .iter()
        .map(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == "cve")
                .unwrap()
                .1
                .split(',')
                .count()
        })
        .max()
        .unwrap();
    assert_eq!(longest, epss::BATCH_SIZE);
}

// ── OSV ─────────────────────────────────────────────────────

fn add_package(conn: &Connection, tag: &str, purl: &str, version: &str) -> i64 {
    conn.execute(
        "INSERT OR IGNORE INTO asset (asset_tag, name, category, first_seen, last_seen)
         VALUES (?1, ?1, 'host', 't', 't')",
        params![tag],
    )
    .unwrap();
    let asset: i64 = conn
        .query_row("SELECT id FROM asset WHERE asset_tag = ?1", [tag], |r| {
            r.get(0)
        })
        .unwrap();
    conn.execute(
        "INSERT INTO component (asset_id, kind, purl, purl_full, version_raw, source, updated_at)
         VALUES (?1, 'package', ?2, ?2, ?3, 'dpkg', 't')",
        params![asset, purl, version],
    )
    .unwrap();
    conn.last_insert_rowid()
}

async fn mount_osv(h: &Harness) {
    let batch = json("osv/querybatch.json");
    let first_only = serde_json::json!({ "results": [batch["results"][0]] });
    Mock::given(method("POST"))
        .and(path("/osv/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(first_only))
        .mount(&h.server)
        .await;
    let detail = json("osv/DEBIAN-CVE-2023-5678.json");
    let mut other = detail.clone();
    other["id"] = "DEBIAN-CVE-2023-6129".into();
    other["upstream"] = serde_json::json!(["CVE-2023-6129"]);
    for (id, body) in [
        ("DEBIAN-CVE-2023-5678", detail),
        ("DEBIAN-CVE-2023-6129", other),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/osv/vulns/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&h.server)
            .await;
    }
}

#[tokio::test]
async fn osv_saves_hits_with_cve_aliases_and_fixed_versions() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    let openssl = add_package(
        &conn,
        "WEB-01",
        "pkg:deb/debian/openssl?distro=debian-12",
        "3.0.11-1~deb12u2",
    );
    add_package(
        &conn,
        "WEB-01",
        "pkg:rpm/redhat/bash?distro=rhel-9",
        "5.1.8-9.el9",
    );
    mount_osv(&h).await;
    let options = SyncOptions::default();
    let stats = osv::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!(stats.added, 2);
    assert!(
        stats.notes.iter().any(|n| n.contains("1 個套件")),
        "{stats:?}"
    );
    let (cves, fixed): (String, String) = conn
        .query_row(
            "SELECT cve_ids_json, fixed_versions_json FROM osv_hit
             WHERE component_id = ?1 AND osv_id = 'DEBIAN-CVE-2023-5678'",
            [openssl],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(cves, r#"["CVE-2023-5678"]"#);
    assert_eq!(fixed, r#"["3.0.13-1~deb12u1"]"#);

    // 第二次：modified 未變 → 不重抓詳細資料（expect(1)），命中改記為更新
    let stats = osv::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!((stats.added, stats.updated), (0, 2));
    // OSV 命中的 CVE 也納入 EPSS 查詢對象
    let request = serde_json::json!({"data": []});
    Mock::given(method("GET"))
        .and(path("/epss"))
        .and(query_param("cve", "CVE-2023-5678,CVE-2023-6129"))
        .respond_with(ResponseTemplate::new(200).set_body_json(request))
        .expect(1)
        .mount(&h.server)
        .await;
    epss::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
}

/// 第十八輪 #2：OSV 命中沒有變動、也沒有等待 OSV 的套件時，不需要重新比對。
#[tokio::test]
async fn osv_unchanged_hits_do_not_mark_match_stale() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    let id = add_package(
        &conn,
        "WEB-01",
        "pkg:deb/debian/openssl?distro=debian-12",
        "3.0.11-1~deb12u2",
    );
    conn.execute(
        "UPDATE component SET updated_at = '2026-11-01T00:00:00+00:00' WHERE id = ?1",
        [id],
    )
    .unwrap();
    mount_osv(&h).await;
    let options = SyncOptions::default();
    osv::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert!(match_is_stale(&conn).unwrap(), "新命中");
    meta::delete(&conn, MATCH_STALE_KEY).unwrap();
    conn.execute(
        "INSERT INTO sync_run (started_at, finished_at, source, status)
         VALUES (?1, ?1, 'osv', 'success')",
        [NOW],
    )
    .unwrap();
    osv::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert!(!match_is_stale(&conn).unwrap(), "命中沒有變動");
    // 套件在上次成功同步後變更 → 這次同步後即可比對
    conn.execute(
        "UPDATE component SET updated_at = '2026-11-08T00:00:00+00:00' WHERE id = ?1",
        [id],
    )
    .unwrap();
    osv::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert!(match_is_stale(&conn).unwrap(), "等待 OSV 的套件");
}

/// 第十九輪 #5：KEV 只有列入、移出或列入日期改變才需要重新比對。
#[tokio::test]
async fn kev_edits_to_other_fields_do_not_mark_match_stale() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    Mock::given(method("GET"))
        .and(path("/kev.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json("kev/kev.json")))
        .mount(&h.server)
        .await;
    let options = SyncOptions::default();
    let ctx = h.ctx(&options, at(NOW));
    kev::sync(&ctx, &mut conn).await.unwrap();
    assert!(match_is_stale(&conn).unwrap(), "新列入");
    meta::delete(&conn, MATCH_STALE_KEY).unwrap();
    let reset = "UPDATE kev SET due_date = '2000-01-01', required_action = 'x'
                 WHERE cve_id = 'CVE-2024-3400'";
    conn.execute(reset, []).unwrap();
    let stats = kev::sync(&ctx, &mut conn).await.unwrap();
    assert_eq!(stats.updated, 1);
    assert!(!match_is_stale(&conn).unwrap(), "只有期限與措施改變");
    conn.execute(
        "UPDATE kev SET date_added = '2000-01-01' WHERE cve_id = 'CVE-2024-3400'",
        [],
    )
    .unwrap();
    kev::sync(&ctx, &mut conn).await.unwrap();
    assert!(match_is_stale(&conn).unwrap(), "列入日期改變");
}

/// 第十九輪 #3、#4：EPSS 只有分數改變才需要重新比對；OSV 命中 JSON 損壞不讓 EPSS 同步失敗。
#[tokio::test]
async fn epss_percentile_only_change_and_corrupt_osv_json() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    for id in ["CVE-2024-3400", "CVE-2025-0108"] {
        stub(&conn, id);
    }
    let pkg = add_package(&conn, "WEB-01", "pkg:deb/debian/openssl", "3.0.11-1");
    conn.execute(
        "INSERT INTO osv_hit (component_id, osv_id, cve_ids_json, fixed_versions_json, queried_at)
         VALUES (?1, 'X-1', '{損壞', '[]', 't')",
        [pkg],
    )
    .unwrap();
    Mock::given(method("GET"))
        .and(path("/epss"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json("epss/epss.json")))
        .mount(&h.server)
        .await;
    let options = SyncOptions::default();
    epss::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .expect("損壞的 OSV 命中不影響 EPSS 同步");
    meta::delete(&conn, MATCH_STALE_KEY).unwrap();
    conn.execute(
        "UPDATE epss SET percentile = 0.5, fetched_on = '2026-11-06'",
        [],
    )
    .unwrap();
    epss::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert!(!match_is_stale(&conn).unwrap(), "只有百分位數改變");
    conn.execute("UPDATE epss SET score = 0.1, fetched_on = '2026-11-06'", [])
        .unwrap();
    epss::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert!(match_is_stale(&conn).unwrap(), "分數改變");
}
