//! 6.6：Palo Alto CSAF（RSS）、OASIS 規格範例、provider-metadata（changes.csv / ROLIE）、
//! 雜湊不符、TLP、版本更新取代、無法對應的產品被忽略。

mod common;

use common::ingest::{at, harness, Harness};
use cvewatch_core::config::{CsafDiscovery, CsafSourceConfig};
use cvewatch_core::ingest::csaf::{self, AdvisorySource, CsafCursor, CsafReader};
use cvewatch_core::ingest::cursor;
use cvewatch_core::ingest::source::{SyncOptions, SyncStats};
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use sha2::{Digest, Sha512};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const NOW: &str = "2026-11-07T03:30:00Z";

const TEST_CATALOG: &str = r#"
[[product]]
key = "example-widget"
display_name = "Widget OS"
vendor_display = "Example Corp"
version_scheme = "dotted"
csaf = [{ vendor = "Example Corp", product = "Widget OS" }]

[[product]]
key = "csaf-converter"
display_name = "CVRF-CSAF-Converter"
vendor_display = "CSAF Tools"
version_scheme = "dotted"
csaf = [{ vendor = "CSAF Tools", product = "CVRF-CSAF-Converter" }]
"#;

fn fixture_text(h: &Harness, rel: &str) -> String {
    String::from_utf8(common::read_ingest(rel))
        .unwrap()
        .replace("{{BASE}}", &h.server.uri())
}

async fn serve(h: &Harness, p: &str, body: Vec<u8>, times: Option<u64>) {
    let mock = Mock::given(method("GET"))
        .and(path(p))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body));
    match times {
        Some(n) => mock.expect(n).mount(&h.server).await,
        None => mock.mount(&h.server).await,
    }
}

fn sha512_line(body: &[u8], name: &str) -> Vec<u8> {
    let hex = data_encoding::HEXLOWER.encode(&Sha512::digest(body));
    format!("{hex}  {name}\n").into_bytes()
}

fn provider(h: &Harness, file: &str) -> CsafReader {
    CsafReader {
        config: CsafSourceConfig {
            id: "example".into(),
            discovery: CsafDiscovery::ProviderMetadata,
            url: format!("{}/pm/{file}", h.server.uri()),
            csaf_url_template: None,
            enabled: true,
        },
    }
}

async fn run(h: &Harness, conn: &mut Connection, reader: &CsafReader, now: &str) -> SyncStats {
    let options = SyncOptions::default();
    reader.sync(&h.ctx(&options, at(now)), conn).await.unwrap()
}

fn set_cursor(conn: &Connection, id: &str, t: &str) {
    let c = CsafCursor {
        last_updated: Some(at(t)),
        catalog_hash: None,
    };
    cursor::set(conn, &csaf::source_name(id), &c, NOW).unwrap();
}

