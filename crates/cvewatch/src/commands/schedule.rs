//! `cvewatch schedule install | status | uninstall`

use cvewatch_core::schedule::xml::{task_path, InstallContext};
use cvewatch_core::{CoreError, ExitCode};

use crate::app::App;
use crate::cli::ScheduleAction;
use crate::schedule::{self, result_text, SystemRunner};

pub fn run(app: &App, action: ScheduleAction) -> Result<ExitCode, CoreError> {
    let runner = SystemRunner;
    match action {
        ScheduleAction::Install => install(app, &runner),
        ScheduleAction::Status => status(&runner),
        ScheduleAction::Uninstall => uninstall(&runner),
    }
}

fn install(app: &App, runner: &SystemRunner) -> Result<ExitCode, CoreError> {
    let exe = std::env::current_exe()
        .map_err(|e| CoreError::Invalid(format!("無法取得執行檔路徑：{e}")))?;
    let ctx = InstallContext {
        user_id: schedule::current_user()?,
        wrapper: schedule::wrapper_path(&exe)?,
        // 明確指定的設定檔傳給排程；環境變數 CVEWATCH_CONFIG 在排程中不一定存在，一併寫入。
        // 排程的工作目錄是執行檔所在資料夾，相對路徑先轉為絕對路徑
        config: app
            .location()
            .is_explicit()
            .then(|| schedule::absolute(&app.location().path))
            .transpose()?,
        today: chrono::Local::now().date_naive(),
    };
    let cfg = &app.loaded.config.schedule;
    let specs = schedule::install(runner, cfg, &ctx, &std::env::temp_dir())?;
    for spec in &specs {
        println!("已建立 {}：{}", task_path(&spec.name), spec.description);
    }
    println!(
        "以 {} 登入時執行（不需儲存密碼）；錯過時間會在開機後補跑，只在有網路時執行。",
        ctx.user_id
    );
    Ok(ExitCode::Success)
}

fn status(runner: &SystemRunner) -> Result<ExitCode, CoreError> {
    let tasks = schedule::status(runner)?;
    if tasks.iter().all(|t| t.info.is_none()) {
        println!("尚未安裝排程。請執行 `cvewatch schedule install`。");
        return Ok(ExitCode::Success);
    }
    for t in tasks {
        match t.info {
            None => println!(
                "{}：尚未安裝（執行 `cvewatch schedule install`）",
                task_path(&t.name)
            ),
            Some(info) => println!(
                "{}：上次 {}，結果 {}；下次 {}",
                task_path(&t.name),
                info.last_run.as_deref().unwrap_or("尚未執行"),
                info.last_result.map_or("-".into(), result_text),
                info.next_run.as_deref().unwrap_or("-")
            ),
        }
    }
    Ok(ExitCode::Success)
}

fn uninstall(runner: &SystemRunner) -> Result<ExitCode, CoreError> {
    let removed = schedule::uninstall(runner)?;
    if removed.is_empty() {
        println!("沒有已安裝的排程工作。");
    }
    for name in removed {
        println!("已移除 {}", task_path(&name));
    }
    Ok(ExitCode::Success)
}
