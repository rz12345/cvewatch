//! `cvewatch tui`：開啟文字介面（實作在 `cvewatch-tui`，啟動與資料在 `cvewatch-frontend`）。

use cvewatch_core::{CoreError, ExitCode};
use cvewatch_frontend::context::{self, ContextOptions};
use cvewatch_tui::{glyphs, Context};

use crate::app::App;

/// `app`（執行環境）持有日誌寫入器，由 `main` 保留到 TUI 結束後才 drop。
pub fn run(app: &App) -> Result<ExitCode, CoreError> {
    let exe = std::env::current_exe()
        .map_err(|e| CoreError::Invalid(format!("無法取得執行檔路徑：{e}")))?;
    // 背景 sync 與重新安裝排程以本執行檔執行；明確指定的設定檔一併傳入（環境變數會自動繼承）
    let base = context::build(app, ContextOptions::for_runtime(app, exe)?)?;
    cvewatch_tui::run(Context {
        base,
        glyphs: glyphs::detect(),
    })?;
    Ok(ExitCode::Success)
}
