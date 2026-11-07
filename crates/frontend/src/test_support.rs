//! 測試替身（`test-support` feature）：瀏覽器、剪貼簿、背景 sync、通知後端、背景工作、
//! 憑證庫與固定時鐘。TUI 與 GUI 的測試共用，只在 dev-dependencies 啟用。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, FixedOffset, Utc};
use cvewatch_core::config::Config;
use cvewatch_core::notify::redmine::Named;
use cvewatch_core::secrets::{MemoryStore, SecretError, SecretKey, SecretStore};
use zeroize::Zeroizing;

use crate::services::{ClipboardSink, DisplayTz, Opener, RunningSync, Services, SyncLauncher};
use crate::settings::{NotifyBackend, RedmineLists, Spawner};

/// 設定畫面的連線替身：記錄呼叫（含使用的設定值）並回傳預設結果。
#[derive(Default)]
pub struct FakeBackend {
    pub calls: Mutex<Vec<String>>,
    pub email: Mutex<Option<Result<String, String>>>,
    pub redmine: Mutex<Option<Result<String, String>>>,
    pub lists: Mutex<Option<Result<RedmineLists, String>>>,
    pub project_lists: Mutex<Option<Result<RedmineLists, String>>>,
    /// 依專案回傳不同的 tracker／成員清單（優先於 `project_lists`）
    pub lists_by_project: Mutex<HashMap<String, RedmineLists>>,
}

impl FakeBackend {
    fn log(&self, s: String) {
        self.calls.lock().expect("calls").push(s);
    }
}

impl NotifyBackend for FakeBackend {
    fn test_email(
        &self,
        c: &Config,
        password: Option<Zeroizing<String>>,
    ) -> Result<String, String> {
        let e = &c.notify.email;
        self.log(format!(
            "test_email {}:{} {} password={}",
            e.smtp_host,
            e.smtp_port,
            e.security,
            password.is_some()
        ));
        self.email
            .lock()
            .expect("email")
            .clone()
            .unwrap_or(Ok("已寄出測試信".into()))
    }

    fn test_redmine(&self, c: &Config, key: Option<Zeroizing<String>>) -> Result<String, String> {
        self.log(format!(
            "test_redmine {} key={}",
            c.notify.redmine.base_url,
            key.is_some()
        ));
        self.redmine
            .lock()
            .expect("redmine")
            .clone()
            .unwrap_or(Ok("Redmine 連線正常".into()))
    }

    fn redmine_lists(
        &self,
        _c: &Config,
        _key: Option<Zeroizing<String>>,
        project: Option<String>,
    ) -> Result<RedmineLists, String> {
        self.log(format!("lists {project:?}"));
        if let Some(l) = project
            .as_ref()
            .and_then(|p| self.lists_by_project.lock().expect("by").get(p).cloned())
        {
            return Ok(l);
        }
        let slot = if project.is_some() {
            &self.project_lists
        } else {
            &self.lists
        };
        slot.lock()
            .expect("lists")
            .clone()
            .unwrap_or(Ok(RedmineLists::default()))
    }

    fn install_schedule(&self) -> Result<String, String> {
        self.log("install_schedule".into());
        Ok("排程已重新安裝".into())
    }
}

/// 立即執行（測試中背景工作同步完成，下一次輪詢即可取回結果）。
pub struct InlineSpawner;

impl Spawner for InlineSpawner {
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) {
        job();
    }
}

/// 共用的記憶體憑證庫；`fails` 為真時模擬 Credential Manager 無法存取。
pub struct SharedStore(pub Arc<MemoryStore>, pub Arc<AtomicBool>);

impl SharedStore {
    fn check(&self, key: SecretKey) -> Result<(), SecretError> {
        if self.1.load(Ordering::SeqCst) {
            return Err(SecretError::Backend {
                key,
                reason: "無法存取憑證庫".into(),
            });
        }
        Ok(())
    }
}

