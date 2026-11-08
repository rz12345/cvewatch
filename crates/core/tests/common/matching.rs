//! 比對測試共用：記憶體資料庫、資產與 CVE 資料建構器、查詢命中與等待區。

use chrono::{DateTime, Utc};
use cvewatch_core::assets::AssetClasses;
use cvewatch_core::catalog::store::sync_catalog;
use cvewatch_core::catalog::Catalog;
use cvewatch_core::ingest::cve5;
use cvewatch_core::ingest::relevance::Relevance;
use cvewatch_core::ingest::store::{ensure_stub, upsert_record};
use cvewatch_core::matching::run::{run_match, MatchReport, MatchSettings};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

pub const T0: &str = "2026-11-01T00:00:00+00:00";

pub fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .expect("time")
        .with_timezone(&Utc)
}

pub struct World {
    pub conn: Connection,
    pub catalog: Catalog,
    /// 內建子類別表（畫面編修與匯入使用）
    pub classes: AssetClasses,
    pub settings: MatchSettings,
    /// 最近一次比對的時間（逾期以此計算）
    pub last_run: String,
}

pub fn world() -> World {
    world_with(None)
}

/// 內建種子 + 額外目錄（模擬 catalog.d）。
pub fn world_with(extra: Option<&str>) -> World {
    let conn = super::memory_db();
    let catalog = super::ingest::catalog_with(extra);
    sync_catalog(&conn, &catalog).expect("sync catalog");
    World {
        conn,
        catalog,
        classes: AssetClasses::builtin(),
        settings: MatchSettings::default(),
        last_run: T0.to_string(),
    }
}

impl World {
    pub fn run_at(&mut self, now: &str) -> MatchReport {
        self.last_run = now.to_string();
        run_match(&mut self.conn, &self.catalog, &self.settings, at(now)).expect("match")
    }

    /// 以最近一次比對時間與目前設定列出等待區。
    pub fn pending_list(
        &self,
        only_stale: bool,
    ) -> Vec<cvewatch_core::matching::pending_ops::PendingRow> {
        cvewatch_core::matching::pending_ops::list(
            &self.conn,
            only_stale,
            self.settings.stale_days,
            at(&self.last_run),
        )
        .expect("pending list")
    }

    pub fn run(&mut self) -> MatchReport {
        self.run_at(T0)
    }

    /// 新增 active 資產與產品元件，回傳各元件 id。
    pub fn asset(&self, tag: &str, exposure: &str, components: &[(&str, &str)]) -> Vec<i64> {
        self.conn
            .execute(
                "INSERT INTO asset (asset_tag, name, category, exposure, first_seen, last_seen)
                 VALUES (?1, ?1, 'HW01', ?2, ?3, ?3)",
                params![tag, exposure, T0],
            )
            .expect("asset");
        let asset_id = self.conn.last_insert_rowid();
        components
            .iter()
            .map(|(key, version)| {
                self.conn
                    .execute(
                        "INSERT INTO component (asset_id, kind, product_id, version_raw, source, updated_at)
                         VALUES (?1, 'product', (SELECT id FROM catalog_product WHERE key = ?2), ?3, 'excel', ?4)",
                        params![asset_id, key, version, T0],
                    )
                    .expect("component");
                self.conn.last_insert_rowid()
            })
            .collect()
    }

    /// 設定資產價值（C/I/A 三項同值，MAX(C, I, A) 即為該值）。
    pub fn set_asset_value(&self, tag: &str, value: u8) {
        self.conn
            .execute(
                "UPDATE asset SET confidentiality = ?2, integrity = ?2, availability = ?2
                 WHERE asset_tag = ?1",
                params![tag, value],
            )
            .expect("asset value");
    }

    pub fn package(&self, tag: &str, purl: &str, version: &str, updated_at: &str) -> i64 {
        self.conn
            .execute(
                "INSERT INTO component (asset_id, kind, purl, purl_full, version_raw, source, updated_at)
                 VALUES ((SELECT id FROM asset WHERE asset_tag = ?1), 'package', ?2, ?2 || '@' || ?3, ?3, 'dpkg', ?4)",
                params![tag, purl, version, updated_at],
            )
            .expect("package");
        self.conn.last_insert_rowid()
    }

    pub fn set_version(&self, component_id: i64, version: &str, updated_at: &str) {
        self.conn
            .execute(
                "UPDATE component SET version_raw = ?2, updated_at = ?3 WHERE id = ?1",
                params![component_id, version, updated_at],
            )
            .expect("version");
    }