fn rows(conn: &Connection, sql: &str) -> Vec<Vec<String>> {
    let mut stmt = conn.prepare(sql).unwrap();
    let n = stmt.column_count();
    stmt.query_map([], |r| {
        (0..n)
            .map(|i| {
                Ok(match r.get_ref(i)? {
                    ValueRef::Null => "NULL".to_string(),
                    ValueRef::Integer(v) => v.to_string(),
                    ValueRef::Real(v) => v.to_string(),
                    ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                    ValueRef::Blob(_) => "<blob>".to_string(),
                })
            })
            .collect()
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

// ── Palo Alto（RSS + CSAF 2.1）────────────────────────────────

#[tokio::test]
async fn paloalto_rss_advisory_is_parsed() {
    let mut conn = common::memory_db();
    let h = harness(&conn, None, vec![]).await;
    serve(
        &h,
        "/pa/rss.xml",
        common::read_ingest("csaf/paloalto/rss.xml"),
        None,
    )
    .await;
    serve(
        &h,
        "/pa/csaf/CVE-2026-0322",
        common::read_ingest("csaf/paloalto/CVE-2026-0322.json"),
        Some(1),
    )
    .await;
    // 其他 RSS item 的 CSAF 不存在（404）→ 略過，不影響其他文件
    let reader = CsafReader {
        config: CsafSourceConfig {
            id: "paloalto".into(),
            discovery: CsafDiscovery::Rss,
            url: format!("{}/pa/rss.xml", h.server.uri()),
            csaf_url_template: Some(format!("{}/pa/csaf/{{id}}", h.server.uri())),
            enabled: true,
        },
    };
    let stats = run(&h, &mut conn, &reader, NOW).await;
    assert_eq!(stats.added, 1, "{stats:?}");
    assert_eq!(
        stats.notes.iter().filter(|n| n.contains("404")).count(),
        2,
        "{stats:?}"
    );

    let doc = rows(
        &conn,
        "SELECT tracking_id, version, tlp, sha_ok FROM csaf_document",
    );
    assert_eq!(doc, vec![vec!["CVE-2026-0322", "1", "CLEAR", "NULL"]]);
    let affected = rows(
        &conn,
        "SELECT version_expr FROM csaf_status WHERE status = 'affected' AND kind = 'range'
         ORDER BY version_expr",
    );
    assert!(
        affected.contains(&vec!["vers:generic/>=11.1.0".to_string()]),
        "{affected:?}"
    );
    let fixed = rows(
        &conn,
        "SELECT version_expr FROM csaf_status WHERE status = 'fixed'",
    );
    assert!(fixed.contains(&vec!["vers:generic/>=11.1.4-h37".to_string()]));
    // Cloud NGFW、Prisma Access 不在目錄 → 忽略
    let ignored = rows(
        &conn,
        "SELECT count(*) FROM csaf_status WHERE csaf_product_id NOT LIKE 'PANW-PAN-OS-%'",
    );
    assert_eq!(ignored, vec![vec!["0"]]);
    let cve = rows(
        &conn,
        "SELECT relevance_reason FROM cve WHERE id = 'CVE-2026-0322'",
    );
    assert_eq!(cve, vec![vec!["csaf"]]);
    let c: CsafCursor = cursor::get(&conn, "csaf:paloalto").unwrap().unwrap();
    assert_eq!(c.last_updated, Some(at("2026-10-31T03:15:00Z")));
}

// ── provider-metadata：目錄式 changes.csv ─────────────────────

async fn mount_directory(h: &Harness) {
    serve(
        h,
        "/pm/provider-metadata-dir.json",
        fixture_text(h, "csaf/provider/provider-metadata-dir.json").into_bytes(),
        None,
    )
    .await;
    serve(
        h,
        "/csaf/white/changes.csv",
        common::read_ingest("csaf/provider/changes.csv"),
        None,
    )
    .await;
}

fn doc_bytes(name: &str) -> Vec<u8> {
    common::read_ingest(&format!("csaf/provider/docs/{name}"))
}

#[tokio::test]
async fn changes_csv_downloads_only_newer_documents() {
    let mut conn = common::memory_db();
    let h = harness(&conn, Some(TEST_CATALOG), vec![]).await;
    mount_directory(&h).await;
    set_cursor(&conn, "example", "2026-10-15T00:00:00Z");
    let d3 = doc_bytes("ex-2026-003.json");
    serve(&h, "/csaf/white/2026/ex-2026-003.json", d3.clone(), Some(1)).await;
    serve(
        &h,
        "/csaf/white/2026/ex-2026-003.json.sha512",
        sha512_line(&d3, "ex-2026-003.json"),
        Some(1),
    )
    .await;
    serve(
        &h,
        "/csaf/white/2026/ex-2026-004.json",
        doc_bytes("ex-2026-004.json"),
        Some(1),
    )
    .await;
    serve(
        &h,
        "/csaf/white/2026/ex-2026-002.json",
        doc_bytes("ex-2026-002.json"),
        Some(0),
    )
    .await;
    serve(
        &h,
        "/csaf/white/2026/ex-2026-001.json",
        doc_bytes("ex-2026-001.v1.json"),
        Some(0),
    )
    .await;

    let stats = run(
        &h,
        &mut conn,
        &provider(&h, "provider-metadata-dir.json"),
        NOW,
    )
    .await;
    assert_eq!(stats.added, 1, "{stats:?}");
    assert!(
        stats.notes.iter().any(|n| n.contains("TLP:AMBER")),
        "{stats:?}"
    );
    let docs = rows(&conn, "SELECT tracking_id, sha_ok FROM csaf_document");
    assert_eq!(docs, vec![vec!["EX-2026-003", "1"]]);

    // 狀態對應：range 受影響、固定版本 fixed、not_affected 附理由；Gadget 不在目錄被忽略
    let statuses = rows(
        &conn,
        "SELECT csaf_product_id, kind, version_expr, status, justification FROM csaf_status
         ORDER BY csaf_product_id",
    );
    assert_eq!(
        statuses,
        vec![
            vec![
                "WOS-199",
                "exact",
                "1.9.9",
                "not_affected",
                "vulnerable_code_not_present"
            ],
            vec!["WOS-241", "exact", "2.4.1", "fixed", "NULL"],
            vec![
                "WOS-RANGE-1",
                "range",
                "vers:generic/>=2.0.0|<2.4.1",
                "affected",
                "NULL"
            ],
        ]
    );
    let fixes = rows(&conn, "SELECT cve_id, fixed_version, url FROM csaf_fix");
    assert_eq!(
        fixes,
        vec![vec![
            "CVE-2099-60003",
            "2.4.1",
            "https://example.invalid/fix/ex-2026-003"
        ]]
    );
}

#[tokio::test]
async fn hash_mismatch_skips_only_that_document() {
    let mut conn = common::memory_db();
    let h = harness(&conn, Some(TEST_CATALOG), vec![]).await;
    mount_directory(&h).await;
    set_cursor(&conn, "example", "2026-09-15T00:00:00Z");
    serve(
        &h,
        "/csaf/white/2026/ex-2026-003.json",
        doc_bytes("ex-2026-003.json"),
        None,
    )
    .await;
    serve(
        &h,
        "/csaf/white/2026/ex-2026-003.json.sha512",
        sha512_line(b"something else", "ex-2026-003.json"),
        None,
    )
    .await;
    serve(
        &h,
        "/csaf/white/2026/ex-2026-002.json",
        doc_bytes("ex-2026-002.json"),
        None,
    )
    .await;
    serve(
        &h,
        "/csaf/white/2026/ex-2026-004.json",
        doc_bytes("ex-2026-004.json"),
        None,
    )
    .await;
    let stats = run(
        &h,
        &mut conn,
        &provider(&h, "provider-metadata-dir.json"),
        NOW,
    )
    .await;
    assert!(
        stats.notes.iter().any(|n| n.contains("雜湊不符")),
        "{stats:?}"
    );
    let docs = rows(&conn, "SELECT tracking_id, sha_ok FROM csaf_document");
    assert_eq!(docs, vec![vec!["EX-2026-002", "NULL"]]);
}

#[tokio::test]
async fn revised_advisory_replaces_previous_version() {
    let mut conn = common::memory_db();
    let h = harness(&conn, Some(TEST_CATALOG), vec![]).await;
    serve(
        &h,
        "/pm/provider-metadata-dir.json",
        fixture_text(&h, "csaf/provider/provider-metadata-dir.json").into_bytes(),
        None,
    )
    .await;
    let csv_v1 = b"\"2026/ex-2026-001.json\",\"2026-09-01T08:00:00+00:00\"\n".to_vec();
    let csv_v2 = b"\"2026/ex-2026-001.json\",\"2026-11-06T08:00:00+00:00\"\n".to_vec();
    Mock::given(method("GET"))
        .and(path("/csaf/white/changes.csv"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(csv_v1))
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    serve(&h, "/csaf/white/changes.csv", csv_v2, None).await;
    Mock::given(method("GET"))
        .and(path("/csaf/white/2026/ex-2026-001.json"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(doc_bytes("ex-2026-001.v1.json")))
        .up_to_n_times(1)
        .mount(&h.server)
        .await;
    serve(
        &h,
        "/csaf/white/2026/ex-2026-001.json",
        doc_bytes("ex-2026-001.v2.json"),
        None,
    )
    .await;
    let reader = provider(&h, "provider-metadata-dir.json");

    let stats = run(&h, &mut conn, &reader, NOW).await;
    assert_eq!(stats.added, 1);
    let stats = run(&h, &mut conn, &reader, "2026-11-07T05:00:00Z").await;
    assert_eq!((stats.added, stats.updated), (0, 1), "{stats:?}");
    assert_eq!(
        rows(&conn, "SELECT version FROM csaf_document"),
        vec![vec!["1.1.0"]]
    );
    let statuses = rows(
        &conn,
        "SELECT csaf_product_id, status FROM csaf_status ORDER BY csaf_product_id",
    );
    assert_eq!(
        statuses,
        vec![
            vec!["WOS-199", "under_investigation"],
            vec!["WOS-241", "fixed"],
            vec!["WOS-RANGE-1", "affected"],
        ]
    );
    // workaround 不是修正版本
    assert_eq!(
        rows(&conn, "SELECT details FROM csaf_fix"),
        vec![vec!["Upgrade to 2.4.1"]]
    );
}

// ── provider-metadata：ROLIE feed ────────────────────────────

#[tokio::test]
async fn rolie_feed_with_hash_links_and_tlp_feeds() {
    let mut conn = common::memory_db();
    let h = harness(&conn, Some(TEST_CATALOG), vec![]).await;
    serve(
        &h,
        "/pm/provider-metadata-rolie.json",
        fixture_text(&h, "csaf/provider/provider-metadata-rolie.json").into_bytes(),
        None,
    )
    .await;
    serve(
        &h,
        "/csaf/feed-white.json",
        fixture_text(&h, "csaf/provider/feed-white.json").into_bytes(),
        None,
    )
    .await;
    serve(&h, "/csaf/feed-green.json", b"{}".to_vec(), Some(0)).await;
    for name in ["ex-2026-002", "ex-2026-003"] {
        let body = doc_bytes(&format!("{name}.json"));
        let hash = sha512_line(&body, &format!("{name}.json"));
        serve(&h, &format!("/csaf/white/2026/{name}.json"), body, Some(1)).await;
        serve(
            &h,
            &format!("/csaf/white/2026/{name}.json.sha512"),
            hash,
            Some(1),
        )
        .await;
    }
    let stats = run(
        &h,
        &mut conn,
        &provider(&h, "provider-metadata-rolie.json"),
        NOW,
    )
    .await;
    assert_eq!(stats.added, 2, "{stats:?}");
    assert_eq!(
        rows(&conn, "SELECT count(*) FROM csaf_document WHERE sha_ok = 1"),
        vec![vec!["2"]]
    );
}

// ── OASIS 規格範例 ───────────────────────────────────────────

#[tokio::test]
async fn oasis_example_document() {
    let mut conn = common::memory_db();
    let h = harness(&conn, Some(TEST_CATALOG), vec![]).await;
    serve(
        &h,
        "/pm/provider-metadata-dir.json",
        fixture_text(&h, "csaf/provider/provider-metadata-dir.json").into_bytes(),
        None,
    )
    .await;
    serve(
        &h,
        "/csaf/white/changes.csv",
        b"\"2022/bsi-2022-0001.json\",\"2026-11-01T00:00:00+00:00\"\n".to_vec(),
        None,
    )
    .await;
    serve(
        &h,
        "/csaf/white/2022/bsi-2022-0001.json",
        common::read_ingest("csaf/oasis/bsi-2022-0001.json"),
        Some(1),
    )
    .await;
    let stats = run(
        &h,
        &mut conn,
        &provider(&h, "provider-metadata-dir.json"),
        NOW,
    )
    .await;
    assert_eq!(stats.added, 1, "{stats:?}");
    assert_eq!(
        rows(
            &conn,
            "SELECT count(*) FROM csaf_status WHERE cve_id = 'CVE-2022-27193' AND status = 'affected'"
        ),
        vec![vec!["5"]]
    );
    assert_eq!(
        rows(&conn, "SELECT fixed_version FROM csaf_fix"),
        vec![vec!["1.0.0-rc2"]]
    );
}

// ── 審查修正 ─────────────────────────────────────────────────

#[tokio::test]
async fn hash_server_error_fails_source_and_keeps_cursor() {
    let mut conn = common::memory_db();
    let h = harness(&conn, Some(TEST_CATALOG), vec![]).await;
    mount_directory(&h).await;
    set_cursor(&conn, "example", "2026-11-01T00:00:00Z");
    serve(
        &h,
        "/csaf/white/2026/ex-2026-003.json",
        doc_bytes("ex-2026-003.json"),
        None,
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/csaf/white/2026/ex-2026-003.json.sha512"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&h.server)
        .await;
    let options = SyncOptions::default();
    let reader = provider(&h, "provider-metadata-dir.json");
    assert!(reader
        .sync(&h.ctx(&options, at(NOW)), &mut conn)
        .await
        .is_err());
    let c: CsafCursor = cursor::get(&conn, "csaf:example").unwrap().unwrap();
    assert_eq!(
        c.last_updated,
        Some(at("2026-11-01T00:00:00Z")),
        "游標不可越過未驗證的文件"
    );
    assert_eq!(
        rows(&conn, "SELECT count(*) FROM csaf_document"),
        vec![vec!["0"]]
    );
}

#[tokio::test]
async fn catalog_change_remaps_stored_advisories() {
    let mut conn = common::memory_db();
    // 目錄起初沒有 Widget OS → 文件保存但沒有狀態列
    let mut h = harness(&conn, None, vec![]).await;
    mount_directory(&h).await;
    set_cursor(&conn, "example", "2026-11-01T00:00:00Z");
    serve(
        &h,
        "/csaf/white/2026/ex-2026-003.json",
        doc_bytes("ex-2026-003.json"),
        Some(1),
    )
    .await;
    serve(
        &h,
        "/csaf/white/2026/ex-2026-004.json",
        doc_bytes("ex-2026-004.json"),
        None,
    )
    .await;
    let reader = provider(&h, "provider-metadata-dir.json");
    run(&h, &mut conn, &reader, NOW).await;
    assert_eq!(
        rows(&conn, "SELECT count(*) FROM csaf_status"),
        vec![vec!["0"]]
    );

    // 加入 Widget OS 後再 sync：沒有新文件，但既有公告重新對應
    h.set_catalog(&conn, Some(TEST_CATALOG));
    let stats = run(&h, &mut conn, &reader, "2026-11-07T05:00:00Z").await;
    assert!(
        stats.notes.iter().any(|n| n.contains("重新對應 1 份")),
        "{stats:?}"
    );
    assert_eq!(
        rows(&conn, "SELECT count(*) FROM csaf_status"),
        vec![vec!["3"]]
    );
    assert_eq!(
        rows(
            &conn,
            "SELECT relevance_reason FROM cve WHERE id = 'CVE-2099-60003'"
        ),
        vec![vec!["csaf"]]
    );
    // 目錄未再變更 → 不重複對應
    let stats = run(&h, &mut conn, &reader, "2026-11-07T06:00:00Z").await;
    assert!(
        !stats.notes.iter().any(|n| n.contains("重新對應")),
        "{stats:?}"
    );
}

#[tokio::test]
async fn invalid_cve_ids_in_advisory_are_skipped() {
    let mut conn = common::memory_db();
    let h = harness(&conn, Some(TEST_CATALOG), vec![]).await;
    mount_directory(&h).await;
    set_cursor(&conn, "example", "2026-11-01T00:00:00Z");
    let mut doc: serde_json::Value =
        serde_json::from_slice(&doc_bytes("ex-2026-003.json")).unwrap();
    let mut second = doc["vulnerabilities"][0].clone();
    second["cve"] = "cve-2099-60099 ".into();
    doc["vulnerabilities"][0]["cve"] = "CVE-99".into();
    doc["vulnerabilities"].as_array_mut().unwrap().push(second);
    serve(
        &h,
        "/csaf/white/2026/ex-2026-003.json",
        serde_json::to_vec(&doc).unwrap(),
        None,
    )
    .await;
    serve(
        &h,
        "/csaf/white/2026/ex-2026-004.json",
        doc_bytes("ex-2026-004.json"),
        None,
    )
    .await;
    let stats = run(
        &h,
        &mut conn,
        &provider(&h, "provider-metadata-dir.json"),
        NOW,
    )
    .await;
    assert!(
        stats.notes.iter().any(|n| n.contains("CVE-99")),
        "{stats:?}"
    );
    // 小寫、帶空白的 ID 標準化後保存
    assert_eq!(
        rows(&conn, "SELECT id FROM cve ORDER BY id"),
        vec![vec!["CVE-2099-60099"]]
    );
}
