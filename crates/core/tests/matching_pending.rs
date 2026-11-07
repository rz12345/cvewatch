//! 04 任務 3.5：等待區進入、資料補齊後移出（受影響 / 不受影響）、逾期與產品暫時不在目錄時的延後。

mod common;

use common::matching::{affected, at, world, world_with, World, T0};
use cvewatch_core::ingest::relevance::Relevance;
use cvewatch_core::matching::pending::ResolvedReason;
use serde_json::json;

const CVE: &str = "CVE-2099-20001";
const PVE_CPE: &str = "cpe:2.3:a:proxmox:virtual_environment:*:*:*:*:*:*:*:*";
const THREE_DAYS_LATER: &str = "2026-11-04T00:00:00+00:00";

/// 種子目錄沒有 Proxmox 的 CNA 寫法，以 catalog.d 補上（使用者實際的做法）。
const PROXMOX_CNA: &str = r#"
[[product]]
key = "proxmox-ve"
cna = [{ vendor = "Proxmox Server Solutions GmbH", product = "Proxmox VE" }]
"#;

fn proxmox_world() -> (World, i64) {
    let w = world_with(Some(PROXMOX_CNA));
    let c = w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.2")])[0];
    w.cve(
        CVE,
        json!([affected(
            "Proxmox Server Solutions GmbH",
            "Proxmox VE",
            None,
            json!([])
        )]),
        json!([]),
    );
    (w, c)
}

// ── Requirement: Entering the pending queue ─────────────────

#[test]
fn newly_published_cve_without_details_enters_pending() {
    let (mut w, c) = proxmox_world();
    let report = w.run();
    assert_eq!(
        w.pending(CVE),
        Some(("no_version_data".into(), false, None))
    );
    assert_eq!(report.pending.entered, 1);
    // 產品相符但無版本 → 同時建立 probable 命中
    assert_eq!(w.finding(c, CVE).unwrap().confidence, "probable");
}

#[test]
fn vendor_investigating_enters_pending() {
    let mut w = world();
    let c = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4")])[0];
    w.csaf(
        CVE,
        "paloalto-panos",
        "PAN-SA-9",
        "2026-10-01T00:00:00Z",
        &[("11.1.4", "exact", "under_investigation")],
    );
    w.run();
    assert_eq!(w.pending(CVE).unwrap().0, "vendor_investigating");
    assert_eq!(w.finding(c, CVE).unwrap().confidence, "probable");
}

#[test]
fn keyword_only_cve_enters_pending_when_product_is_in_use() {
    let mut w = world();
    w.cve_full(
        CVE,
        "PUBLISHED",
        "2026-10-01T00:00:00Z",
        Some("A flaw in Synology DiskStation Manager allows ..."),
        json!([affected(
            "n/a",
            "n/a",
            None,
            json!([{"version": "n/a", "status": "affected"}])
        )]),
        json!([]),
        Relevance::KeywordOnly,
    );
    w.run();
    assert!(w.pending(CVE).is_none(), "沒有使用 DSM 的資產時不進等待區");
    w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.2-72806")]);
    w.run();
    assert_eq!(w.pending(CVE).unwrap().0, "keyword_only");
}

// ── Requirement: Re-evaluation after each sync ──────────────

#[test]
fn details_published_later_create_confirmed_finding() {
    let (mut w, c) = proxmox_world();
    w.run();
    w.nvd(CVE, &[(PVE_CPE, true, None, Some("8.2.4"))]);
    let report = w.run_at(THREE_DAYS_LATER);
    assert_eq!(report.pending.resolved, 1);
    let (_, _, why) = w.pending(CVE).unwrap();
    assert_eq!(why.as_deref(), Some(ResolvedReason::DataAffected.as_str()));
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (
            f.confidence.as_str(),
            f.decided_by.as_str(),
            f.status.as_str()
        ),
        ("confirmed", "nvd", "open")
    );
}