impl SecretStore for SharedStore {
    fn get(&self, key: SecretKey) -> Result<Option<Zeroizing<String>>, SecretError> {
        self.check(key)?;
        self.0.get(key)
    }
    fn set(&self, key: SecretKey, value: &str) -> Result<(), SecretError> {
        self.check(key)?;
        self.0.set(key, value)
    }
    fn delete(&self, key: SecretKey) -> Result<(), SecretError> {
        self.check(key)?;
        self.0.delete(key)
    }
}

pub fn named(id: u64, name: &str) -> Named {
    Named {
        id,
        name: name.into(),
    }
}

/// 固定時鐘。
pub fn fixed_clock(now: DateTime<Utc>) -> Box<dyn Fn() -> DateTime<Utc>> {
    Box::new(move || now)
}

/// 外部效果的紀錄與替身設定。
#[derive(Clone)]
pub struct Recorder {
    /// 設定檔所在的臨時目錄
    pub dir: Rc<tempfile::TempDir>,
    pub secrets: Arc<MemoryStore>,
    /// 模擬憑證庫無法存取
    pub store_fails: Arc<AtomicBool>,
    pub backend: Arc<FakeBackend>,
    pub opened: Rc<RefCell<Vec<String>>>,
    pub copied: Rc<RefCell<Vec<String>>>,
    pub launches: Rc<Cell<usize>>,
    /// 背景 sync 的結束狀態；None 表示仍在執行
    pub exit: Rc<RefCell<Option<Option<i32>>>>,
    pub clipboard_fails: Rc<Cell<bool>>,
}

struct FakeOpener(Recorder);

impl Opener for FakeOpener {
    fn open(&self, url: &str) -> Result<(), String> {
        self.0.opened.borrow_mut().push(url.to_string());
        Ok(())
    }
}

struct FakeClipboard(Recorder);

impl ClipboardSink for FakeClipboard {
    fn copy(&self, text: &str) -> Result<(), String> {
        if self.0.clipboard_fails.get() {
            return Err("剪貼簿無法使用".into());
        }
        self.0.copied.borrow_mut().push(text.to_string());
        Ok(())
    }
}

struct FakeLauncher(Recorder);
struct FakeChild(Recorder);

impl SyncLauncher for FakeLauncher {
    fn launch(&self) -> Result<Box<dyn RunningSync>, String> {
        self.0.launches.set(self.0.launches.get() + 1);
        Ok(Box::new(FakeChild(self.0.clone())))
    }
}

impl RunningSync for FakeChild {
    fn try_wait(&mut self) -> Result<Option<Option<i32>>, String> {
        Ok(*self.0.exit.borrow())
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder {
            dir: Rc::new(tempfile::tempdir().expect("tempdir")),
            secrets: Arc::new(MemoryStore::new()),
            store_fails: Arc::default(),
            backend: Arc::new(FakeBackend::default()),
            opened: Rc::default(),
            copied: Rc::default(),
            launches: Rc::default(),
            exit: Rc::default(),
            clipboard_fails: Rc::default(),
        }
    }
}

impl Recorder {
    pub fn documents_dir(&self) -> PathBuf {
        self.dir.path().join("Documents")
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.path().join("cvewatch.toml")
    }

    pub fn config_text(&self) -> String {
        std::fs::read_to_string(self.config_path()).expect("config")
    }
}

/// 全部以替身組成的外部效果（UTC 顯示時區、固定時鐘）。
pub fn services(rec: &Recorder, now: DateTime<Utc>) -> Services {
    Services {
        opener: Box::new(FakeOpener(rec.clone())),
        clipboard: Box::new(FakeClipboard(rec.clone())),
        launcher: Box::new(FakeLauncher(rec.clone())),
        clock: fixed_clock(now),
        tz: DisplayTz::Fixed(FixedOffset::east_opt(0).expect("utc")),
        secrets: Box::new(SharedStore(rec.secrets.clone(), rec.store_fails.clone())),
        backend: rec.backend.clone(),
        spawner: Box::new(InlineSpawner),
        documents_dir: rec.documents_dir(),
    }
}
