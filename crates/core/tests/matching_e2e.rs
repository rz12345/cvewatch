//! 04 任務 6.2：端對端 — fixture 清冊（PA-460、R6515 + ESXi/iDRAC、Proxmox VE、Synology DSM）
//! 經實際匯入、CVE 紀錄 / NVD / 原廠 CSAF 寫入流程後比對，結果與 golden file 比較。
//!
//! 更新 golden file：`UPDATE_GOLDEN=1 cargo test -p cvewatch-core --test matching_e2e`，並人工檢查差異。

mod common;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use common::ingest::harness;
use common::matching::at;
use common::{memory_db, read_ingest};
use cvewatch_core::assets::columns::ColumnMap;
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::sheet::read_inventory;
use cvewatch_core::assets::validate::parse_inventory;
use cvewatch_core::assets::AssetClasses;
use cvewatch_core::catalog::store::sync_catalog;
use cvewatch_core::catalog::Catalog;
use cvewatch_core::ingest::csaf::fetch::Document;
use cvewatch_core::ingest::csaf::{status, store as csaf_store, tree};
use cvewatch_core::ingest::relevance::{package_keys, RelevanceIndex};
use cvewatch_core::ingest::source::SyncOptions;
use cvewatch_core::ingest::{cve5, nvd, store};
use cvewatch_core::matching::pending_ops;
use cvewatch_core::matching::query::{self, FindingFilter};
use cvewatch_core::matching::run::{run_match, MatchSettings};
use rusqlite::{params, Connection};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const NOW: &str = "2026-11-07T00:00:00+00:00";
const AFTER_UPGRADE: &str = "2026-11-08T00:00:00+00:00";
const GOLDEN: &str = "expected.txt";

/// cvelistV5 錄製紀錄（含與目錄無關的 WordPress 外掛，應被關聯性篩選排除）。
const INGEST_RECORDS: [&str; 6] = [
    "CVE-2024-3400",
    "CVE-2025-0108",
    "CVE-2099-50001",
    "CVE-2099-50002",
    "CVE-2026-103479",
    "CVE-2025-7735",
];
const MATCHING_RECORDS: [&str; 3] = ["CVE-2099-60001", "CVE-2099-60004", "CVE-2099-60005"];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/matching")
        .join(name)
}

fn import(conn: &mut Connection, catalog: &Catalog, inventory: &str, now: DateTime<Utc>) {
    import_with(conn, catalog, inventory, Some("components.csv"), now);
}

fn import_with(
    conn: &mut Connection,
    catalog: &Catalog,
    inventory: &str,
    components: Option<&str>,
    now: DateTime<Utc>,
) {
    let components = components.map(fixture);
    let book = read_inventory(&fixture(inventory), components.as_deref()).expect("讀取清冊");
    let parsed =
        parse_inventory(&book, &ColumnMap::default(), &AssetClasses::builtin()).expect("驗證清冊");
    let plan =
        import_inventory(conn, catalog, &parsed, ImportOptions::default(), now).expect("匯入");
    assert!(
        plan.unresolved.is_empty(),
        "所有產品都應辨識：{:?}",
        plan.unresolved
    );
}

/// 依實際同步流程：關聯性篩選通過者才寫入。
fn load_records(conn: &Connection, catalog: &Catalog) {
    let index = RelevanceIndex::new(catalog, package_keys(conn).unwrap());
    let ingest = INGEST_RECORDS.map(|id| read_ingest(&format!("cvelist/records/{id}.json")));
    let synthetic =
        MATCHING_RECORDS.map(|id| std::fs::read(fixture(&format!("cves/{id}.json"))).unwrap());
    for bytes in ingest.iter().chain(synthetic.iter()) {
        let record = cve5::parse(bytes).expect("CVE 紀錄");
        if let Some(reason) = index.evaluate(&record) {
            store::upsert_record(conn, &record, bytes, reason, NOW).expect("寫入紀錄");
        }
    }
}

/// 經實際的 NVD 同步流程寫入（wiremock 對每個目錄 CPE 的查詢都回傳 fixture 頁面）。
async fn load_nvd(conn: &mut Connection) {
    let h = harness(conn, None, Vec::new()).await;
    let page: Value = serde_json::from_slice(&std::fs::read(fixture("nvd.json")).unwrap()).unwrap();
    Mock::given(method("GET"))
        .and(path("/nvd"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page))
        .mount(&h.server)
        .await;
    let options = SyncOptions::default();
    nvd::sync(&h.ctx(&options, at(NOW)), conn)
        .await
        .expect("NVD 同步");
}

