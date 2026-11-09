//! `cvewatch asset import-isms <workbook> [--dry-run]`（18 design D6）：從 ISMS 資訊資產清冊匯入。
//!
//! 先印 ISMS 報表，再印與 `asset import` 相同格式的匯入差異（欄位變更列出新舊值），最後提醒
//! 版本待補的元件未納入比對。結束碼與 `asset import` 相同。

use std::path::Path;

use cvewatch_core::assets::isms::{import_isms as core_import_isms, IsmsImport};
use cvewatch_core::assets::pending::PENDING_REMINDER;
use cvewatch_core::assets::plan::ImportPlan;
use cvewatch_core::assets::AssetClasses;
use cvewatch_core::db::open_working_db;
use cvewatch_core::{CoreError, ExitCode};

use super::asset::rematch_after_import;
use super::asset_view::{pending_line, plan_detail_lines_with, FieldDetail};
use crate::app::App;

pub fn import_isms(app: &App, workbook: &Path, dry_run: bool) -> Result<ExitCode, CoreError> {
    let catalog = app.load_catalog()?;
    let classes = app.loaded.config.asset_classes()?;
    // 先開資料庫：尚未初始化時不讀 ISMS 清冊（spec Database not initialized）
    let mut conn = open_working_db(&app.paths, &app.store)?;
    let result = core_import_isms(
        &mut conn,
        &classes,
        &catalog,
        workbook,
        dry_run,
        chrono::Utc::now(),
    )?;
    for line in isms_lines(&result, dry_run, &classes) {
        println!("{line}");
    }
    if dry_run {
        return Ok(ExitCode::Success);
    }
    Ok(rematch_after_import(app, &mut conn, &catalog))
}

/// 「  新增 5、更新 1（版本變更 0）、未變 0、匯入後版本待補 7 個元件、略過 10 列」（待補數含
/// 先前已待補的元件；這次新加入的數量在 ISMS 報表的摘要）。
fn summary_line(result: &IsmsImport) -> String {
    let plan = &result.plan;
    format!(
        "  新增 {}、更新 {}（版本變更 {}）、未變 {}、匯入後版本待補 {} 個元件、略過 {} 列",
        plan.added.len(),
        plan.updated.len(),
        plan.version_change_count(),
        plan.unchanged,
        plan.pending_components(),
        result.report.skipped()
    )
}

/// 匯入差異的明細：未辨識產品由 ISMS 報表的「未辨識的描述行」列出、不在清冊中的資產由
/// 「不在 ISMS 清冊」列出（ISMS 匯入不退役），這裡不重複。
fn detail_lines(plan: &ImportPlan, classes: &AssetClasses) -> Vec<String> {
    let shown = ImportPlan {
        unresolved: Vec::new(),
        disappeared: Vec::new(),
        ..plan.clone()
    };
    plan_detail_lines_with(&shown, FieldDetail::Values(classes))
}

/// CLI 輸出（GUI 預覽顯示同一份輸出）。
pub(crate) fn isms_lines(
    result: &IsmsImport,
    dry_run: bool,
    classes: &AssetClasses,
) -> Vec<String> {
    let title = if dry_run {
        "ISMS 匯入預覽（--dry-run，未寫入資料庫）"
    } else {
        "ISMS 匯入完成"
    };
    let mut out = vec![title.to_string(), summary_line(result)];
    out.extend(result.report.cli_lines());
    out.extend(detail_lines(&result.plan, classes));
    if pending_line(&result.plan).is_some() {
        out.push(PENDING_REMINDER.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use cvewatch_core::catalog::Catalog;
    use rusqlite::{params, Connection};

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name)
    }

    fn catalog() -> Catalog {
        Catalog::load(&fixture("isms_catalog.d")).unwrap()
    }

    fn memory_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        cvewatch_core::db::migrate::migrate(&mut conn).unwrap();
        conn
    }

    fn run(conn: &mut Connection, dry_run: bool) -> IsmsImport {
        core_import_isms(
            conn,
            &AssetClasses::builtin(),
            &catalog(),
            &fixture("isms_workbook.xlsx"),
            dry_run,
            chrono::Utc::now(),
        )
        .unwrap()
    }

    fn assets(conn: &Connection) -> i64 {
        conn.query_row("SELECT count(*) FROM asset", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn previewing_and_importing_print_counts() {
        let classes = AssetClasses::builtin();
        let mut conn = memory_db();
        let preview = isms_lines(&run(&mut conn, true), true, &classes);
        assert_eq!(assets(&conn), 0, "--dry-run 不改資料庫");
        assert_eq!(preview[0], "ISMS 匯入預覽（--dry-run，未寫入資料庫）");
        assert_eq!(
            preview[1],
            "  新增 6、更新 0（版本變更 0）、未變 0、匯入後版本待補 4 個元件、略過 5 列"
        );
        assert!(preview.contains(&"略過 PE 1、IF 2、SE 1".to_string()));
        assert_eq!(preview.last().unwrap(), PENDING_REMINDER);

        let done = isms_lines(&run(&mut conn, false), false, &classes);
        assert_eq!(assets(&conn), 6);
        assert_eq!(done[0], "ISMS 匯入完成");
        assert_eq!(done[1], preview[1]);
    }

    #[test]
    fn existing_asset_lists_old_and_new_values() {
        let classes = AssetClasses::builtin();
        let mut conn = memory_db();
        run(&mut conn, false);
        conn.execute(
            "UPDATE asset SET confidentiality = 2, custodian = ?1 WHERE asset_tag = 'NET-HW-002'",
            params!["李小華"],
        )
        .unwrap();
        let lines = isms_lines(&run(&mut conn, true), true, &classes);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("NET-HW-002 欄位變更：機密性 2 → 3、保管人 李小華 → 王小明")),
            "{lines:#?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("清冊中消失")),
            "{lines:#?}"
        );
    }
}
