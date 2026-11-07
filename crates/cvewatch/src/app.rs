//! 執行環境：設定位置與啟動（實作在 `cvewatch_core::runtime`，TUI／GUI 共用）。

use cvewatch_core::paths::{
    default_config_path, resolve_config_location, ConfigLocation, CONFIG_ENV_VAR,
};
use cvewatch_core::runtime;
use cvewatch_core::CoreError;

pub use cvewatch_core::runtime::Runtime as App;

pub fn resolve_location(cli_path: Option<std::path::PathBuf>) -> Result<ConfigLocation, CoreError> {
    Ok(resolve_config_location(
        cli_path,
        std::env::var_os(CONFIG_ENV_VAR),
        default_config_path(),
    )?)
}

/// 啟動執行環境，並把啟動警告（未知欄位、日誌無法寫入）逐行印到 stderr。
pub fn start(location: &ConfigLocation, command: &str) -> Result<App, CoreError> {
    let app = runtime::start(location, command)?;
    for warning in &app.warnings {
        eprintln!("警告：{warning}");
    }
    Ok(app)
}
