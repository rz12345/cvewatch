//! 12 任務 1.1～1.4：資產更名（驗證、保留紀錄、不記錄比對待補、與其他編修同一交易），
//! 以及更名後外部資料不認舊編號（清冊 `--prune` 重匯、VEX bom-ref、重新使用舊編號）。

mod common;

use common::matching::{at, World};
use common::vex::{vex_world, FW, OPEN_CVE, RESOLVED_CVE};
use cvewatch_core::assets::columns::SheetKind;
use cvewatch_core::assets::edit::{self, EditOp, NewComponent};
use cvewatch_core::assets::import::{import_inventory, ImportOptions};
use cvewatch_core::assets::validate::{
    validate_record, IncomingAsset, IncomingComponent, Inventory, RecordInput, RowRef,
};
use cvewatch_core::assets::{AssetRepo, AssetStatus};
use cvewatch_core::matching::run::match_is_stale;
use cvewatch_core::vex::import::{self, ImportOptions as VexOptions, Method};
use cvewatch_core::CoreError;
use rusqlite::params;
use serde_json::json;

const NEW: &str = "FW-11";
const NOW: &str = "2026-11-08T00:00:00+00:00";

fn now() -> chrono::DateTime<chrono::Utc> {
    at(NOW)
}

fn message(r: Result<impl std::fmt::Debug, CoreError>) -> String {
    match r {
        Err(CoreError::Invalid(m)) => m,
        other => panic!("應以 Invalid 拒絕：{other:?}"),
    }
}

