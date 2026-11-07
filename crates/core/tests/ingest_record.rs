//! 2.4：CVE 5.0/5.1/5.2 fixture 解析、顯示用 CVSS 優先序、REJECTED 標記與原始 JSON 壓縮保存。

mod common;

use cvewatch_core::ingest::compress;
use cvewatch_core::ingest::cve5::{self, CveState};
use cvewatch_core::ingest::cvss::{self, MetricRow, MetricSource};
use cvewatch_core::ingest::relevance::Relevance;
use cvewatch_core::ingest::store::{upsert_record, UpsertOutcome};
use rusqlite::{params, Connection};

const NOW: &str = "2026-11-07T00:00:00Z";

fn record(id: &str) -> (cve5::CveRecord, Vec<u8>) {
    let raw = common::read_ingest(&format!("cvelist/records/{id}.json"));
    (cve5::parse(&raw).expect("parse"), raw)
}

fn insert(conn: &Connection, id: &str) -> UpsertOutcome {
    let (rec, raw) = record(id);
    upsert_record(conn, &rec, &raw, Relevance::Cna, NOW).unwrap()
}

#[test]
fn every_fixture_parses() {
    let dir = common::ingest_fixture("cvelist/records");
    let mut versions = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let rec = cve5::parse(&std::fs::read(&path).unwrap())
            .unwrap_or_else(|e| panic!("{}：{e}", path.display()));
        assert!(rec.id.starts_with("CVE-"), "{}", path.display());
        versions.insert(rec.data_version.unwrap_or_default());
    }
    assert_eq!(
        versions.into_iter().collect::<Vec<_>>(),
        vec!["5.0", "5.1", "5.2"]
    );
}

#[test]
fn v5_0_record_fields() {
    let (rec, _) = record("CVE-2099-50001");
    assert_eq!(rec.data_version.as_deref(), Some("5.0"));
    assert_eq!(rec.state, CveState::Published);
    assert_eq!(rec.cna.affected[0].product.as_deref(), Some("VMware ESXi"));
    assert_eq!(rec.cna.metrics[0].version, "3.1");
    assert_eq!(rec.cna.metrics[0].base_score, 8.2);
}

#[test]
fn v5_1_record_with_adp_ssvc() {
    let (rec, _) = record("CVE-2024-3400");
    assert_eq!(rec.data_version.as_deref(), Some("5.1"));
    assert_eq!(rec.assigner_short.as_deref(), Some("palo_alto"));
    assert!(rec
        .cna
        .affected
        .iter()
        .any(|a| a.vendor.as_deref() == Some("Palo Alto Networks")
            && a.product.as_deref() == Some("PAN-OS")
            && !a.versions.is_empty()));
    let cisa = rec
        .adp
        .iter()
        .find(|a| a.provider.as_deref() == Some("CISA-ADP"))
        .expect("CISA-ADP container");
    let ssvc = cisa.ssvc.as_ref().expect("SSVC");
    assert_eq!(ssvc.exploitation.as_deref(), Some("active"));
    assert!(rec.title.is_some() && rec.description_en.is_some());
}

#[test]
fn v5_2_record_with_package_url() {
    let (rec, _) = record("CVE-2026-108949");
    assert_eq!(rec.data_version.as_deref(), Some("5.2"));
    assert_eq!(
        rec.cna.affected[0].package_url.as_deref(),
        Some("pkg:maven/org.apache.activemq/activemq-broker")
    );
}

