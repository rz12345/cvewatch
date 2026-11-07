//! 04 任務 3.5：等待區人工判定套用（`pending resolve --apply`）。

mod common;

use common::matching::{affected, at, world_with, World, T0};
use cvewatch_core::ingest::relevance::Relevance;
use cvewatch_core::matching::pending_ops::{self, Action};
use serde_json::json;

const CVE: &str = "CVE-2099-20001";
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

// ── Requirement: Manual resolution of pending items ─────────

#[test]
fn user_applies_pending_cve_to_asset() {
    let (mut w, c) = proxmox_world();
    let other = w.asset("PVE-02", "internal", &[("proxmox-ve", "8.1.0")])[0];
    w.conn.execute("DELETE FROM cve_affected", []).unwrap(); // 僅剩關鍵字：沒有自動 probable 命中
    w.conn
        .execute(
            "UPDATE cve SET relevance_reason = 'keyword_only', description_en = 'Proxmox VE flaw' WHERE id = ?1",
            [CVE],
        )
        .unwrap();
    w.run();
    assert_eq!(w.pending(CVE).unwrap().0, "keyword_only");
    assert!(w.finding(c, CVE).is_none());
    let scoring = w.settings.scoring.clone();
    let applied = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        &CVE.to_lowercase(),
        &Action::Apply("PVE-01".into()),
        "已確認使用受影響功能",
        at(T0),
    )
    .unwrap()
    .applied;
    assert_eq!(applied.len(), 1);
    assert!(applied[0].created);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (
            f.confidence.as_str(),
            f.decided_by.as_str(),
            f.status.as_str()
        ),
        ("probable", "manual", "open")
    );
    let events = w.events(f.id);
    assert_eq!(events[0].0, "user");
    assert!(events[0].3.contains("已確認使用受影響功能"));
    assert_eq!(w.pending(CVE).unwrap().2.as_deref(), Some("manual_apply"));
    assert!(w.finding(other, CVE).is_none(), "只套用到指定資產");
    // 重新比對：不放回等待區，人工命中保留
    w.run_at(THREE_DAYS_LATER);
    assert_eq!(w.pending(CVE).unwrap().2.as_deref(), Some("manual_apply"));
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (f.decided_by.as_str(), f.status.as_str()),
        ("manual", "open")
    );
}

#[test]
fn manual_resolution_errors() {
    let (mut w, _) = proxmox_world();
    w.asset(
        "SRV-01",
        "internal",
        &[
            ("dell-idrac9", "7.00.00.00"),
            ("dell-poweredge-bios", "2.16.0"),
        ],
    );
    w.run();
    let scoring = w.settings.scoring.clone();
    let mut resolve = |cve: &str, action: Action| {
        pending_ops::resolve(&mut w.conn, &w.catalog, &scoring, cve, &action, "", at(T0))
            .unwrap_err()
            .to_string()
    };
    assert!(resolve("CVE-2099-99999", Action::Dismiss).contains("不在等待區"));
    assert!(resolve("not-a-cve", Action::Dismiss).contains("不是有效的 CVE ID"));
    assert!(resolve(CVE, Action::Apply("NOPE".into())).contains("找不到使用中的資產"));
    let ambiguous = resolve(CVE, Action::Apply("SRV-01".into()));
    assert!(
        ambiguous.contains("SRV-01:<產品>") && ambiguous.contains("dell-idrac9"),
        "{ambiguous}"
    );
    assert!(resolve(CVE, Action::Apply("SRV-01:vmware-esxi".into())).contains("沒有產品"));
    // 指定元件即可套用
    let applied = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("SRV-01:dell-idrac9".into()),
        "",
        at(T0),
    )
    .unwrap();
    assert_eq!(applied.applied[0].product_key, "dell-idrac9");
}

