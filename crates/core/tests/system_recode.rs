//! 19 任務 1.2～1.3、2.1～2.2：修改資產說明代碼的預覽、衝突檢查與套用。

mod common;

use common::matching::World;
use common::vex::{vex_world, FW};
use cvewatch_core::assets::systems::{create_system, SystemField};
use cvewatch_core::assets::systems_edit::{
    edit_system, plan_system_edit, SystemEdit, SystemEditError, PLAN_CHANGED,
};
use cvewatch_core::assets::SystemCode;
use cvewatch_core::matching::run::match_is_stale;
use cvewatch_core::CoreError;
use rusqlite::params;

const HW: &str = "NET-HW-001";

fn edit(code: &str, new_code: &str, name: &str) -> SystemEdit {
    SystemEdit {
        code: SystemCode::parse(code).unwrap(),
        new_code: new_code.into(),
        name: name.into(),
    }
}

fn set(w: &World, tag: &str, isms: Option<&str>, code: &str) {
    w.conn
        .execute(
            "UPDATE asset SET isms_id = ?2, system_code = ?3 WHERE asset_tag = ?1",
            params![tag, isms, code],
        )
        .unwrap();
}

/// NET「網路服務」：active 的 NET-HW-001（原 FW-01，ISMS 編號相同，有命中與處理紀錄）、
/// retired 的 SW-CORE-02（ISMS 編號 NET-SW-002）、沒有 ISMS 編號的 FW-LAB；
/// SYS「核心系統」：WEB-01。比對已完成（「比對待補」未設定）。
fn net_world() -> World {
    let mut w = vex_world();
    create_system(&w.conn, "NET", "網路服務").unwrap();
    create_system(&w.conn, "SYS", "核心系統").unwrap();
    w.conn
        .execute(
            "UPDATE asset SET asset_tag = ?2 WHERE asset_tag = ?1",
            params![FW, HW],
        )
        .unwrap();
    set(&w, HW, Some(HW), "NET");
    w.asset("SW-CORE-02", "internal", &[("synology-dsm", "7.2.1-69057")]);
    w.retire("SW-CORE-02");
    set(&w, "SW-CORE-02", Some("NET-SW-002"), "NET");
    w.asset("FW-LAB", "internal", &[("synology-dsm", "7.2.1-69057")]);
    set(&w, "FW-LAB", None, "NET");
    set(&w, "WEB-01", Some("SYS-SW-001"), "SYS");
    w.run_at("2026-11-08T00:00:00+00:00");
    assert!(!match_is_stale(&w.conn).unwrap());
    w
}

