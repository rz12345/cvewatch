//! `cvewatch finding list | show | set | conflicts`

use cvewatch_core::db::open_working_db;
use cvewatch_core::matching::engine::Confidence;
use cvewatch_core::matching::query::{self, FindingFilter};
use cvewatch_core::matching::scoring::Priority;
use cvewatch_core::matching::triage::{self, ChangeRequest, Status};
use cvewatch_core::notify::triage_notes;
use cvewatch_core::{CoreError, ExitCode};

use super::finding_view;
use crate::app::App;
use crate::cli::FindingAction;

pub fn run(app: &App, action: FindingAction) -> Result<ExitCode, CoreError> {
    match action {
        FindingAction::List {
            status,
            priority,
            asset,
            confidence,
        } => list(
            app,
            filter(&status, &priority, asset, confidence.as_deref())?,
        ),
        FindingAction::Show { id } => show(app, id),
        FindingAction::Set {
            id,
            status,
            reason,
            justification,
            note,
        } => set(
            app,
            id,
            &ChangeRequest {
                status,
                reason,
                justification,
                note,
            },
        ),
        FindingAction::Conflicts => conflicts(app),
    }
}

fn filter(
    statuses: &[String],
    priorities: &[String],
    asset: Option<String>,
    confidence: Option<&str>,
) -> Result<FindingFilter, CoreError> {
    let invalid = |what: &str, v: &str| CoreError::Invalid(format!("未知的{what} `{v}`"));
    Ok(FindingFilter {
        statuses: statuses
            .iter()
            .map(|s| Status::parse(s).ok_or_else(|| invalid("狀態", s)))
            .collect::<Result<_, _>>()?,
        priorities: priorities
            .iter()
            .map(|p| Priority::parse(p).ok_or_else(|| invalid("等級", p)))
            .collect::<Result<_, _>>()?,
        asset_tag: asset,
        confidence: confidence
            .map(|c| Confidence::parse(c).ok_or_else(|| invalid("信心等級", c)))
            .transpose()?,
        first_seen_from: None,
    })
}

fn list(app: &App, filter: FindingFilter) -> Result<ExitCode, CoreError> {
    let conn = open_working_db(&app.paths, &app.store)?;
    let rows = query::list(&conn, &filter)?;
    finding_view::print_list(&rows);
    Ok(ExitCode::Success)
}

fn show(app: &App, id: i64) -> Result<ExitCode, CoreError> {
    let conn = open_working_db(&app.paths, &app.store)?;
    match query::show(&conn, id)? {
        Some(detail) => finding_view::print_detail(&detail),
        None => return Err(CoreError::Invalid(format!("找不到命中 {id}"))),
    }
    Ok(ExitCode::Success)
}

fn set(app: &App, id: i64, request: &ChangeRequest) -> Result<ExitCode, CoreError> {
    // 先驗證，參數錯誤時不開啟資料庫
    let change = triage::validate(request)?;
    let mut conn = open_working_db(&app.paths, &app.store)?;
    // CLI 一律記錄事件（即使狀態相同，也保留備註）；已有 Redmine 議題時同一交易排入註記
    let notify = &app.loaded.config.notify;
    triage_notes::set_status_many(&mut conn, notify, &[id], &change, chrono::Utc::now(), false)?;
    let detail = match (change.reason, change.justification) {
        (Some(r), Some(j)) => format!("（{}，{j}）", r.as_str()),
        (Some(r), None) => format!("（{}）", r.as_str()),
        _ => String::new(),
    };
    println!("命中 {id} 已設為 {}{detail}", change.status.label());
    Ok(ExitCode::Success)
}

fn conflicts(app: &App) -> Result<ExitCode, CoreError> {
    let conn = open_working_db(&app.paths, &app.store)?;
    finding_view::print_conflicts(&query::conflicts(&conn)?);
    Ok(ExitCode::Success)
}
