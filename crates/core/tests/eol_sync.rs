//! 22 任務 4.5：`eol` 來源（wiremock）——只執行 eol、endoflife.date 503、查無代稱、沒有元件的
//! 產品不查詢、304 條件請求、301 轉址，且不標記「比對待補」（asset-lifecycle 的 Software
//! end-of-life sync）。

mod common;

use common::ingest::{at, harness, Harness};
use cvewatch_core::config::SourcesConfig;
use cvewatch_core::ingest::eol;
use cvewatch_core::ingest::source::SyncOptions;
use cvewatch_core::ingest::sync::{plan, run};
use cvewatch_core::matching::run::match_is_stale;
use cvewatch_core::ExitCode;
use rusqlite::{params, Connection};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

const NOW: &str = "2026-11-09T03:30:00Z";

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/endoflife")
        .join(name);
    std::fs::read(p).unwrap()
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// 資產與其清冊元件（`status` 為 active／retired）。
fn asset(conn: &Connection, tag: &str, status: &str, products: &[(&str, &str)]) {
    conn.execute(
        "INSERT INTO asset (asset_tag, name, category, exposure, status, first_seen, last_seen)
         VALUES (?1, ?1, 'HW01', 'internal', ?2, ?3, ?3)",
        params![tag, status, NOW],
    )
    .unwrap();
    let id = conn.last_insert_rowid();
    for (key, version) in products {
        conn.execute(
            "INSERT INTO component (asset_id, kind, product_id, version_raw, source, updated_at)
             VALUES (?1, 'product', (SELECT id FROM catalog_product WHERE key = ?2), ?3, 'excel', ?4)",
            params![id, key, version, NOW],
        )
        .unwrap();
    }
}

async fn mount_json(h: &Harness, slug: &str, file: &str, etag: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/eol/products/{slug}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(fixture(file), "application/json")
                .insert_header("ETag", etag),
        )
        .mount(&h.server)
        .await;
}

fn release_eol(conn: &Connection, slug: &str, name: &str) -> Option<String> {
    conn.query_row(
        "SELECT eol_from FROM eol_release WHERE slug = ?1 AND name = ?2",
        [slug, name],
        |r| r.get(0),
    )
    .unwrap()
}

/// Scenario: Refreshing EoL data only
#[tokio::test]
async fn refreshing_eol_data_only() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    asset(&conn, "FW-EDGE", "active", &[("paloalto-panos", "10.2.9")]);
    mount_json(&h, "panos", "panos.json", "\"p1\"").await;
    let plans = plan(&SourcesConfig::default(), &["eol".into()]).unwrap();
    assert_eq!(plans.len(), 1);
    let options = SyncOptions::default();
    let report = run(&h.ctx(&options, at(NOW)), &mut conn, &plans)
        .await
        .unwrap();
    assert_eq!(report.exit_code(), ExitCode::Success);
    assert_eq!(
        release_eol(&conn, "panos", "10.2").as_deref(),
        Some("2027-03-01")
    );
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM eol_release WHERE slug = 'panos'"
        ),
        3
    );
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM sync_cursor WHERE source <> 'eol'"
        ),
        0,
        "其他來源游標不變"
    );
    assert!(!match_is_stale(&conn).unwrap(), "命中不重新比對");
    let status: String = conn
        .query_row(
            "SELECT status FROM sync_run WHERE source = 'eol'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "success");
}

/// Scenario: endoflife.date unavailable（其他代稱已提交、先前資料保留、來源失敗、結束碼 4）
#[tokio::test]
async fn endoflife_unavailable_keeps_previous_data() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    asset(
        &conn,
        "SRV-01",
        "active",
        &[("vmware-esxi", "8.0 U3c"), ("paloalto-panos", "10.2.9")],
    );
    conn.execute(
        "INSERT INTO eol_product (slug, status, fetched_at) VALUES ('esxi', 'ok', ?1)",
        [NOW],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO eol_release (slug, name, is_eol, eol_from, position)
         VALUES ('esxi', '8.0', 0, '2027-10-11', 0)",
        [],
    )
    .unwrap();
    Mock::given(method("GET"))
        .and(path("/eol/products/esxi"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&h.server)
        .await;
    mount_json(&h, "panos", "panos.json", "\"p1\"").await;
    let plans = plan(&SourcesConfig::default(), &["eol".into()]).unwrap();
    let options = SyncOptions::default();
    let report = run(&h.ctx(&options, at(NOW)), &mut conn, &plans)
        .await
        .unwrap();
    assert_eq!(report.exit_code(), ExitCode::PartialFailure);
    let (status, error): (String, String) = conn
        .query_row(
            "SELECT status, error FROM sync_run WHERE source = 'eol'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "failed");
    assert!(error.contains("esxi") && error.contains("503"), "{error}");
    assert_eq!(
        release_eol(&conn, "esxi", "8.0").as_deref(),
        Some("2027-10-11")
    );
    assert_eq!(
        release_eol(&conn, "panos", "10.2").as_deref(),
        Some("2027-03-01"),
        "成功的代稱已提交"
    );
}

/// Scenario: Unknown product slug
#[tokio::test]
async fn unknown_product_slug_is_a_note() {
    let mut conn = common::memory_db();
    let extra = "[[product]]\nkey = \"nginx\"\nendoflife = \"no-such-nginx\"\n";
    let h = harness(&conn, Some(extra), vec![]).await;
    asset(
        &conn,
        "WEB-01",
        "active",
        &[("nginx", "1.99.0"), ("paloalto-panos", "10.2.9")],
    );
    Mock::given(method("GET"))
        .and(path("/eol/products/no-such-nginx"))
        .respond_with(
            ResponseTemplate::new(404).set_body_raw(fixture("not-found.html"), "text/html"),
        )
        .mount(&h.server)
        .await;
    mount_json(&h, "panos", "panos.json", "\"p1\"").await;
    let options = SyncOptions::default();
    let stats = eol::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!(stats.notes.len(), 1);
    assert!(
        stats.notes[0].contains("no-such-nginx") && stats.notes[0].contains("nginx"),
        "{:?}",
        stats.notes
    );
    assert_eq!(
        eol::product_status(&conn, "no-such-nginx")
            .unwrap()
            .as_deref(),
        Some("not_found")
    );
    assert!(
        release_eol(&conn, "panos", "10.2").is_some(),
        "其他產品照常更新"
    );
}

/// Scenario: Products without components are skipped（含 retired 資產與已移除的元件）
#[tokio::test]
async fn products_without_components_are_skipped() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    asset(&conn, "NAS-02", "retired", &[("vmware-esxi", "8.0 U3c")]);
    asset(&conn, "FW-EDGE", "active", &[("paloalto-panos", "")]);
    conn.execute(
        "INSERT INTO component (asset_id, kind, product_id, version_raw, source, updated_at, removed_at)
         VALUES ((SELECT id FROM asset WHERE asset_tag = 'FW-EDGE'), 'product',
                 (SELECT id FROM catalog_product WHERE key = 'nginx'), '1.26', 'excel', ?1, ?1)",
        [NOW],
    )
    .unwrap();
    for slug in ["nginx", "esxi"] {
        Mock::given(method("GET"))
            .and(path(format!("/eol/products/{slug}")))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&h.server)
            .await;
    }
    mount_json(&h, "panos", "panos.json", "\"p1\"").await;
    let slugs = eol::wanted_slugs(&conn, &h.catalog).unwrap();
    assert_eq!(
        slugs.keys().collect::<Vec<_>>(),
        ["panos"],
        "版本待補的元件也查詢"
    );
    let options = SyncOptions::default();
    eol::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
}