/// code review #5、#8：套用到既有（已處理）命中時重新開啟並記錄使用者事件；資產編號不分大小寫。
#[test]
fn applying_to_existing_resolved_finding_reopens_it() {
    let (mut w, c) = proxmox_world();
    w.run();
    let id = w.finding(c, CVE).unwrap().id;
    w.conn
        .execute("UPDATE finding SET status = 'resolved' WHERE id = ?1", [id])
        .unwrap();
    let scoring = w.settings.scoring.clone();
    let applied = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("pve-01".into()),
        "確認受影響",
        at(T0),
    )
    .unwrap()
    .applied;
    assert_eq!(applied.len(), 1);
    assert!(!applied[0].created && applied[0].reopened);
    let f = w.finding(c, CVE).unwrap();
    // 人工判定記在 manual 欄位；decided_by 保留來源判定
    assert_eq!(
        (
            f.status.as_str(),
            f.decided_by.as_str(),
            f.in_range,
            f.manual
        ),
        ("open", "cna", true, true)
    );
    let (actor, from, to, note) = w.events(id).pop().unwrap();
    assert_eq!(
        (actor.as_str(), from.as_deref(), to.as_str()),
        ("user", Some("resolved"), "open")
    );
    assert!(note.contains("確認受影響"));
    // 重算後維持未處理（資料仍不足，manual 判定保留）
    w.run_at(THREE_DAYS_LATER);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!((f.status.as_str(), f.manual), ("open", true));
}

/// code review 第三輪 #6：`--apply <資產>:<產品>` 的產品 key 不分大小寫。
#[test]
fn apply_product_key_is_case_insensitive() {
    let (mut w, _) = proxmox_world();
    w.run();
    let scoring = w.settings.scoring.clone();
    let resolution = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("pve-01:PROXMOX-VE".into()),
        "",
        at(T0),
    )
    .unwrap();
    assert_eq!(resolution.applied[0].product_key, "proxmox-ve");
    assert_eq!(resolution.cve_id, CVE);
}

/// code review 第三輪 #7：人工判定以 manual 欄位保存；來源之後確認又撤回時，命中不被系統轉為已處理。
#[test]
fn manual_flag_survives_source_changes() {
    let (mut w, c) = proxmox_world();
    w.conn.execute("DELETE FROM cve_affected", []).unwrap();
    w.conn
        .execute(
            "UPDATE cve SET relevance_reason = 'keyword_only', description_en = 'Proxmox VE flaw' WHERE id = ?1",
            [CVE],
        )
        .unwrap();
    w.run();
    let scoring = w.settings.scoring.clone();
    pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("PVE-01".into()),
        "",
        at(T0),
    )
    .unwrap();
    // CNA 補上區間 → confirmed，decided_by 改為 cna，人工標記保留
    w.cve_full(
        CVE,
        "PUBLISHED",
        "2026-11-03T00:00:00Z",
        None,
        json!([affected(
            "Proxmox Server Solutions GmbH",
            "Proxmox VE",
            None,
            json!([{"version": "8.0", "lessThan": "8.3", "status": "affected"}])
        )]),
        json!([]),
        Relevance::Cna,
    );
    w.run_at(THREE_DAYS_LATER);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (f.confidence.as_str(), f.decided_by.as_str(), f.manual),
        ("confirmed", "cna", true)
    );
    // CNA 又移除此產品 → 來源不再提到，人工命中維持未處理
    w.conn.execute("DELETE FROM cve_affected", []).unwrap();
    w.run_at(THREE_DAYS_LATER);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!((f.status.as_str(), f.manual), ("open", true));
}

/// code review 第四輪 #6：對已是未處理的命中套用時不寫事件；confirmed 命中不標記人工。
#[test]
fn applying_to_open_finding_is_a_no_op() {
    let (mut w, c) = proxmox_world();
    w.nvd(
        CVE,
        &[(
            "cpe:2.3:a:proxmox:virtual_environment:*:*:*:*:*:*:*:*",
            true,
            None,
            Some("8.2.4"),
        )],
    );
    w.conn
        .execute(
            "INSERT INTO pending (cve_id, reason, entered_at, last_checked) VALUES (?1, 'keyword_only', ?2, ?2)",
            rusqlite::params![CVE, T0],
        )
        .unwrap();
    w.run();
    // 比對後資料可判定，等待區已移出；重新放回以模擬使用者在比對前處理
    w.conn
        .execute(
            "UPDATE pending SET resolved_at = NULL, resolved_reason = NULL WHERE cve_id = ?1",
            [CVE],
        )
        .unwrap();
    let id = w.finding(c, CVE).unwrap().id;
    let events_before = w.events(id).len();
    let scoring = w.settings.scoring.clone();
    let resolution = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("PVE-01".into()),
        "",
        at(T0),
    )
    .unwrap();
    assert!(!resolution.applied[0].created && !resolution.applied[0].reopened);
    assert_eq!(w.events(id).len(), events_before);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!((f.confidence.as_str(), f.manual), ("confirmed", false));
}