    pub fn retire(&self, tag: &str) {
        self.conn
            .execute(
                "UPDATE asset SET status = 'retired', retired_at = ?2 WHERE asset_tag = ?1",
                params![tag, T0],
            )
            .expect("retire");
    }

    /// 以 CVE 5.x 紀錄寫入（`cna` / `adp` 為 affected 陣列）。
    pub fn cve(&self, id: &str, cna: Value, adp: Value) {
        self.cve_full(
            id,
            "PUBLISHED",
            "2026-10-01T00:00:00Z",
            None,
            cna,
            adp,
            Relevance::Cna,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn cve_full(
        &self,
        id: &str,
        state: &str,
        updated: &str,
        description: Option<&str>,
        cna: Value,
        adp: Value,
        reason: Relevance,
    ) {
        let adp_containers: Vec<Value> = adp
            .as_array()
            .map(|a| {
                if a.is_empty() {
                    Vec::new()
                } else {
                    vec![json!({"providerMetadata": {"shortName": "CISA-ADP"}, "affected": a})]
                }
            })
            .unwrap_or_default();
        let record = json!({
            "dataType": "CVE_RECORD",
            "dataVersion": "5.1",
            "cveMetadata": {"cveId": id, "state": state, "assignerShortName": "test",
                             "datePublished": "2026-09-01T00:00:00Z", "dateUpdated": updated},
            "containers": {
                "cna": {"providerMetadata": {"shortName": "test"},
                        "descriptions": [{"lang": "en", "value": description.unwrap_or("test vulnerability")}],
                        "affected": cna},
                "adp": adp_containers
            }
        });
        let bytes = serde_json::to_vec(&record).expect("json");
        let parsed = cve5::parse(&bytes).expect("parse");
        upsert_record(&self.conn, &parsed, &bytes, reason, T0).expect("upsert");
    }

    /// NVD cpeMatch 列：(criteria, vulnerable, start_incl, end_excl)
    pub fn nvd(&self, cve: &str, rows: &[(&str, bool, Option<&str>, Option<&str>)]) {
        self.nvd_config(cve, None, rows);
    }

    pub fn nvd_config(
        &self,
        cve: &str,
        operator: Option<&str>,
        rows: &[(&str, bool, Option<&str>, Option<&str>)],
    ) {
        ensure_stub(&self.conn, cve, Relevance::Nvd, None, T0).expect("stub");
        for (i, (criteria, vuln, start, end)) in rows.iter().enumerate() {
            self.conn
                .execute(
                    "INSERT INTO cve_nvd_match (cve_id, config_group, config_operator, node_group, node_operator,
                                                node_negate, criteria_cpe, vuln, ver_start_incl, ver_end_excl)
                     VALUES (?1, 0, ?2, ?3, 'OR', 0, ?4, ?5, ?6, ?7)",
                    params![cve, operator, i as i64, criteria, vuln, start, end],
                )
                .expect("nvd");
        }
        self.conn
            .execute(
                "UPDATE cve SET nvd_vuln_status = 'Analyzed' WHERE id = ?1",
                params![cve],
            )
            .expect("status");
    }

    /// 原廠 CSAF 狀態：(version_expr, kind, status)
    pub fn csaf(
        &self,
        cve: &str,
        product: &str,
        tracking: &str,
        updated: &str,
        rows: &[(&str, &str, &str)],
    ) {
        ensure_stub(&self.conn, cve, Relevance::Csaf, None, T0).expect("stub");
        self.conn
            .execute(
                "INSERT OR REPLACE INTO csaf_document (provider, tracking_id, version, updated, url, raw, fetched_at)
                 VALUES ('paloalto', ?1, '1', ?2, 'https://example/csaf', x'00', ?3)",
                params![tracking, updated, T0],
            )
            .expect("doc");
        for (expr, kind, status) in rows {
            self.conn
                .execute(
                    "INSERT INTO csaf_status (provider, tracking_id, cve_id, catalog_product_id, csaf_product_id,
                                              version_expr, kind, status)
                     VALUES ('paloalto', ?1, ?2, (SELECT id FROM catalog_product WHERE key = ?3), 'P', ?4, ?5, ?6)",
                    params![tracking, cve, product, expr, kind, status],
                )
                .expect("status");
        }
    }

    pub fn csaf_fix(&self, cve: &str, product: &str, tracking: &str, fixed: &str) {
        self.conn
            .execute(
                "INSERT INTO csaf_fix (provider, tracking_id, cve_id, catalog_product_id, fixed_version)
                 VALUES ('paloalto', ?1, ?2, (SELECT id FROM catalog_product WHERE key = ?3), ?4)",
                params![tracking, cve, product, fixed],
            )
            .expect("fix");
    }

