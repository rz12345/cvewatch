//! `cvewatch asset system recode <舊代碼> <新代碼> [--dry-run]`（19 design D7）：修改資產說明代碼。
//!
//! 名稱沿用原名稱；輸出受影響的資產數與每筆 ISMS 編號、資產編號的變更，成功時提醒外部資料的
//! 舊代碼與舊編號不會對應。`--dry-run` 只預覽。失敗時以 `CoreError::Invalid` 結束（與
//! `asset rename` 相同）。

use cvewatch_core::assets::systems_edit::{
    edit_system, plan_system_edit, SystemEdit, SystemEditPlan,
};
use cvewatch_core::assets::{SystemCode, SystemDirectory};
use cvewatch_core::db::open_working_db;
use cvewatch_core::{CoreError, ExitCode};
use cvewatch_frontend::workspace::systems::{affected_text, RECODE_NOTE};

use crate::app::App;
use crate::cli::SystemAction;

/// 預覽時取代結果行與提醒的文字。
const DRY_RUN_NOTE: &str = "預覽：未寫入資料庫";

pub fn run(app: &App, action: SystemAction) -> Result<ExitCode, CoreError> {
    match action {
        SystemAction::Recode {
            code,
            new_code,
            dry_run,
        } => recode(app, &code, &new_code, dry_run),
    }
}

fn not_found(code: &str) -> CoreError {
    CoreError::Invalid(format!("找不到資產說明 {}", code.trim()))
}

fn recode(app: &App, code: &str, new_code: &str, dry_run: bool) -> Result<ExitCode, CoreError> {
    let old = SystemCode::parse(code).ok_or_else(|| not_found(code))?;
    if SystemCode::parse(new_code).as_ref() == Some(&old) {
        return Err(CoreError::Invalid(format!("新代碼與原代碼相同：{old}")));
    }
    let mut conn = open_working_db(&app.paths, &app.store)?;
    let current = SystemDirectory::load(&conn)?
        .get(&old)
        .cloned()
        .ok_or_else(|| not_found(old.as_str()))?;
    let edit = SystemEdit {
        code: old,
        new_code: new_code.to_string(),
        name: current.name,
    };
    let plan = if dry_run {
        plan_system_edit(&conn, &edit)?
    } else {
        // 同一交易內計算並寫入，沒有確認時差
        edit_system(&mut conn, &edit, None)?
    };
    for line in recode_lines(&plan, dry_run) {
        println!("{line}");
    }
    Ok(ExitCode::Success)
}

/// 輸出內容（19 design D7）：標題、受影響資產數與變更清單（有資產歸屬時）、結果行與提醒
/// （`dry_run` 時兩者改為「預覽：未寫入資料庫」）。
pub(crate) fn recode_lines(plan: &SystemEditPlan, dry_run: bool) -> Vec<String> {
    let mut lines = vec![format!(
        "資產說明 {} → {}",
        plan.before.label(),
        plan.after.label()
    )];
    if !plan.assets.is_empty() {
        lines.push(affected_text(plan));
        push_changes(&mut lines, "ISMS 編號：", &plan.isms_changes());
        push_changes(&mut lines, "資產編號：", &plan.tag_changes());
    }
    if dry_run {
        lines.push(DRY_RUN_NOTE.to_string());
    } else {
        lines.push(format!(
            "已將資產說明代碼 {} 改為 {}",
            plan.before.code, plan.after.code
        ));
        lines.push(RECODE_NOTE.to_string());
    }
    lines
}

fn push_changes(lines: &mut Vec<String>, heading: &str, changes: &[(&str, &str)]) {
    if changes.is_empty() {
        return;
    }
    lines.push(heading.to_string());
    lines.extend(changes.iter().map(|(from, to)| format!("  {from} → {to}")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use cvewatch_core::assets::systems_edit::RecodedAsset;
    use cvewatch_core::assets::{AssetStatus, AssetSystem};

    fn system(code: &str) -> AssetSystem {
        AssetSystem {
            code: SystemCode::parse(code).unwrap(),
            name: "網路服務".into(),
        }
    }

    fn plan(assets: Vec<RecodedAsset>) -> SystemEditPlan {
        SystemEditPlan {
            before: system("NET"),
            after: system("NWS"),
            assets,
        }
    }

    fn hw() -> RecodedAsset {
        RecodedAsset {
            id: 1,
            asset_tag: "NET-HW-001".into(),
            status: AssetStatus::Active,
            isms_id: Some(("NET-HW-001".into(), "NWS-HW-001".into())),
            new_tag: Some("NWS-HW-001".into()),
        }
    }

    /// Scenario: Changing a code on the command line
    #[test]
    fn changing_a_code_lists_changes_and_the_note() {
        assert_eq!(
            recode_lines(&plan(vec![hw()]), false),
            [
                "資產說明 NET 網路服務 → NWS 網路服務",
                "受影響的資產：1 個",
                "ISMS 編號：",
                "  NET-HW-001 → NWS-HW-001",
                "資產編號：",
                "  NET-HW-001 → NWS-HW-001",
                "已將資產說明代碼 NET 改為 NWS",
                RECODE_NOTE,
            ]
        );
        assert!(RECODE_NOTE.contains("cvewatch asset export"));
    }

    /// Scenario: Dry run
    #[test]
    fn dry_run_lists_the_same_changes() {
        let retired = RecodedAsset {
            id: 2,
            asset_tag: "SW-CORE-02".into(),
            status: AssetStatus::Retired,
            isms_id: Some(("NET-SW-002".into(), "NWS-SW-002".into())),
            new_tag: None,
        };
        assert_eq!(
            recode_lines(&plan(vec![hw(), retired]), true),
            [
                "資產說明 NET 網路服務 → NWS 網路服務",
                "受影響的資產：2 個（含已退役 1 個）",
                "ISMS 編號：",
                "  NET-HW-001 → NWS-HW-001",
                "  NET-SW-002 → NWS-SW-002",
                "資產編號：",
                "  NET-HW-001 → NWS-HW-001",
                DRY_RUN_NOTE,
            ]
        );
    }

    #[test]
    fn without_assets_only_the_title_and_result() {
        assert_eq!(
            recode_lines(&plan(Vec::new()), false),
            [
                "資產說明 NET 網路服務 → NWS 網路服務",
                "已將資產說明代碼 NET 改為 NWS",
                RECODE_NOTE,
            ]
        );
        assert_eq!(
            recode_lines(&plan(Vec::new()), true),
            ["資產說明 NET 網路服務 → NWS 網路服務", DRY_RUN_NOTE]
        );
    }
}