/// (資產編號, ISMS 編號, 資產說明代碼, 狀態)，依資產編號排序。
fn assets(w: &World) -> Vec<(String, Option<String>, Option<String>, String)> {
    let mut stmt = w
        .conn
        .prepare("SELECT asset_tag, isms_id, system_code, status FROM asset ORDER BY asset_tag")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn systems(w: &World) -> Vec<(String, String)> {
    let mut stmt = w
        .conn
        .prepare("SELECT code, name FROM asset_system ORDER BY code")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

type Snapshot = (
    Vec<(String, Option<String>, Option<String>, String)>,
    Vec<(String, String)>,
);

fn snapshot(w: &World) -> Snapshot {
    (assets(w), systems(w))
}

/// (命中數, 處理事件數, 元件數, 版本歷史數)
fn records_of(w: &World, tag: &str) -> (i64, i64, i64, i64) {
    w.conn
        .query_row(
            "SELECT
               (SELECT count(*) FROM finding f JOIN component c ON c.id = f.component_id
                WHERE c.asset_id = a.id),
               (SELECT count(*) FROM finding_event e JOIN finding f ON f.id = e.finding_id
                JOIN component c ON c.id = f.component_id WHERE c.asset_id = a.id),
               (SELECT count(*) FROM component WHERE asset_id = a.id),
               (SELECT count(*) FROM component_history h JOIN component c ON c.id = h.component_id
                WHERE c.asset_id = a.id)
             FROM asset a WHERE a.asset_tag = ?1",
            [tag],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .expect(tag)
}

/// (cve, status) of the asset's findings
fn finding_states(w: &World, tag: &str) -> Vec<(String, String)> {
    let mut stmt = w
        .conn
        .prepare(
            "SELECT f.cve_id, f.status FROM finding f JOIN component c ON c.id = f.component_id
             JOIN asset a ON a.id = c.asset_id WHERE a.asset_tag = ?1 ORDER BY f.cve_id",
        )
        .unwrap();
    stmt.query_map([tag], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn no_orphans(w: &World) {
    let mut stmt = w.conn.prepare("PRAGMA foreign_key_check").unwrap();
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(rows.is_empty(), "外鍵孤兒列：{rows:?}");
}

fn field_errors(r: Result<impl std::fmt::Debug, SystemEditError>) -> Vec<(SystemField, String)> {
    match r {
        Err(SystemEditError::Fields(errors)) => {
            errors.into_iter().map(|e| (e.field, e.message)).collect()
        }
        other => panic!("應回報欄位錯誤：{other:?}"),
    }
}

// ── Preview before writing ──────────────────────────

#[test]
fn preview_lists_changes_without_writing() {
    let w = net_world();
    let before = snapshot(&w);
    let plan = plan_system_edit(&w.conn, &edit("NET", "nws", "網路服務")).unwrap();
    assert!(plan.code_changed());
    assert_eq!(plan.before.label(), "NET 網路服務");
    assert_eq!(plan.after.label(), "NWS 網路服務");
    let tags: Vec<&str> = plan.assets.iter().map(|a| a.asset_tag.as_str()).collect();
    assert_eq!(tags, ["FW-LAB", HW, "SW-CORE-02"], "依資產編號排序");
    assert_eq!(plan.retired(), 1);
    assert_eq!(
        plan.isms_changes(),
        [(HW, "NWS-HW-001"), ("NET-SW-002", "NWS-SW-002")]
    );
    assert_eq!(plan.tag_changes(), [(HW, "NWS-HW-001")]);
    assert_eq!(snapshot(&w), before, "預覽不寫入");
}

#[test]
fn name_only_edit_has_no_assets() {
    let w = net_world();
    let plan = plan_system_edit(&w.conn, &edit("NET", "net", "校園網路服務")).unwrap();
    assert!(!plan.code_changed());
    assert!(plan.assets.is_empty());
    assert_eq!(plan.after.label(), "NET 校園網路服務");
}

#[test]
fn lowercase_isms_ids_keep_the_rest() {
    let w = net_world();
    set(&w, "FW-LAB", Some("net-hw-009"), "NET");
    w.conn
        .execute(
            "UPDATE asset SET asset_tag = 'NET-hw-009' WHERE asset_tag = 'FW-LAB'",
            [],
        )
        .unwrap();
    let plan = plan_system_edit(&w.conn, &edit("NET", "NWS", "網路服務")).unwrap();
    let lab = plan
        .assets
        .iter()
        .find(|a| a.asset_tag == "NET-hw-009")
        .unwrap();
    assert_eq!(
        lab.isms_id,
        Some(("net-hw-009".to_string(), "NWS-hw-009".to_string()))
    );
    assert_eq!(lab.new_tag.as_deref(), Some("NWS-hw-009"), "不分大小寫相同");
}

// ── 衝突 ──────────────────────────

#[test]
fn new_code_already_used_is_rejected() {
    let mut w = net_world();
    let before = snapshot(&w);
    let errors = field_errors(edit_system(
        &mut w.conn,
        &edit("NET", "sys", "網路服務"),
        None,
    ));
    assert_eq!(
        errors,
        [(SystemField::Code, "資產說明代碼 SYS 已被使用".to_string())]
    );
    assert_eq!(snapshot(&w), before);
}

#[test]
fn new_asset_tag_already_used_is_rejected() {
    let mut w = net_world();
    w.asset("nws-hw-001", "internal", &[]);
    w.retire("nws-hw-001");
    w.asset("NWS-SW-002", "internal", &[]);
    w.conn
        .execute(
            "UPDATE asset SET asset_tag = 'NET-SW-002' WHERE asset_tag = 'SW-CORE-02'",
            [],
        )
        .unwrap();
    let before = snapshot(&w);
    let errors = field_errors(edit_system(
        &mut w.conn,
        &edit("NET", "NWS", "網路服務"),
        None,
    ));
    assert_eq!(
        errors,
        [
            (
                SystemField::Code,
                "資產編號 NWS-HW-001 已被使用（屬於已退役的資產）".to_string()
            ),
            (
                SystemField::Code,
                "資產編號 NWS-SW-002 已被使用".to_string()
            ),
        ],
        "收集全部衝突"
    );
    assert_eq!(snapshot(&w), before, "資料庫不變");
    let preview = field_errors(plan_system_edit(&w.conn, &edit("NET", "NWS", "網路服務")));
    assert_eq!(preview.len(), 2, "預覽也回報衝突");
}

#[test]
fn unknown_system_is_rejected() {
    let mut w = net_world();
    let r = edit_system(&mut w.conn, &edit("LAB", "NWS", "實驗室"), None);
    let err: CoreError = r.unwrap_err().into();
    match err {
        CoreError::Invalid(m) => assert!(m.contains("資產說明 LAB 不存在"), "{m}"),
        other => panic!("{other:?}"),
    }
}

// ── 套用 ──────────────────────────

#[test]
fn changing_a_code_cascades_to_assets() {
    let mut w = net_world();
    let plan = edit_system(&mut w.conn, &edit("NET", "nws", "網路服務"), None).unwrap();
    assert_eq!(plan.assets.len(), 3);
    assert_eq!(
        systems(&w),
        [
            ("NWS".to_string(), "網路服務".to_string()),
            ("SYS".to_string(), "核心系統".to_string())
        ]
    );
    let s = |v: &str| Some(v.to_string());
    assert_eq!(
        assets(&w),
        [
            ("FW-LAB".into(), None, s("NWS"), "active".into()),
            (
                "NWS-HW-001".into(),
                s("NWS-HW-001"),
                s("NWS"),
                "active".into()
            ),
            (
                "SW-CORE-02".into(),
                s("NWS-SW-002"),
                s("NWS"),
                "retired".into()
            ),
            ("WEB-01".into(), s("SYS-SW-001"), s("SYS"), "active".into()),
        ]
    );
    no_orphans(&w);
    let fk: i64 = w
        .conn
        .query_row("PRAGMA defer_foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fk, 0, "延後外鍵檢查只在交易內有效");
}

#[test]
fn changing_the_code_and_name_together() {
    let mut w = net_world();
    let plan = edit_system(&mut w.conn, &edit("NET", "NWS", "校園網路服務"), None).unwrap();
    assert_eq!(plan.after.label(), "NWS 校園網路服務");
    assert_eq!(systems(&w)[0], ("NWS".into(), "校園網路服務".into()));
    no_orphans(&w);
}

#[test]
fn renamed_asset_keeps_its_records() {
    let mut w = net_world();
    let records = records_of(&w, HW);
    let states = finding_states(&w, HW);
    assert_eq!(records.0, 4, "NET-HW-001 有 4 筆命中");
    assert!(records.1 > 0, "有處理事件");
    w.conn
        .execute(
            "INSERT INTO component_history (component_id, version_raw, changed_at)
             SELECT c.id, '11.1.2', ?2 FROM component c JOIN asset a ON a.id = c.asset_id
             WHERE a.asset_tag = ?1",
            params![HW, common::matching::T0],
        )
        .unwrap();
    let records = records_of(&w, HW);
    edit_system(&mut w.conn, &edit("NET", "NWS", "網路服務"), None).unwrap();
    assert_eq!(records_of(&w, "NWS-HW-001"), records);
    assert_eq!(finding_states(&w, "NWS-HW-001"), states);
    assert!(!match_is_stale(&w.conn).unwrap(), "不記錄「比對待補」");
}

#[test]
fn renaming_only_keeps_isms_ids() {
    let mut w = net_world();
    let before = assets(&w);
    let plan = edit_system(&mut w.conn, &edit("NET", "NET", "校園網路服務"), None).unwrap();
    assert!(plan.assets.is_empty());
    assert_eq!(systems(&w)[0], ("NET".into(), "校園網路服務".into()));
    assert_eq!(assets(&w), before, "ISMS 編號與歸屬不變");
}

#[test]
fn recoding_a_system_without_assets() {
    let mut w = net_world();
    create_system(&w.conn, "LAB", "實驗室").unwrap();
    let plan = edit_system(&mut w.conn, &edit("LAB", "tst", "實驗室"), None).unwrap();
    assert!(plan.code_changed() && plan.assets.is_empty());
    let codes: Vec<String> = systems(&w).into_iter().map(|(c, _)| c).collect();
    assert_eq!(codes, ["NET", "SYS", "TST"]);
}

// ── code review 修正 ──────────────────────────

/// 全形與半形的編號在資料庫中是不同值，但改代碼後會變成同一個：整批拒絕，不留到寫入時才失敗。
#[test]
fn duplicates_within_the_batch_are_rejected() {
    let mut w = net_world();
    w.conn
        .execute(
            "UPDATE asset SET asset_tag = 'ＮＥＴ-HW-001', isms_id = 'ＮＥＴ-HW-001'
             WHERE asset_tag = 'FW-LAB'",
            [],
        )
        .unwrap();
    let before = snapshot(&w);
    let errors = field_errors(edit_system(
        &mut w.conn,
        &edit("NET", "NWS", "網路服務"),
        None,
    ));
    assert_eq!(
        errors,
        [
            (
                SystemField::Code,
                "資產編號 NWS-HW-001 在這次修改中重複".to_string()
            ),
            (
                SystemField::Code,
                "ISMS 編號 NWS-HW-001 在這次修改中重複".to_string()
            ),
        ]
    );
    assert_eq!(snapshot(&w), before);
}

/// 系統代碼以第一個 `-` 之前判斷（與 membership 規則相同）；資產編號與 ISMS 編號只差全形半形
/// 時視為相同，改為新 ISMS 編號。
#[test]
fn isms_prefix_follows_the_membership_rule() {
    let w = net_world();
    set(&w, "FW-LAB", Some("NETX-HW-001"), "NET");
    w.conn
        .execute(
            "UPDATE asset SET asset_tag = 'ＮＥＴ-SW-002' WHERE asset_tag = 'SW-CORE-02'",
            [],
        )
        .unwrap();
    let plan = plan_system_edit(&w.conn, &edit("NET", "NWS", "網路服務")).unwrap();
    let lab = plan
        .assets
        .iter()
        .find(|a| a.asset_tag == "FW-LAB")
        .unwrap();
    assert_eq!(lab.isms_id, None, "NETX 不是 NET 的編號");
    let sw = plan
        .assets
        .iter()
        .find(|a| a.asset_tag == "ＮＥＴ-SW-002")
        .unwrap();
    assert_eq!(
        sw.new_tag.as_deref(),
        Some("NWS-SW-002"),
        "新編號為新 ISMS 編號"
    );
}

/// 確認後受影響的資產有變動：不寫入，回傳新計畫供重新確認。
#[test]
fn changed_plan_after_confirmation_is_not_written() {
    let mut w = net_world();
    let confirmed = plan_system_edit(&w.conn, &edit("NET", "NWS", "網路服務")).unwrap();
    w.asset("NET-HW-003", "internal", &[]);
    set(&w, "NET-HW-003", Some("NET-HW-003"), "NET");
    let before = snapshot(&w);
    let r = edit_system(
        &mut w.conn,
        &edit("NET", "NWS", "網路服務"),
        Some(&confirmed),
    );
    let Err(SystemEditError::Changed(plan)) = r else {
        panic!("應回報計畫已變動：{r:?}");
    };
    assert_eq!(plan.assets.len(), 4);
    assert_eq!(snapshot(&w), before, "資料庫不變");
    let err: CoreError = SystemEditError::Changed(plan.clone()).into();
    assert!(matches!(err, CoreError::Invalid(m) if m == PLAN_CHANGED));
    edit_system(&mut w.conn, &edit("NET", "NWS", "網路服務"), Some(&plan)).unwrap();
    assert_eq!(systems(&w)[0].0, "NWS");
}