    pub fn kev(&self, cve: &str, date: &str) {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO kev (cve_id, date_added) VALUES (?1, ?2)",
                params![cve, date],
            )
            .expect("kev");
    }

    pub fn epss(&self, cve: &str, score: f64) {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO epss (cve_id, score, percentile, score_date, fetched_on)
                 VALUES (?1, ?2, 0.5, '2026-11-01', '2026-11-01')",
                params![cve, score],
            )
            .expect("epss");
    }

    pub fn cvss(&self, cve: &str, score: f64) {
        self.conn
            .execute(
                "INSERT INTO cve_metric (cve_id, source, version, base_score) VALUES (?1, 'nvd', '3.1', ?2)",
                params![cve, score],
            )
            .expect("cvss");
    }

    pub fn osv_hit(&self, component: i64, osv_id: &str, cves: &[&str], queried_at: &str) {
        self.conn
            .execute(
                "INSERT INTO osv_hit (component_id, osv_id, cve_ids_json, fixed_versions_json, queried_at)
                 VALUES (?1, ?2, ?3, '[\"3.0.13-1\"]', ?4)",
                params![component, osv_id, serde_json::to_string(cves).unwrap(), queried_at],
            )
            .expect("osv");
    }

    pub fn osv_synced(&self, started_at: &str) {
        self.conn
            .execute(
                "INSERT INTO sync_run (started_at, finished_at, source, status) VALUES (?1, ?1, 'osv', 'success')",
                params![started_at],
            )
            .expect("run");
    }

    pub fn finding(&self, component: i64, cve: &str) -> Option<Finding> {
        self.conn
            .query_row(
                "SELECT id, confidence, decided_by, priority, status, ignore_reason, justification,
                        in_range, (status = 'resolved' AND in_range), manual
                 FROM finding WHERE component_id = ?1 AND cve_id = ?2",
                params![component, cve],
                |r| {
                    Ok(Finding {
                        id: r.get(0)?,
                        confidence: r.get(1)?,
                        decided_by: r.get(2)?,
                        priority: r.get(3)?,
                        status: r.get(4)?,
                        ignore_reason: r.get(5)?,
                        justification: r.get(6)?,
                        in_range: r.get(7)?,
                        still_vulnerable: r.get(8)?,
                        manual: r.get(9)?,
                    })
                },
            )
            .optional()
            .expect("finding")
    }

    pub fn events(&self, finding_id: i64) -> Vec<(String, Option<String>, String, String)> {
        let mut stmt = self
            .conn
            .prepare("SELECT actor, from_status, to_status, note FROM finding_event WHERE finding_id = ?1 ORDER BY id")
            .unwrap();
        stmt.query_map(params![finding_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    pub fn evidence(&self, finding_id: i64) -> Vec<(String, String, String)> {
        let mut stmt = self
            .conn
            .prepare("SELECT source, verdict, detail_json FROM finding_evidence WHERE finding_id = ?1 ORDER BY rowid")
            .unwrap();
        stmt.query_map(params![finding_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    pub fn conflicts(&self) -> Vec<(i64, String, String, String)> {
        let mut stmt = self
            .conn
            .prepare("SELECT component_id, cve_id, final_source, other_source FROM match_conflict ORDER BY cve_id")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// (reason, stale, resolved_reason)
    pub fn pending(&self, cve: &str) -> Option<(String, bool, Option<String>)> {
        let cutoff = cvewatch_core::matching::pending::stale_cutoff(
            at(&self.last_run),
            self.settings.stale_days,
        );
        self.conn
            .query_row(
                "SELECT reason, resolved_at IS NULL AND entered_at < ?2, resolved_reason
                 FROM pending WHERE cve_id = ?1",
                params![cve, cutoff],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .expect("pending")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: i64,
    pub confidence: String,
    pub decided_by: String,
    pub priority: String,
    pub status: String,
    pub ignore_reason: Option<String>,
    pub justification: Option<String>,
    pub in_range: bool,
    pub still_vulnerable: bool,
    pub manual: bool,
}

/// CVE 5.x affected 項目。
pub fn affected(
    vendor: &str,
    product: &str,
    default_status: Option<&str>,
    versions: Value,
) -> Value {
    let mut v = json!({"vendor": vendor, "product": product, "versions": versions});
    if let Some(d) = default_status {
        v["defaultStatus"] = json!(d);
    }
    v
}

pub fn panos(versions: Value) -> Value {
    json!([affected(
        "Palo Alto Networks",
        "PAN-OS",
        Some("unaffected"),
        versions
    )])
}