/// code review 第六輪 #4、#6：`--apply` 跳過來源已判定不受影響的元件；
/// 對已不在受影響範圍的未處理命中，以使用者判定為準恢復為受影響。
#[test]
fn apply_skips_unaffected_components_and_restores_in_range() {
    let (mut w, pve) = proxmox_world();
    let srv_fw = w.asset(
        "SRV-01",
        "internal",
        &[("paloalto-panos", "11.1.4"), ("proxmox-ve", "8.2.2")],
    );
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
                json!([])
            )
        ]),
        json!([]),
        Relevance::Cna,
    );
    w.run();
    let scoring = w.settings.scoring.clone();
    let resolution = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("SRV-01".into()),
        "",
        at(T0),
    )
    .unwrap();
    let keys: Vec<&str> = resolution
        .applied
        .iter()
        .map(|a| a.product_key.as_str())
        .collect();
    assert_eq!(keys, vec!["proxmox-ve"]);
    assert!(w.finding(srv_fw[0], CVE).is_none());

    // 既有 open、但已不在受影響範圍的命中
    w.conn
        .execute(
            "UPDATE finding SET in_range = 0 WHERE component_id = ?1",
            [pve],
        )
        .unwrap();
    w.conn
        .execute(
            "UPDATE pending SET resolved_at = NULL, resolved_reason = NULL WHERE cve_id = ?1",
            [CVE],
        )
        .unwrap();
    pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("PVE-01".into()),
        "",
        at(T0),
    )
    .unwrap();
    let f = w.finding(pve, CVE).unwrap();
    assert_eq!(
        (f.status.as_str(), f.in_range, f.manual),
        ("open", true, true)
    );
}

/// code review 第七輪 #2：人工判定套用的命中，來源之後判定不受影響時也不被系統自動轉為已處理。
#[test]
fn manual_finding_is_not_auto_resolved() {
    let (mut w, c) = proxmox_world();
    w.run();
    let scoring = w.settings.scoring.clone();
    pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("PVE-01".into()),
        "",
        at(T0),
    )
    .unwrap();
    w.nvd(
        CVE,
        &[(
            "cpe:2.3:a:proxmox:virtual_environment:*:*:*:*:*:*:*:*",
            true,
            None,
            Some("8.0.0"),
        )],
    );
    let report = w.run_at(THREE_DAYS_LATER);
    assert_eq!(report.findings.resolved, 0);
    let f = w.finding(c, CVE).unwrap();
    assert_eq!(
        (f.status.as_str(), f.manual, f.in_range),
        ("open", true, false)
    );
}

/// code review 第八輪 #1：資產唯一的元件已被來源判定不受影響時，`--apply` 不以它作為後備對象。
#[test]
fn apply_does_not_fall_back_to_unaffected_component() {
    let (mut w, _) = proxmox_world();
    w.asset("FW-01", "internet", &[("paloalto-panos", "11.1.5")]);
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
                json!([])
            )
        ]),
        json!([]),
        Relevance::Cna,
    );
    w.run();
    let scoring = w.settings.scoring.clone();
    let err = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("FW-01".into()),
        "",
        at(T0),
    )
    .unwrap_err()
    .to_string();
    assert!(
        err.contains("不受影響") && err.contains("FW-01:<產品>"),
        "{err}"
    );
    // 明確指定元件時仍可套用（使用者判定）
    let ok = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("FW-01:paloalto-panos".into()),
        "",
        at(T0),
    )
    .unwrap();
    assert_eq!(ok.applied.len(), 1);
}

/// code review 第十輪 #3：資產編號含 `:` 時以完整編號查找。
#[test]
fn apply_target_with_colon_in_asset_tag() {
    let (mut w, _) = proxmox_world();
    let c = w.asset("IT:0042", "internal", &[("proxmox-ve", "8.2.2")])[0];
    w.run();
    let scoring = w.settings.scoring.clone();
    let r = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("it:0042".into()),
        "",
        at(T0),
    )
    .unwrap();
    assert_eq!(r.applied[0].finding_id, w.finding(c, CVE).unwrap().id);
    w.conn
        .execute(
            "UPDATE pending SET resolved_at = NULL, resolved_reason = NULL WHERE cve_id = ?1",
            [CVE],
        )
        .unwrap();
    let r = pending_ops::resolve(
        &mut w.conn,
        &w.catalog,
        &scoring,
        CVE,
        &Action::Apply("IT:0042:proxmox-ve".into()),
        "",
        at(T0),
    )
    .unwrap();
    assert_eq!(r.applied[0].product_key, "proxmox-ve");
}