#[test]
fn details_showing_not_affected_resolve_without_confirmed_finding() {
    let (mut w, c) = proxmox_world();
    w.run();
    w.nvd(CVE, &[(PVE_CPE, true, None, Some("8.2.0"))]);
    w.run_at(THREE_DAYS_LATER);
    let (_, _, why) = w.pending(CVE).unwrap();
    let why = ResolvedReason::parse(why.as_deref().unwrap()).unwrap();
    assert_eq!(why.label(), "資料補齊，不受影響");
    // 先前的 probable 命中不轉為 confirmed，且已不在受影響範圍
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(f.confidence, "probable");
    assert!(!f.in_range);
    assert_eq!(f.status, "resolved");
}

// ── Requirement: Stale pending items ────────────────────────

#[test]
fn pending_for_61_days_is_stale() {
    let (mut w, _) = proxmox_world();
    w.run();
    w.run_at("2026-12-30T00:00:00+00:00"); // 59 天
    assert!(!w.pending(CVE).unwrap().1);
    assert!(w.pending_list(true).is_empty());
    let report = w.run_at("2027-01-01T00:00:00+00:00"); // 61 天
    assert!(w.pending(CVE).unwrap().1);
    assert_eq!(report.pending.stale, 1);
    let stale = w.pending_list(true);
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].cve_id, CVE);
}

#[test]
fn stale_days_is_configurable() {
    let (mut w, _) = proxmox_world();
    w.settings.stale_days = 3;
    w.run();
    w.run_at("2026-11-05T00:00:00+00:00");
    assert!(w.pending(CVE).unwrap().1);
}

#[test]
fn kev_listed_pending_cve_is_pinned_to_top() {
    let (mut w, _) = proxmox_world();
    let second = "CVE-2099-20002";
    w.cve(
        second,
        json!([affected(
            "Proxmox Server Solutions GmbH",
            "Proxmox VE",
            None,
            json!([])
        )]),
        json!([]),
    );
    w.run();
    let order: Vec<String> = w
        .pending_list(false)
        .into_iter()
        .map(|r| r.cve_id)
        .collect();
    assert_eq!(order, vec![CVE.to_string(), second.to_string()]);
    w.kev(second, "2026-11-03");
    w.run_at(THREE_DAYS_LATER);
    let rows = w.pending_list(false);
    assert_eq!(rows[0].cve_id, second);
    assert_eq!(rows[0].kev_date.as_deref(), Some("2026-11-03"));
}

#[test]
fn rejected_cve_leaves_pending() {
    let (mut w, _) = proxmox_world();
    w.run();
    w.cve_full(
        CVE,
        "REJECTED",
        "2026-11-03T00:00:00Z",
        None,
        json!([]),
        json!([]),
        Relevance::Cna,
    );
    w.run_at(THREE_DAYS_LATER);
    assert_eq!(
        w.pending(CVE).unwrap().2.as_deref(),
        Some("no_longer_relevant")
    );
}

/// code review 第三輪 #1：`defaultStatus: unknown` 不算有版本資料（Vulnogram 常見寫法）。
#[test]
fn default_status_unknown_still_counts_as_no_version_data() {
    let mut w = world_with(Some(PROXMOX_CNA));
    w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.2")]);
    w.cve(
        CVE,
        json!([affected(
            "Proxmox Server Solutions GmbH",
            "Proxmox VE",
            Some("unknown"),
            json!([{"version": "n/a", "status": "affected"}])
        )]),
        json!([]),
    );
    w.run();
    assert_eq!(w.pending(CVE).unwrap().0, "no_version_data");
}

/// code review 第十六輪 #3：「標示受影響但版本為佔位字」時判定不採用 defaultStatus，
/// 因此明確的 `defaultStatus: unaffected` 也不算有版本資料。
#[test]
fn placeholder_affected_with_explicit_default_counts_as_no_version_data() {
    let mut w = world_with(Some(PROXMOX_CNA));
    w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.2")]);
    w.cve(
        CVE,
        json!([affected(
            "Proxmox Server Solutions GmbH",
            "Proxmox VE",
            Some("unaffected"),
            json!([{"version": "n/a", "status": "affected"}])
        )]),
        json!([]),
    );
    w.run();
    assert_eq!(w.pending(CVE).unwrap().0, "no_version_data");
}

