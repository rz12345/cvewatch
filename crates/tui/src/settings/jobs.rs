//! 設定畫面的背景工作：測試寄信、測試 Redmine、載入 Redmine 清單、重新安裝排程。
//!
//! 連線一律在背景執行緒（[`Spawner`]），結果經 channel 回到 UI 執行緒，於 `on_tick` 取回，
//! 畫面不會被阻塞。實際動作以 [`NotifyBackend`] 抽象，測試以替身取代。

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError};

use cvewatch_core::config::Config;
use cvewatch_core::notify::check;
use cvewatch_core::notify::redmine::{Named, RedmineClient, RedmineSettings};
use zeroize::Zeroizing;

/// Redmine 清單（未載入者為 None）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedmineLists {
    pub projects: Option<Vec<Named>>,
    pub priorities: Option<Vec<Named>>,
    pub trackers: Option<Vec<Named>>,
    pub members: Option<Vec<Named>>,
}

impl RedmineLists {
    /// 合併新載入的清單（只覆蓋有值者）。
    pub fn merged(&self, more: RedmineLists) -> RedmineLists {
        RedmineLists {
            projects: more.projects.or_else(|| self.projects.clone()),
            priorities: more.priorities.or_else(|| self.priorities.clone()),
            trackers: more.trackers.or_else(|| self.trackers.clone()),
            members: more.members.or_else(|| self.members.clone()),
        }
    }
}

/// 實際的連線動作（在背景執行緒呼叫）。
pub trait NotifyBackend: Send + Sync {
    fn test_email(
        &self,
        config: &Config,
        password: Option<Zeroizing<String>>,
    ) -> Result<String, String>;
    fn test_redmine(
        &self,
        config: &Config,
        api_key: Option<Zeroizing<String>>,
    ) -> Result<String, String>;
    /// `project` 為 None 時載入專案與優先權；否則載入該專案的 tracker 與成員。
    fn redmine_lists(
        &self,
        config: &Config,
        api_key: Option<Zeroizing<String>>,
        project: Option<String>,
    ) -> Result<RedmineLists, String>;
    fn install_schedule(&self) -> Result<String, String>;
}

/// 執行背景工作。
pub trait Spawner {
    fn spawn(&self, job: Box<dyn FnOnce() + Send>);
}

/// 正式環境：每個工作一個執行緒。
pub struct ThreadSpawner;

impl Spawner for ThreadSpawner {
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) {
        std::thread::spawn(job);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    TestEmail,
    TestRedmine,
    LoadLists,
    LoadProject,
    InstallSchedule,
}

impl JobKind {
    pub fn label(self) -> &'static str {
        match self {
            JobKind::TestEmail => "測試寄信",
            JobKind::TestRedmine => "測試 Redmine",
            JobKind::LoadLists | JobKind::LoadProject => "載入 Redmine 清單",
            JobKind::InstallSchedule => "重新安裝排程",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobResult {
    Message(Result<String, String>),
    Lists(Result<RedmineLists, String>),
}

/// 執行中的工作。
pub struct Running {
    pub kind: JobKind,
    /// 載入專案清單時的專案（結果回來時專案已改變則捨棄）
    pub project: Option<String>,
    pub rx: Receiver<JobResult>,
}

impl Running {
    /// 已完成時取回結果；工作異常結束（沒有送出結果）時回報錯誤。
    pub fn poll(&self) -> Option<JobResult> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(JobResult::Message(Err(format!(
                "{}異常結束",
                self.kind.label()
            )))),
        }
    }
}

/// 實際連線：呼叫 core 的測試與 Redmine API；重新安裝排程以子程序執行 `cvewatch schedule install`。
pub struct LiveBackend {
    pub exe: PathBuf,
    pub config: Option<PathBuf>,
}

impl NotifyBackend for LiveBackend {
    fn test_email(
        &self,
        config: &Config,
        password: Option<Zeroizing<String>>,
    ) -> Result<String, String> {
        check::send_test_email(&config.notify, password, &config.http)
    }

    fn test_redmine(
        &self,
        config: &Config,
        api_key: Option<Zeroizing<String>>,
    ) -> Result<String, String> {
        check::test_redmine(&config.notify, api_key, &config.http)
            .map(|r| check::describe_report(&r))
    }

    fn redmine_lists(
        &self,
        config: &Config,
        api_key: Option<Zeroizing<String>>,
        project: Option<String>,
    ) -> Result<RedmineLists, String> {
        let n = &config.notify;
        let client = RedmineClient::new(&RedmineSettings {
            redmine: &n.redmine,
            api_key,
            ca_file: n.ca_file(),
            allow_insecure: n.allow_insecure_transport,
            http: &config.http,
        })?;
        let text = |e: cvewatch_core::notify::redmine::RedmineError| e.to_string();
        match project {
            None => Ok(RedmineLists {
                projects: Some(client.projects().map_err(text)?),
                priorities: Some(client.priorities().map_err(text)?),
                ..RedmineLists::default()
            }),
            Some(p) => {
                let (_, trackers) = client.project(&p).map_err(text)?;
                Ok(RedmineLists {
                    trackers: Some(trackers),
                    members: Some(client.members(&p).map_err(text)?),
                    ..RedmineLists::default()
                })
            }
        }
    }

    fn install_schedule(&self) -> Result<String, String> {
        let mut cmd = Command::new(&self.exe);
        if let Some(c) = &self.config {
            cmd.arg("--config").arg(c);
        }
        cmd.args(["schedule", "install"]).stdin(Stdio::null());
        cvewatch_core::process::hide_window(&mut cmd);
        let out = cmd
            .output()
            .map_err(|e| format!("無法執行 schedule install：{e}"))?;
        let text = |b: &[u8]| String::from_utf8_lossy(b).trim().to_string();
        if out.status.success() {
            Ok(text(&out.stdout)
                .lines()
                .last()
                .unwrap_or("排程已重新安裝")
                .to_string())
        } else {
            Err(text(&out.stderr))
        }
    }
}