fn tags(w: &World) -> Vec<(String, String)> {
    let mut stmt = w
        .conn
        .prepare("SELECT asset_tag, status FROM asset ORDER BY asset_tag")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// (命中數, 處理事件數, 元件數, 版本歷史數, 未辨識產品數)
fn records_of(w: &World, tag: &str) -> (i64, i64, i64, i64, i64) {
    w.conn
        .query_row(
            "SELECT
               (SELECT count(*) FROM finding f JOIN component c ON c.id = f.component_id
                WHERE c.asset_id = a.id),
               (SELECT count(*) FROM finding_event e JOIN finding f ON f.id = e.finding_id
                JOIN component c ON c.id = f.component_id WHERE c.asset_id = a.id),
               (SELECT count(*) FROM component WHERE asset_id = a.id),
               (SELECT count(*) FROM component_history h JOIN component c ON c.id = h.component_id
                WHERE c.asset_id = a.id),
               (SELECT count(*) FROM asset_unresolved WHERE asset_id = a.id)
             FROM asset a WHERE a.asset_tag = ?1",
            [tag],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .expect(tag)
}

/// FW-01 加上一筆版本歷史與一筆未辨識產品，並清除「比對待補」。
fn fw_world() -> World {
    let mut w = vex_world();
    let id: i64 = w
        .conn
        .query_row(
            "SELECT c.id FROM component c JOIN asset a ON a.id = c.asset_id
             WHERE a.asset_tag = ?1",
            [FW],
            |r| r.get(0),
        )
        .unwrap();
    w.conn
        .execute(
            "INSERT INTO component_history (component_id, version_raw, changed_at)
             VALUES (?1, '11.1.2', ?2)",
            params![id, common::matching::T0],
        )
        .unwrap();
    w.conn
        .execute(
            "INSERT INTO asset_unresolved (asset_id, product_raw, version_raw, seen_at)
             VALUES ((SELECT id FROM asset WHERE asset_tag = ?1), 'GlobalProtect', '6.2', ?2)",
            params![FW, common::matching::T0],
        )
        .unwrap();
    w.run_at(NOW);
    assert!(!match_is_stale(&w.conn).unwrap());
    w
}

fn status_of(w: &World, tag: &str) -> AssetStatus {
    AssetRepo::new(&w.conn)
        .find_by_tag(tag)
        .unwrap()
        .expect(tag)
        .status
}

// ── Requirement: Renaming an asset ──────────────────────────

#[test]
fn renaming_keeps_findings_triage_history_and_unresolved() {
    let mut w = fw_world();
    let before = records_of(&w, FW);
    assert_eq!(before.0, 4, "FW-01 有 4 筆命中");
    assert!(before.1 > 0 && before.3 == 1 && before.4 == 1);
    edit::rename_asset(&mut w.conn, FW, NEW).unwrap();
    assert!(AssetRepo::new(&w.conn).find_by_tag(FW).unwrap().is_none());
    assert_eq!(records_of(&w, NEW), before);
    let status: String = w
        .conn
        .query_row(
            "SELECT f.status FROM finding f JOIN component c ON c.id = f.component_id
             JOIN asset a ON a.id = c.asset_id WHERE a.asset_tag = ?1 AND f.cve_id = ?2",
            params![NEW, RESOLVED_CVE],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "resolved", "處理狀態保留");
}

#[test]
fn renaming_alone_does_not_mark_matching_stale() {
    let mut w = fw_world();
    edit::rename_asset(&mut w.conn, FW, NEW).unwrap();
    assert!(!match_is_stale(&w.conn).unwrap());
}

#[test]
fn new_tag_is_trimmed() {
    let mut w = fw_world();
    edit::rename_asset(&mut w.conn, FW, "  FW-11 ").unwrap();
    assert_eq!(status_of(&w, NEW), AssetStatus::Active);
}

#[test]
fn changing_only_letter_case_is_allowed() {
    let mut w = fw_world();
    edit::rename_asset(&mut w.conn, FW, "fw-01").unwrap();
    assert_eq!(status_of(&w, "fw-01"), AssetStatus::Active);
    assert!(AssetRepo::new(&w.conn).find_by_tag(FW).unwrap().is_none());
}

#[test]
fn tag_used_by_another_active_or_retired_asset_is_rejected() {
    let mut w = fw_world();
    // WEB-01 為 active；另建一個 retired 的 NAS-02
    w.asset("NAS-02", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.retire("NAS-02");
    let before = tags(&w);
    let active = message(edit::rename_asset(&mut w.conn, FW, "web-01"));
    assert!(
        active.contains("WEB-01") && active.contains("已被使用"),
        "{active}"
    );
    let retired = message(edit::rename_asset(&mut w.conn, FW, "nas-02"));
    assert!(
        retired.contains("NAS-02") && retired.contains("已退役"),
        "{retired}"
    );
    assert_eq!(tags(&w), before, "資產不變");
}

#[test]
fn blank_or_unchanged_tag_is_rejected() {
    let mut w = fw_world();
    let blank = message(edit::rename_asset(&mut w.conn, FW, "  "));
    assert!(blank.contains("不可空白"), "{blank}");
    let same = message(edit::rename_asset(&mut w.conn, FW, FW));
    assert!(same.contains("相同"), "{same}");
    assert_eq!(status_of(&w, FW), AssetStatus::Active);
}

#[test]
fn renaming_a_retired_or_unknown_asset_is_rejected() {
    let mut w = fw_world();
    w.retire(FW);
    let retired = message(edit::rename_asset(&mut w.conn, FW, NEW));
    assert!(retired.contains("恢復"), "{retired}");
    let unknown = message(edit::rename_asset(&mut w.conn, "NAS-99", NEW));
    assert!(unknown.contains("找不到資產 NAS-99"), "{unknown}");
    assert!(AssetRepo::new(&w.conn).find_by_tag(NEW).unwrap().is_none());
}

#[test]
fn rename_with_other_edits_saves_together() {
    let mut w = fw_world();
    let input = RecordInput {
        name: "fw11",
        category: "security",
        ..RecordInput::default()
    };
    edit::apply_edits(
        &mut w.conn,
        &w.catalog,
        FW,
        &[EditOp::Rename(NEW), EditOp::Fields(input)],
        now(),
    )
    .unwrap();
    let r = AssetRepo::new(&w.conn).find_by_tag(NEW).unwrap().unwrap();
    assert_eq!(r.record.name, "fw11");
    assert_eq!(r.record.asset_tag, NEW);
    assert!(
        match_is_stale(&w.conn).unwrap(),
        "與其他修改一起儲存時照常比對"
    );
}

#[test]
fn failing_rename_with_other_edits_saves_nothing() {
    let mut w = fw_world();
    let input = RecordInput {
        name: "fw11",
        category: "security",
        ..RecordInput::default()
    };
    let msg = message(edit::apply_edits(
        &mut w.conn,
        &w.catalog,
        FW,
        &[EditOp::Rename("WEB-01"), EditOp::Fields(input)],
        now(),
    ));
    assert!(msg.contains("已被使用"), "{msg}");
    let r = AssetRepo::new(&w.conn).find_by_tag(FW).unwrap().unwrap();
    assert_eq!(r.record.name, FW, "名稱不變");
}

#[test]
fn later_steps_after_rename_use_the_new_tag() {
    let mut w = fw_world();
    let msg = message(edit::apply_edits(
        &mut w.conn,
        &w.catalog,
        FW,
        &[EditOp::Rename(NEW), EditOp::RemoveComponent(999_999)],
        now(),
    ));
    assert!(msg.starts_with(NEW), "錯誤訊息使用新編號：{msg}");
    assert_eq!(status_of(&w, FW), AssetStatus::Active, "整批不寫入");
}

// ── Requirement: Old asset tag is not remembered ────────────

fn old_inventory() -> Inventory {
    let row = RowRef {
        sheet: SheetKind::Assets,
        number: 2,
    };
    Inventory {
        assets: vec![
            IncomingAsset {
                row,
                record: validate_record(&RecordInput {
                    asset_tag: FW,
                    name: FW,
                    category: "security",
                    exposure: "internet",
                    ..RecordInput::default()
                })
                .unwrap(),
                components: vec![IncomingComponent {
                    row,
                    product_raw: "PAN-OS".into(),
                    version_raw: "11.1.4-h7".into(),
                }],
            },
            IncomingAsset {
                row: RowRef { number: 3, ..row },
                record: validate_record(&RecordInput {
                    asset_tag: "WEB-01",
                    name: "WEB-01",
                    category: "server",
                    exposure: "dmz",
                    ..RecordInput::default()
                })
                .unwrap(),
                components: vec![],
            },
        ],
    }
}

#[test]
fn importing_the_old_inventory_with_prune_after_a_rename() {
    let mut w = fw_world();
    edit::rename_asset(&mut w.conn, FW, NEW).unwrap();
    let findings = records_of(&w, NEW).0;
    let plan = import_inventory(
        &mut w.conn,
        &w.catalog,
        &old_inventory(),
        ImportOptions {
            dry_run: false,
            prune: true,
        },
        now(),
    )
    .unwrap();
    assert_eq!(plan.added, vec![FW.to_string()]);
    assert_eq!(plan.disappeared, vec![NEW.to_string()]);
    assert_eq!(status_of(&w, FW), AssetStatus::Active);
    assert_eq!(status_of(&w, NEW), AssetStatus::Retired);
    assert_eq!(records_of(&w, NEW).0, findings, "命中與處理紀錄保留");
}

#[test]
fn vex_with_the_old_tag_is_unmatched() {
    let mut w = fw_world();
    edit::rename_asset(&mut w.conn, FW, NEW).unwrap();
    let reference = format!("cvewatch:{FW}:paloalto-panos");
    let doc = json!({
        "bomFormat": "CycloneDX", "specVersion": "1.6",
        "vulnerabilities": [{"id": OPEN_CVE, "analysis": {"state": "resolved"},
                             "affects": [{"ref": reference}]}]
    });
    let bom = import::doc::parse(doc.to_string().as_bytes()).unwrap();
    let report = import::import(
        &mut w.conn,
        &bom,
        "old.json",
        VexOptions::default(),
        now(),
        &|_, _| Ok(()),
    )
    .unwrap();
    assert_eq!(report.plan.unmatched.len(), 1);
    assert_eq!(report.plan.unmatched[0].method, Some(Method::BomRef));
    let status: String = w
        .conn
        .query_row(
            "SELECT f.status FROM finding f JOIN component c ON c.id = f.component_id
             JOIN asset a ON a.id = c.asset_id WHERE a.asset_tag = ?1 AND f.cve_id = ?2",
            params![NEW, OPEN_CVE],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "open", "FW-11 的命中不變");
}

#[test]
fn the_old_tag_can_be_reused_by_a_new_asset() {
    let mut w = fw_world();
    edit::rename_asset(&mut w.conn, FW, NEW).unwrap();
    let input = RecordInput {
        asset_tag: FW,
        name: "fw01-new",
        category: "security",
        ..RecordInput::default()
    };
    edit::create_asset(
        &mut w.conn,
        &w.catalog,
        &input,
        &[NewComponent {
            product_key: "paloalto-panos".into(),
            version_raw: "11.2.3".into(),
        }],
        now(),
    )
    .unwrap();
    assert_eq!(records_of(&w, FW).0, 0, "新資產尚未比對，沒有命中");
    assert_eq!(records_of(&w, NEW).0, 4, "更名後的資產不受影響");
}

// ── code review：只更名時 apply_edits 不記錄比對待補；匯入不新增只差大小寫的編號 ──

#[test]
fn apply_edits_with_only_a_rename_does_not_mark_matching_stale() {
    let mut w = fw_world();
    edit::apply_edits(&mut w.conn, &w.catalog, FW, &[EditOp::Rename(NEW)], now()).unwrap();
    assert_eq!(status_of(&w, NEW), AssetStatus::Active);
    assert!(!match_is_stale(&w.conn).unwrap());
    assert!(!EditOp::Rename(NEW).affects_matching());
}

#[test]
fn rename_asset_returns_the_stored_tag() {
    let mut w = fw_world();
    let stored = edit::rename_asset(&mut w.conn, FW, "  FW-11 ").unwrap();
    assert_eq!(stored, NEW);
}

#[test]
fn import_rejects_a_tag_differing_only_in_case_from_an_existing_asset() {
    let mut w = fw_world();
    edit::rename_asset(&mut w.conn, FW, "fw-01").unwrap();
    let before = tags(&w);
    let err = import_inventory(
        &mut w.conn,
        &w.catalog,
        &old_inventory(),
        ImportOptions::default(),
        now(),
    )
    .unwrap_err();
    let CoreError::ImportRejected(errors) = err else {
        panic!("應以 ImportRejected 拒絕：{err:?}");
    };
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("第 2 列") && errors[0].contains("FW-01") && errors[0].contains("fw-01"),
        "{}",
        errors[0]
    );
    assert!(errors[0].contains("只差大小寫"), "{}", errors[0]);
    assert_eq!(tags(&w), before, "資料庫不變");
}