/// code review 第三輪 #5：有版本資料但仍無法判定 → 以正確原因移出（保留 probable 命中）。
#[test]
fn inconclusive_data_leaves_pending_with_its_own_reason() {
    let (mut w, c) = proxmox_world();
    w.run();
    assert!(w.pending(CVE).unwrap().2.is_none());
    // 較新的 dateUpdated 才會覆寫本機紀錄
    w.cve_full(
        CVE,
        "PUBLISHED",
        "2026-11-03T00:00:00Z",
        None,
        json!([affected(
            "Proxmox Server Solutions GmbH",
            "Proxmox VE",
            None,
            json!([{"version": "7.0", "lessThan": "7.4", "status": "affected"}])
        )]),
        json!([]),
        Relevance::Cna,
    );
    w.run_at(THREE_DAYS_LATER);
    let (_, _, why) = w.pending(CVE).unwrap();
    let why = ResolvedReason::parse(why.as_deref().unwrap()).unwrap();
    assert_eq!(why, ResolvedReason::DataInconclusive);
    assert_eq!(w.finding(c, CVE).unwrap().confidence, "probable");
}

/// code review 第四輪 #2：產品暫時不在目錄中時，等待區的 CVE 維持原狀（不標為已與資產無關）。
#[test]
fn pending_kept_while_product_missing_from_catalog() {
    use cvewatch_core::catalog::{Catalog, CatalogSource};

    let (mut w, _) = proxmox_world();
    w.run();
    let full = w.catalog.clone();
    w.catalog = Catalog::from_sources(&[CatalogSource {
        name: "only-panos.toml".into(),
        text: "[[product]]\nkey = \"paloalto-panos\"\ndisplay_name = \"PAN-OS\"\n\
               vendor_display = \"Palo Alto Networks\"\nversion_scheme = \"panos\"\n"
            .into(),
    }])
    .unwrap();
    let report = w.run_at(THREE_DAYS_LATER);
    assert_eq!(report.skipped_components, 1);
    assert_eq!(
        w.pending(CVE),
        Some(("no_version_data".into(), false, None))
    );
    w.catalog = full;
    w.run_at("2027-01-01T00:00:00+00:00");
    assert!(w.pending(CVE).unwrap().1, "進入時間保留，逾期照常判斷");
}

/// code review 第十六輪 #2：等待區記錄相關產品；產品暫時不在目錄中時，僅關鍵字相符（沒有命中）的
/// CVE 也維持原狀，與該產品無關的 CVE 照常移出。
#[test]
fn keyword_only_pending_kept_while_its_product_missing_from_catalog() {
    use cvewatch_core::catalog::{Catalog, CatalogSource};

    const OTHER: &str = "CVE-2026-9999";
    let mut w = world();
    w.asset("NAS-01", "internal", &[("synology-dsm", "7.2.2-72806")]);
    w.asset("FW-01", "dmz", &[("paloalto-panos", "11.1.4")]);
    let keyword_cve = |w: &mut World, id: &str, text: &str| {
        w.cve_full(
            id,
            "PUBLISHED",
            "2026-10-01T00:00:00Z",
            Some(text),
            json!([affected(
                "n/a",
                "n/a",
                None,
                json!([{"version": "n/a", "status": "affected"}])
            )]),
            json!([]),
            Relevance::KeywordOnly,
        );
    };
    keyword_cve(
        &mut w,
        CVE,
        "A flaw in Synology DiskStation Manager allows ...",
    );
    keyword_cve(&mut w, OTHER, "A flaw in PAN-OS allows ...");
    w.run();
    assert_eq!(w.pending(CVE).unwrap().0, "keyword_only");
    assert_eq!(w.pending(OTHER).unwrap().0, "keyword_only");
    let full = w.catalog.clone();
    // 目錄暫時只剩 PAN-OS，且 PAN-OS 不再有關鍵字 → DSM 的 CVE 延後，PAN-OS 的 CVE 移出
    w.catalog = Catalog::from_sources(&[CatalogSource {
        name: "only-panos.toml".into(),
        text: "[[product]]
key = \"paloalto-panos\"
display_name = \"PAN-OS\"
               vendor_display = \"Palo Alto Networks\"
version_scheme = \"panos\"
"
        .into(),
    }])
    .unwrap();
    w.run_at(THREE_DAYS_LATER);
    assert_eq!(
        w.pending(CVE),
        Some(("keyword_only".into(), false, None)),
        "DSM 暫時不在目錄中，維持原狀"
    );
    assert_eq!(
        w.pending(OTHER).unwrap().2.as_deref(),
        Some("no_longer_relevant")
    );
    // 期間再跑一次仍保留（延後的依據沒有遺失）
    w.run_at(THREE_DAYS_LATER);
    assert!(w.pending(CVE).unwrap().2.is_none());
    w.catalog = full;
    w.run_at(THREE_DAYS_LATER);
    assert_eq!(w.pending(CVE).unwrap().0, "keyword_only");
}

