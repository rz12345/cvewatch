//! 設定畫面的共用部分：表單模型與即時驗證（[`form`]）、寫回設定檔與憑證庫（[`save`]）、
//! 背景連線工作（[`jobs`]）。按鍵與輸入框屬於各前端。

pub mod form;
pub mod jobs;
pub mod save;
pub mod state;

pub use form::{FieldId, Form, Kind, Row, Secret, Tab};
pub use jobs::{
    JobKind, JobResult, LiveBackend, NotifyBackend, RedmineLists, Running, Spawner, ThreadSpawner,
};
pub use state::{ActionWords, Finished, Refused, Saved, SettingsState};
