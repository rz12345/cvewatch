//! 終端機初始化／還原與事件迴圈。`ratatui::try_init` 會安裝 panic hook，panic 時也先還原終端機。

use std::time::{Duration, Instant};

use cvewatch_core::CoreError;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::ui;

/// 等待按鍵的逾時；逾時後檢查背景 sync 與新資料。
const POLL_INTERVAL: Duration = Duration::from_millis(250);

fn term_error(e: std::io::Error) -> CoreError {
    CoreError::Invalid(format!("終端機錯誤：{e}"))
}

pub fn run(app: App) -> Result<(), CoreError> {
    let terminal = ratatui::try_init().map_err(term_error)?;
    let result = event_loop(terminal, app);
    // 不論成功或失敗都還原。兩者都失敗時回報事件迴圈的錯誤（原因），還原錯誤寫入日誌。
    let restored = ratatui::try_restore();
    if let Err(e) = &restored {
        tracing::error!(error = %e, "還原終端機失敗");
    }
    result.and(restored.map_err(term_error))
}

/// 只在按鍵、視窗大小改變或背景狀態改變時重繪，閒置時不重建畫面。
fn event_loop(mut terminal: DefaultTerminal, mut app: App) -> Result<(), CoreError> {
    let mut dirty = true;
    while !app.quit {
        if dirty {
            terminal
                .draw(|frame| ui::draw(frame, &app))
                .map_err(term_error)?;
            dirty = false;
        }
        if event::poll(POLL_INTERVAL).map_err(term_error)? {
            match event::read().map_err(term_error)? {
                // Windows 會同時送出按下與放開事件，只處理按下
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.on_key(key);
                    dirty = true;
                }
                Event::Resize(..) => dirty = true,
                _ => {}
            }
        }
        dirty |= app.on_tick(Instant::now());
    }
    Ok(())
}
