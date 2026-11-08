//! 由執行環境（[`Runtime`]）建立前端 `Context`：產品目錄、開啟加密資料庫、系統外部效果。
//! TUI 與 GUI 共用；`Runtime` 持有日誌寫入器，由呼叫端保留到前端結束。

use std::path::PathBuf;
use std::sync::Arc;

use cvewatch_core::config::Config;
use cvewatch_core::db::open_working_db;
use cvewatch_core::runtime::Runtime;
use cvewatch_core::secrets::KeyringStore;
use cvewatch_core::CoreError;
use rusqlite::Connection;

use crate::data::Settings;
use crate::services::{
    CliTarget, DisplayTz, LinkPolicy, ProcessLauncher, Services, SystemClipboard, SystemOpener,
};
use crate::settings::{LiveBackend, ThreadSpawner};

/// 開啟前端所需的一切。
pub struct Context {
    pub conn: Connection,
    pub settings: Settings,
    /// 目前的設定檔內容
    pub config: Config,
    /// 設定畫面寫回的設定檔
    pub config_path: PathBuf,
    pub services: Services,
}

/// 由呼叫端決定的啟動參數。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextOptions {
    /// 背景 sync 與重新安裝排程執行的 CLI 執行檔與傳入的設定檔
    pub cli: CliTarget,
}

impl ContextOptions {
    /// 以 `cli_exe` 執行子程序，並傳入使用者明確指定的設定檔（[`Runtime::explicit_config`]）。
    pub fn for_runtime(runtime: &Runtime, cli_exe: PathBuf) -> Result<ContextOptions, CoreError> {
        Ok(ContextOptions {
            cli: CliTarget {
                exe: cli_exe,
                config: runtime.explicit_config()?,
            },
        })
    }
}

pub fn build(runtime: &Runtime, options: ContextOptions) -> Result<Context, CoreError> {
    let catalog = runtime.load_catalog()?;
    let conn = open_working_db(&runtime.paths, &runtime.store)?;
    let config = &runtime.loaded.config;
    Ok(Context {
        conn,
        settings: Settings {
            catalog,
            classes: config.asset_classes()?,
            scoring: config.scoring.clone(),
            stale_days: config.pending.stale_days,
        },
        config: config.clone(),
        config_path: runtime.location().path.clone(),
        services: system_services(&options, runtime.store.clone()),
    })
}

/// 正式環境的外部效果：系統瀏覽器與剪貼簿、CLI 子程序、Credential Manager（與開啟資料庫
/// 使用同一個 `store`）、本機時區。
pub fn system_services(options: &ContextOptions, store: KeyringStore) -> Services {
    let (launcher, backend) = subprocesses(options);
    Services {
        opener: Box::new(SystemOpener),
        clipboard: Box::new(SystemClipboard),
        launcher: Box::new(launcher),
        clock: Box::new(chrono::Utc::now),
        tz: DisplayTz::Local,
        secrets: Box::new(store),
        backend: Arc::new(backend),
        spawner: Box::new(ThreadSpawner),
        documents_dir: cvewatch_core::paths::documents_dir(),
        link_policy: LinkPolicy::Web,
    }
}

/// 背景 sync 與重新安裝排程：同一個 CLI 目標。
fn subprocesses(options: &ContextOptions) -> (ProcessLauncher, LiveBackend) {
    (
        ProcessLauncher(options.cli.clone()),
        LiveBackend(options.cli.clone()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(cmd: &std::process::Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn options_reach_sync_launcher_and_schedule_backend() {
        let exe = r"C:\Program Files\cvewatch\cvewatch.exe";
        let options = ContextOptions {
            cli: CliTarget {
                exe: PathBuf::from(exe),
                config: Some(PathBuf::from(r"D:\cfg\cvewatch.toml")),
            },
        };
        let (launcher, backend) = subprocesses(&options);
        let sync = launcher.command();
        assert_eq!(sync.get_program(), exe);
        assert_eq!(args(&sync), ["--config", r"D:\cfg\cvewatch.toml", "sync"]);
        let install = backend.0.command(&["schedule", "install"]);
        assert_eq!(install.get_program(), exe);
        assert_eq!(
            args(&install),
            ["--config", r"D:\cfg\cvewatch.toml", "schedule", "install"]
        );
        let implicit = ContextOptions {
            cli: CliTarget {
                config: None,
                ..options.cli
            },
        };
        let (launcher, backend) = subprocesses(&implicit);
        assert_eq!(args(&launcher.command()), ["sync"]);
        assert_eq!(
            args(&backend.0.command(&["schedule", "install"])),
            ["schedule", "install"]
        );
    }
}