#[test]
fn rejected_record_is_kept_and_marked() {
    let conn = common::memory_db();
    let (rec, _) = record("CVE-2019-7907");
    assert_eq!(rec.state, CveState::Rejected);
    // 撤銷紀錄沒有 descriptions，改用 rejectedReasons
    assert!(rec.description_en.is_some());
    assert_eq!(insert(&conn, "CVE-2019-7907"), UpsertOutcome::Inserted);
    let state: String = conn
        .query_row(
            "SELECT state FROM cve WHERE id = 'CVE-2019-7907'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "REJECTED");
}

#[test]
fn published_then_rejected_updates_state() {
    let conn = common::memory_db();
    assert_eq!(insert(&conn, "CVE-2024-3400"), UpsertOutcome::Inserted);
    let mut raw: serde_json::Value =
        serde_json::from_slice(&common::read_ingest("cvelist/records/CVE-2024-3400.json")).unwrap();
    raw["cveMetadata"]["state"] = "REJECTED".into();
    raw["cveMetadata"]["dateUpdated"] = "2026-12-01T00:00:00.000Z".into();
    let bytes = serde_json::to_vec(&raw).unwrap();
    let rec = cve5::parse(&bytes).unwrap();
    assert_eq!(
        upsert_record(&conn, &rec, &bytes, Relevance::Cna, NOW).unwrap(),
        UpsertOutcome::Updated
    );
    let state: String = conn
        .query_row(
            "SELECT state FROM cve WHERE id = 'CVE-2024-3400'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "REJECTED");
}

#[test]
fn same_date_updated_is_unchanged() {
    let conn = common::memory_db();
    assert_eq!(insert(&conn, "CVE-2025-0108"), UpsertOutcome::Inserted);
    assert_eq!(insert(&conn, "CVE-2025-0108"), UpsertOutcome::Unchanged);
    let metrics: i64 = conn
        .query_row(
            "SELECT count(*) FROM cve_metric WHERE cve_id = 'CVE-2025-0108'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(metrics >= 2, "CNA 4.0 兩個情境");
}

#[test]
fn raw_json_is_stored_compressed() {
    let conn = common::memory_db();
    insert(&conn, "CVE-2024-3400");
    let blob: Vec<u8> = conn
        .query_row(
            "SELECT raw_cna_json FROM cve WHERE id = 'CVE-2024-3400'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let raw = common::read_ingest("cvelist/records/CVE-2024-3400.json");
    assert!(blob.len() < raw.len());
    assert_eq!(compress::decompress(&blob).unwrap(), raw);
}

#[test]
fn affected_rows_are_normalized() {
    let conn = common::memory_db();
    insert(&conn, "CVE-2024-3400");
    let (vendor, product): (String, String) = conn
        .query_row(
            "SELECT vendor_norm, product_norm FROM cve_affected
             WHERE cve_id = 'CVE-2024-3400' AND source = 'cna' AND product_raw = 'PAN-OS'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (vendor.as_str(), product.as_str()),
        ("paloaltonetworks", "panos")
    );
}

// ── 顯示用 CVSS ──────────────────────────────────────────────

fn row(source: MetricSource, version: &str, score: f64) -> MetricRow {
    MetricRow {
        source,
        provider: None,
        version: version.into(),
        base_score: score,
        vector: None,
        severity: None,
    }
}

#[test]
fn cvss_precedence_order() {
    let all = vec![
        row(MetricSource::Nvd, "3.1", 7.5),
        row(MetricSource::Adp, "3.1", 8.0),
        row(MetricSource::Cna, "3.1", 9.0),
        row(MetricSource::Cna, "4.0", 9.3),
    ];
    let pick = |rows: &[MetricRow]| cvss::preferred(rows).map(|m| (m.source, m.base_score));
    assert_eq!(pick(&all), Some((MetricSource::Cna, 9.3)));
    assert_eq!(pick(&all[..3]), Some((MetricSource::Cna, 9.0)));
    assert_eq!(pick(&all[..2]), Some((MetricSource::Adp, 8.0)));
    assert_eq!(pick(&all[..1]), Some((MetricSource::Nvd, 7.5)));
    // CVSS 2.0 / 3.0 不在優先序內
    assert_eq!(pick(&[row(MetricSource::Cna, "2.0", 5.0)]), None);
}

#[test]
fn several_sources_are_all_retained() {
    let conn = common::memory_db();
    insert(&conn, "CVE-2025-0108");
    conn.execute(
        "INSERT INTO cve_metric (cve_id, source, provider, version, base_score, severity)
         VALUES ('CVE-2025-0108', 'nvd', 'nvd@nist.gov', '3.1', 9.1, 'CRITICAL')",
        [],
    )
    .unwrap();
    let rows = cvss::load_metrics(&conn, "CVE-2025-0108").unwrap();
    assert!(rows
        .iter()
        .any(|m| m.source == MetricSource::Cna && m.version == "4.0"));
    assert!(rows
        .iter()
        .any(|m| m.source == MetricSource::Nvd && m.version == "3.1"));
    assert_eq!(cvss::preferred(&rows).unwrap().source, MetricSource::Cna);
}

#[test]
fn only_nvd_score_is_displayed() {
    let conn = common::memory_db();
    insert(&conn, "CVE-2020-9709");
    assert!(cvss::load_metrics(&conn, "CVE-2020-9709")
        .unwrap()
        .is_empty());
    conn.execute(
        "INSERT INTO cve_metric (cve_id, source, version, base_score, severity)
         VALUES (?1, 'nvd', '3.1', 7.5, 'HIGH')",
        params!["CVE-2020-9709"],
    )
    .unwrap();
    let rows = cvss::load_metrics(&conn, "CVE-2020-9709").unwrap();
    let chosen = cvss::preferred(&rows).unwrap();
    assert_eq!((chosen.base_score, chosen.source), (7.5, MetricSource::Nvd));
}

#[test]
fn no_score_is_unrated() {
    let conn = common::memory_db();
    insert(&conn, "CVE-2020-9709");
    let rows = cvss::load_metrics(&conn, "CVE-2020-9709").unwrap();
    assert_eq!(cvss::display(cvss::preferred(&rows)), cvss::UNRATED_LABEL);
}

// ── 審查修正：rescan 提升原因、較舊版本不覆寫 ─────────────────

#[test]
fn same_version_still_upgrades_relevance_reason() {
    let conn = common::memory_db();
    let (rec, raw) = record("CVE-2024-3400");
    upsert_record(&conn, &rec, &raw, Relevance::KeywordOnly, NOW).unwrap();
    // 目錄新增 CNA 寫法後 rescan：同一 dateUpdated，但原因由 keyword_only 改為 cna
    assert_eq!(
        upsert_record(&conn, &rec, &raw, Relevance::Cna, NOW).unwrap(),
        UpsertOutcome::Updated
    );
    let reason: String = conn
        .query_row(
            "SELECT relevance_reason FROM cve WHERE id = 'CVE-2024-3400'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reason, "cna");
}

#[test]
fn older_record_does_not_overwrite_newer() {
    let conn = common::memory_db();
    let (rec, raw) = record("CVE-2024-3400");
    let newer = common::ingest::edit_record(&raw, |v| {
        v["cveMetadata"]["dateUpdated"] = "2099-01-01T00:00:00.000Z".into();
        v["containers"]["cna"]["title"] = "newer title".into();
    });
    let newer_rec = cve5::parse(&newer).unwrap();
    upsert_record(&conn, &newer_rec, &newer, Relevance::Cna, NOW).unwrap();
    // --since 重播較舊的 delta
    assert_eq!(
        upsert_record(&conn, &rec, &raw, Relevance::Cna, NOW).unwrap(),
        UpsertOutcome::Unchanged
    );
    let title: String = conn
        .query_row(
            "SELECT title FROM cve WHERE id = 'CVE-2024-3400'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(title, "newer title");
}
