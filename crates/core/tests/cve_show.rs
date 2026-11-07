//! 7.2：`cve show` 的整合資料（CVSS、affected、KEV、EPSS、原廠公告、OSV、參考連結）與 `cve search`。

mod common;

use common::ingest::catalog_with;
use cvewatch_core::catalog::store::sync_catalog;
use cvewatch_core::ingest::cve5;
use cvewatch_core::ingest::cvelist::search_index::IndexWriter;
use cvewatch_core::ingest::cvss::MetricSource;
use cvewatch_core::ingest::relevance::Relevance;
use cvewatch_core::ingest::show;
use cvewatch_core::ingest::store::upsert_record;
use rusqlite::{params, Connection};

const NOW: &str = "2026-11-07T00:00:00Z";

fn insert(conn: &Connection, id: &str) {
    let raw = common::read_ingest(&format!("cvelist/records/{id}.json"));
    let rec = cve5::parse(&raw).unwrap();
    upsert_record(conn, &rec, &raw, Relevance::Cna, NOW).unwrap();
}

fn seed_extras(conn: &Connection) {
    let ids = sync_catalog(conn, &catalog_with(None)).unwrap();
    let panos = ids["paloalto-panos"];
    conn.execute_batch(
        "INSERT INTO kev (cve_id, date_added, due_date, known_ransomware)
             VALUES ('CVE-2024-3400', '2024-04-12', '2024-04-19', 1);
         INSERT INTO epss (cve_id, score, percentile, score_date, fetched_on)
             VALUES ('CVE-2024-3400', 0.99999, 1.0, '2026-11-06', '2026-11-07');
         INSERT INTO cve_metric (cve_id, source, provider, version, base_score, severity)
             VALUES ('CVE-2024-3400', 'nvd', 'nvd@nist.gov', '3.1', 10.0, 'CRITICAL');
         INSERT INTO csaf_document (provider, tracking_id, version, url, raw, fetched_at)
             VALUES ('paloalto', 'CVE-2024-3400', '3', 'u', x'00', 't');
         INSERT INTO asset (asset_tag, name, category, first_seen, last_seen)
             VALUES ('WEB-01', 'web', 'host', 't', 't');
         INSERT INTO component (asset_id, kind, purl, purl_full, version_raw, source, updated_at)
             VALUES (1, 'package', 'pkg:deb/debian/openssl?distro=debian-12', 'p', '3.0', 'dpkg', 't');
         INSERT INTO osv_hit (component_id, osv_id, cve_ids_json, fixed_versions_json, queried_at)
             VALUES (1, 'DEBIAN-CVE-2024-3400', '[\"CVE-2024-3400\"]', '[]', 't');",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO csaf_status (provider, tracking_id, cve_id, catalog_product_id, csaf_product_id,
                                  version_expr, kind, status)
         VALUES ('paloalto', 'CVE-2024-3400', 'CVE-2024-3400', ?1, 'P1',
                 'vers:generic/>=11.1.0|<11.1.2-h3', 'range', 'affected')",
        params![panos],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO csaf_fix (provider, tracking_id, cve_id, catalog_product_id, fixed_version, url)
         VALUES ('paloalto', 'CVE-2024-3400', 'CVE-2024-3400', ?1, '11.1.2-h3', 'https://x')",
        params![panos],
    )
    .unwrap();
}

#[test]
fn detail_combines_all_sources() {
    let conn = common::memory_db();
    insert(&conn, "CVE-2024-3400");
    seed_extras(&conn);
    let d = show::load(&conn, "CVE-2024-3400").unwrap().unwrap();
    assert_eq!(d.state, "PUBLISHED");
    assert!(d.has_record);
    assert!(d.metrics.iter().any(|m| m.source == MetricSource::Cna));
    assert!(d.metrics.iter().any(|m| m.source == MetricSource::Nvd));
    assert_eq!(d.preferred_metric().unwrap().source, MetricSource::Cna);
    assert!(d.affected.iter().any(|a| a.product == "PAN-OS"));
    assert!(d.ssvc.is_some());
    let kev = d.kev.unwrap();
    assert!(kev.ransomware);
    assert_eq!(kev.due_date.as_deref(), Some("2024-04-19"));
    assert_eq!(d.epss.unwrap().2, "2026-11-06");
    assert_eq!(d.advisories.len(), 1);
    assert_eq!(d.advisories[0].product, "PAN-OS");
    assert_eq!(d.fixes[0].fixed_version.as_deref(), Some("11.1.2-h3"));
    assert_eq!(
        d.osv,
        vec!["WEB-01 pkg:deb/debian/openssl?distro=debian-12 → DEBIAN-CVE-2024-3400"]
    );
    assert!(!d.references.is_empty() && d.references.len() <= show::MAX_REFERENCES);
}

#[test]
fn rejected_and_missing_cves() {
    let conn = common::memory_db();
    insert(&conn, "CVE-2019-7907");
    let d = show::load(&conn, "CVE-2019-7907").unwrap().unwrap();
    assert_eq!(d.state, "REJECTED");
    assert!(show::load(&conn, "CVE-2099-99999").unwrap().is_none());
}

#[test]
fn search_marks_local_records() {
    let conn = common::memory_db();
    insert(&conn, "CVE-2024-3400");
    let dir = tempfile::tempdir().unwrap();
    assert!(show::search(&conn, dir.path(), "pan-os").unwrap().is_none());
    let mut w = IndexWriter::rebuild(dir.path()).unwrap();
    for id in ["CVE-2024-3400", "CVE-2025-0108", "CVE-2026-103479"] {
        let raw = common::read_ingest(&format!("cvelist/records/{id}.json"));
        w.add(&cve5::parse(&raw).unwrap()).unwrap();
    }
    w.finish().unwrap();
    let hits = show::search(&conn, dir.path(), "PAN-OS").unwrap().unwrap();
    let summary: Vec<(&str, bool)> = hits
        .iter()
        .map(|h| (h.entry.id.as_str(), h.local))
        .collect();
    assert_eq!(
        summary,
        vec![("CVE-2024-3400", true), ("CVE-2025-0108", false)]
    );
    assert!(show::search(&conn, dir.path(), "  ").is_err());
}
