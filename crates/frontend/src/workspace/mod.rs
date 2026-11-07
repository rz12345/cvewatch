//! 前端共用狀態：資料庫連線、設定、外部效果、畫面資料、命中清單模型與背景 sync。
//!
//! 有副作用的操作是 `Workspace` 的方法，回傳 [`Report`]（或含詳情更新的 [`Refreshed`]），
//! 不直接寫入任何畫面欄位。寫入一律呼叫 core 與 CLI 相同的函式，確保事件紀錄一致。
//! 各操作依類別放在子模組：狀態變更（[`triage`]）、等待區（[`pending`]）、重新載入
//! （[`reload`]）、背景 sync 與新資料偵測（[`sync`]）、連結與剪貼簿（[`links`]）、VEX（[`vex`]）。

pub mod links;
pub mod pending;
pub mod reload;
pub mod sync;
pub mod triage;
pub mod vex;

use std::path::PathBuf;
use std::time::Instant;

use cvewatch_core::config::Config;
use cvewatch_core::matching::query::FindingRow;
use cvewatch_core::CoreError;
use rusqlite::Connection;

use crate::context::Context;
use crate::data::{Settings, Snapshot};
use crate::findings::{FindingsModel, ListView};
use crate::services::{RunningSync, Services};

pub use reload::{DetailRefresh, Refreshed};
pub use sync::NEW_DATA_POLL;
pub use triage::{change_detail, TriageRequest};

pub struct Workspace<V = ()> {
    pub conn: Connection,
    /// 寫入操作需要的設定（與 CLI 相同）
    pub settings: Settings,
    /// 目前的設定檔內容（設定畫面由此載入，儲存後更新）
    pub config: Config,
    /// 設定畫面寫回的設定檔
    pub config_path: PathBuf,
    pub services: Services,
    pub data: Snapshot,
    pub findings: FindingsModel<V>,
    /// 載入後有 sync 寫入新資料（提示使用者重新載入）
    pub new_data: bool,
    /// 本程序啟動、仍在執行的背景 sync
    pub sync: Option<Box<dyn RunningSync>>,
    /// 上次檢查新資料的時間
    pub last_poll: Option<Instant>,
}

impl<V: ListView> Workspace<V> {
    /// 載入畫面資料並計算可見列（預設篩選：只看未處理）。
    pub fn new(ctx: Context) -> Result<Self, CoreError> {
        let now = ctx.services.now();
        let data = Snapshot::load(&ctx.conn, ctx.settings.stale_days, now)?;
        let mut ws = Workspace {
            conn: ctx.conn,
            settings: ctx.settings,
            config: ctx.config,
            config_path: ctx.config_path,
            services: ctx.services,
            data,
            findings: FindingsModel::default(),
            new_data: false,
            sync: None,
            last_poll: None,
        };
        ws.refresh_visible();
        Ok(ws)
    }

    /// 篩選、搜尋或排序改變後重新計算可見列。
    pub fn refresh_visible(&mut self) {
        self.findings.refresh_visible(&self.data);
    }
}

impl<V> Workspace<V> {
    /// 目前可見的所選命中（依畫面順序）。
    pub fn visible_selection(&self) -> Vec<i64> {
        self.findings.visible_selection(&self.data)
    }

    /// 目前篩選（含搜尋）的命中 id，依畫面順序。
    pub fn visible_ids(&self) -> Vec<i64> {
        self.findings.visible_ids(&self.data)
    }

    /// 可見列第 `pos` 列的命中。
    pub fn row_at(&self, pos: usize) -> Option<&FindingRow> {
        self.findings.row_at(&self.data, pos)
    }
}