/// code review 第六輪 #5：NVD 只有 `-`（沒有版本）且沒有區間時，視為缺少版本資料。
#[test]
fn nvd_without_version_counts_as_no_version_data() {
    let mut w = world_with(Some(PROXMOX_CNA));
    w.asset("PVE-01", "internal", &[("proxmox-ve", "8.2.2")]);
    w.nvd(
        CVE,
        &[(
            "cpe:2.3:a:proxmox:virtual_environment:-:*:*:*:*:*:*:*",
            true,
            None,
            None,
        )],
    );
    w.run();
    assert_eq!(w.pending(CVE).unwrap().0, "no_version_data");
}

/// code review 第六輪 #2：等待區依產品判斷：其他產品已有結論，仍缺資料的產品讓 CVE 留在等待區。
#[test]
fn pending_is_assessed_per_product() {
    let (mut w, _) = proxmox_world();
    let fw = w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.4")])[0];
    w.cve_full(
        CVE,
        "PUBLISHED",
        "2026-11-02T00:00:00Z",
        None,
        json!([
            affected(
                "Proxmox Server Solutions GmbH",
                "Proxmox VE",
                None,
                json!([])
            ),
            affected(
                "Palo Alto Networks",
                "PAN-OS",
                Some("unaffected"),
                json!([{"version": "10.2.0", "lessThan": "10.2.9", "status": "affected"}])
            )
        ]),
        json!([]),
        Relevance::Cna,
    );
    w.run();
    assert!(w.finding(fw, CVE).is_none(), "PAN-OS 明確不受影響");
    assert_eq!(
        w.pending(CVE).unwrap(),
        ("no_version_data".to_string(), false, None)
    );
}

/// code review 第八至十四輪：比對待補標記與輸入指紋（目錄 + 評分設定）決定匯入後是否需要補比對。
#[test]
fn rematch_needed_after_unmatched_writes_or_input_change() {
    use cvewatch_core::matching::run::{mark_match_stale, rematch_state};

    let (mut w, _) = proxmox_world();
    let needed = |w: &World| {
        rematch_state(&w.conn, &w.catalog, &w.settings)
            .unwrap()
            .needed()
    };
    assert!(needed(&w), "從未比對過");
    w.run();
    assert!(!needed(&w));
    mark_match_stale(&w.conn, at(T0)).unwrap();
    assert!(needed(&w), "有尚未比對的寫入");
    w.run();
    assert!(!needed(&w), "全量比對成功即清除");
    w.catalog = common::ingest::catalog_with(Some(
        "[[esxi_build]]\nbuild = 24585383\nversion = \"8.0 U3d\"\n",
    ));
    assert!(needed(&w), "目錄已變更");
    w.run();
    assert!(!needed(&w));
    w.settings.scoring.epss_high = 0.05;
    assert!(needed(&w), "評分設定已變更");
}

/// code review 第九輪 #7：逾期於查詢時依當下的 stale_days 計算（不需重新比對）。
#[test]
fn stale_is_computed_at_query_time() {
    let (mut w, _) = proxmox_world();
    w.run();
    w.last_run = "2026-11-10T00:00:00+00:00".into();
    assert!(w.pending_list(true).is_empty());
    w.settings.stale_days = 5;
    assert_eq!(w.pending_list(true).len(), 1, "調低 stale_days 立即生效");
}