#[tokio::test]
async fn second_run_sends_if_none_match_and_keeps_data_on_304() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    asset(
        &conn,
        "SRV-01",
        "active",
        &[("windows-server", "10.0.20348.2700")],
    );
    Mock::given(method("GET"))
        .and(path("/eol/products/windows-server"))
        .and(header("If-None-Match", "\"w1\""))
        .respond_with(ResponseTemplate::new(304))
        .with_priority(1)
        .mount(&h.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/eol/products/windows-server"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(fixture("windows-server.json"), "application/json")
                .insert_header("ETag", "\"w1\""),
        )
        .expect(1)
        .mount(&h.server)
        .await;
    let options = SyncOptions::default();
    let first = eol::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!((first.added, first.updated), (3, 0));
    let later = "2026-11-10T03:30:00Z";
    let second = eol::sync(&h.ctx(&options, at(later)), &mut conn)
        .await
        .unwrap();
    assert_eq!((second.added, second.updated), (0, 0));
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM eol_release WHERE slug = 'windows-server'"
        ),
        3
    );
    let fetched: String = conn
        .query_row(
            "SELECT fetched_at FROM eol_product WHERE slug = 'windows-server'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        fetched.starts_with("2026-11-10"),
        "304 只更新 fetched_at：{fetched}"
    );
    assert!(!match_is_stale(&conn).unwrap());
}

#[tokio::test]
async fn redirect_is_followed() {
    let mut conn = common::memory_db();
    let extra = "[[product]]\nkey = \"paloalto-panos\"\nendoflife = \"pan-os\"\n";
    let h = harness(&conn, Some(extra), vec![]).await;
    asset(&conn, "FW-EDGE", "active", &[("paloalto-panos", "10.2.9")]);
    Mock::given(method("GET"))
        .and(path("/eol/products/pan-os"))
        .respond_with(ResponseTemplate::new(301).insert_header(
            "Location",
            format!("{}/eol/products/panos/", h.server.uri()),
        ))
        .mount(&h.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/eol/products/panos/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(fixture("panos.json"), "application/json"),
        )
        .mount(&h.server)
        .await;
    let options = SyncOptions::default();
    eol::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!(
        release_eol(&conn, "pan-os", "10.2").as_deref(),
        Some("2027-03-01")
    );
}

/// code review：404 之後清除 ETag，代稱恢復時以無條件請求取得、狀態回到 ok。
#[tokio::test]
async fn product_recovers_after_a_transient_404() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    asset(&conn, "FW-EDGE", "active", &[("paloalto-panos", "10.2.9")]);
    let options = SyncOptions::default();
    mount_json(&h, "panos", "panos.json", "\"p1\"").await;
    eol::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    h.server.reset().await;
    Mock::given(method("GET"))
        .and(path("/eol/products/panos"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&h.server)
        .await;
    eol::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    assert_eq!(
        eol::product_status(&conn, "panos").unwrap().as_deref(),
        Some("not_found")
    );
    h.server.reset().await;
    mount_json(&h, "panos", "panos.json", "\"p1\"").await;
    eol::sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .unwrap();
    let requests = h.server.received_requests().await.unwrap();
    assert!(
        requests[0].headers.get("If-None-Match").is_none(),
        "404 後不再帶舊的 ETag"
    );
    assert_eq!(
        eol::product_status(&conn, "panos").unwrap().as_deref(),
        Some("ok")
    );
}