/// 實際 Palo Alto CSAF 公告（CVE-2026-0322）經 product_tree 對應、狀態擷取後寫入。
fn load_csaf(conn: &Connection, catalog: &Catalog) {
    let raw = read_ingest("csaf/paloalto/CVE-2026-0322.json");
    let json: Value = serde_json::from_slice(&raw).unwrap();
    let (products, _) = tree::map_products(&json["product_tree"], catalog);
    let vulns = json["vulnerabilities"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let extracted = status::extract(&vulns, &products);
    let tracking = &json["document"]["tracking"];
    let doc = Document {
        url: "https://security.paloaltonetworks.com/csaf/CVE-2026-0322".into(),
        raw: raw.clone(),
        json: json.clone(),
        tracking_id: tracking["id"].as_str().unwrap().into(),
        version: tracking["version"].as_str().unwrap_or("1").into(),
        title: json["document"]["title"].as_str().map(str::to_string),
        tlp: None,
        published: tracking["initial_release_date"]
            .as_str()
            .map(str::to_string),
        updated: tracking["current_release_date"]
            .as_str()
            .map(str::to_string),
        sha_ok: None,
    };
    let ids = sync_catalog(conn, catalog).unwrap();
    csaf_store::replace_document(conn, "paloalto", &doc, &extracted, &ids, NOW).expect("CSAF 寫入");
}

fn load_scores(conn: &Connection) {
    conn.execute(
        "INSERT INTO kev (cve_id, date_added) VALUES ('CVE-2099-50001', '2026-11-05')",
        [],
    )
    .unwrap();
    for (cve, score) in [
        ("CVE-2099-60003", 0.35),
        ("CVE-2026-0322", 0.02),
        ("CVE-2099-60001", 0.004),
    ] {
        conn.execute(
            "INSERT INTO epss (cve_id, score, percentile, score_date, fetched_on)
             VALUES (?1, ?2, 0.5, '2026-11-06', '2026-11-07')",
            params![cve, score],
        )
        .unwrap();
    }
}

fn render(conn: &Connection, title: &str, out: &mut String) {
    writeln!(out, "## {title}").unwrap();
    writeln!(out, "### 命中（風險排序）").unwrap();
    for row in query::list(conn, &FindingFilter::default()).unwrap() {
        let detail = query::show(conn, row.id).unwrap().unwrap();
        let basis = detail
            .evidence
            .iter()
            .find(|e| e.source.as_str() == row.decided_by)
            .map_or("（人工）".to_string(), |e| e.summary.clone());
        writeln!(
            out,
            "{} | {} {} | {} | {} {} | {} | {}{}",
            row.asset_tag,
            row.component,
            row.version,
            row.cve_id,
            row.priority.as_str(),
            row.confidence.as_str(),
            row.decided_by,
            row.status.as_str(),
            if row.still_vulnerable {
                " | 仍受影響"
            } else {
                ""
            }
        )
        .unwrap();
        writeln!(out, "    依據：{basis}").unwrap();
        for e in detail.evidence.iter().flat_map(|e| e.notes.iter()) {
            writeln!(out, "    註記：{e}").unwrap();
        }
        if let Some(event) = detail.events.last().filter(|e| e.from.is_some()) {
            writeln!(
                out,
                "    最近事件：{} → {}：{}",
                event.from.unwrap().as_str(),
                event.to.as_str(),
                event.note
            )
            .unwrap();
        }
    }
    writeln!(out, "### 來源矛盾").unwrap();
    for c in query::conflicts(conn).unwrap() {
        writeln!(
            out,
            "{} {} {} | {} | 最終 {}={} | 相反 {}={}",
            c.asset_tag,
            c.component,
            c.version,
            c.cve_id,
            c.final_source,
            c.final_verdict,
            c.other_source,
            c.other_verdict
        )
        .unwrap();
    }
    writeln!(out, "### 等待區").unwrap();
    for p in pending_ops::list(conn, false, 60, at(NOW)).unwrap() {
        writeln!(
            out,
            "{} | {} | KEV {}",
            p.cve_id,
            p.reason.as_str(),
            p.kev_date.as_deref().unwrap_or("-")
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn compare_golden(actual: &str) {
    let path = fixture(GOLDEN);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("缺少 {}；以 UPDATE_GOLDEN=1 產生", path.display()))
        .replace("\r\n", "\n");
    if expected != actual {
        let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("matching_e2e.actual.txt");
        std::fs::write(&tmp, actual).unwrap();
        panic!(
            "比對結果與 golden file 不同；實際輸出：{}\n\n{actual}",
            tmp.display()
        );
    }
}

#[tokio::test]
async fn fixture_assets_and_cves_match_golden_file() {
    let mut conn = memory_db();
    let catalog = Catalog::seed().unwrap();
    let settings = MatchSettings::default();
    import(&mut conn, &catalog, "inventory.csv", at(NOW));
    load_records(&conn, &catalog);
    load_nvd(&mut conn).await;
    load_csaf(&conn, &catalog);
    load_scores(&conn);
    // 與目錄無關的紀錄不會進入本機
    let unrelated: i64 = conn
        .query_row(
            "SELECT count(*) FROM cve WHERE id = 'CVE-2026-103479'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unrelated, 0);

    let mut out = String::new();
    run_match(&mut conn, &catalog, &settings, at(NOW)).unwrap();
    render(&conn, "初次比對", &mut out);

    // 重匯清冊：NAS-01 升版到已修正版本，比對後自動轉為已處理
    import(
        &mut conn,
        &catalog,
        "inventory_upgraded.csv",
        at(AFTER_UPGRADE),
    );
    run_match(&mut conn, &catalog, &settings, at(AFTER_UPGRADE)).unwrap();
    render(&conn, "NAS-01 升版後重新比對", &mut out);

    compare_golden(&out);
}

/// code review #2：清冊重匯時元件消失 → 軟刪除（命中保留、不再更新）；元件回到清冊時沿用原命中。
#[tokio::test]
async fn dropped_spreadsheet_component_keeps_finding() {
    let mut conn = memory_db();
    let catalog = Catalog::seed().unwrap();
    let settings = MatchSettings::default();
    import(&mut conn, &catalog, "inventory.csv", at(NOW));
    load_nvd(&mut conn).await;
    run_match(&mut conn, &catalog, &settings, at(NOW)).unwrap();
    let idrac = |conn: &Connection| -> (i64, i64, String, bool) {
        conn.query_row(
            "SELECT f.id, c.id, f.status, c.removed_at IS NOT NULL
             FROM finding f JOIN component c ON c.id = f.component_id
             WHERE f.cve_id = 'CVE-2099-60002'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
    };
    let (finding, component, status, removed) = idrac(&conn);
    assert_eq!((status.as_str(), removed), ("open", false));

    import_with(
        &mut conn,
        &catalog,
        "inventory.csv",
        None,
        at(AFTER_UPGRADE),
    );
    run_match(&mut conn, &catalog, &settings, at(AFTER_UPGRADE)).unwrap();
    let (f2, c2, status, removed) = idrac(&conn);
    assert_eq!(
        (f2, c2, status.as_str(), removed),
        (finding, component, "open", true)
    );

    import(&mut conn, &catalog, "inventory.csv", at(AFTER_UPGRADE));
    run_match(&mut conn, &catalog, &settings, at(AFTER_UPGRADE)).unwrap();
    let (f3, c3, status, removed) = idrac(&conn);
    assert_eq!(
        (f3, c3, status.as_str(), removed),
        (finding, component, "open", false)
    );
}

/// code review 第十四輪 #5：影響比對的清冊匯入在同一交易記錄「比對待補」，全量比對成功時清除；
/// 沒有影響比對的重匯不記錄。
#[test]
fn inventory_import_marks_match_stale_in_its_transaction() {
    use cvewatch_core::matching::run::match_is_stale;

    let mut conn = memory_db();
    let catalog = Catalog::seed().unwrap();
    import(&mut conn, &catalog, "inventory.csv", at(NOW));
    assert!(match_is_stale(&conn).unwrap(), "新增資產 → 比對待補");
    run_match(&mut conn, &catalog, &MatchSettings::default(), at(NOW)).unwrap();
    assert!(!match_is_stale(&conn).unwrap());
    import(&mut conn, &catalog, "inventory.csv", at(AFTER_UPGRADE));
    assert!(!match_is_stale(&conn).unwrap(), "沒有變更的重匯不記錄");
    import(
        &mut conn,
        &catalog,
        "inventory_upgraded.csv",
        at(AFTER_UPGRADE),
    );
    assert!(match_is_stale(&conn).unwrap(), "版本變更 → 比對待補");
}
