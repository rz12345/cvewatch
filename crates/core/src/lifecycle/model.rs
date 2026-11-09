//! 生命週期項目的型別與共用文字（22 design D9）：GUI、TUI、CLI 與摘要以相同用語呈現。

use chrono::NaiveDate;

use crate::assets::dates::format_date;

/// 項目種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ItemKind {
    Hardware,
    Software { component_id: i64 },
    Warranty,
    Support,
    License,
}

impl ItemKind {
    /// 硬體或軟體 EoL（其他為保固、合約、授權到期）。
    pub fn is_eol(self) -> bool {
        matches!(self, ItemKind::Hardware | ItemKind::Software { .. })
    }
}

/// 日期的來源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// 手動覆寫的來源說明
    Override(String),
    /// 目錄宣告的來源網址（版本分支或硬體型號）
    Catalog(String),
    /// endoflife.date 的版本分支；`link` 為產品頁
    EndOfLifeDate {
        slug: String,
        cycle: String,
        link: Option<String>,
    },
    /// 資產欄位（保固、合約、授權到期日）
    AssetField,
}

impl Source {
    /// 「延長支援合約（手動覆寫）」「https://…」「endoflife.date」；資產欄位為 None。
    pub fn text(&self) -> Option<String> {
        match self {
            Source::Override(s) => Some(format!("{s}（手動覆寫）")),
            Source::Catalog(url) => Some(url.clone()),
            Source::EndOfLifeDate { .. } => Some("endoflife.date".to_string()),
            Source::AssetField => None,
        }
    }

    /// 可開啟的網址（目錄網址、endoflife.date 產品頁；覆寫為網址時亦可）。
    pub fn url(&self) -> Option<&str> {
        let is_url = |s: &str| crate::catalog::lifecycle::is_http_url(s);
        match self {
            Source::Catalog(url) => Some(url.as_str()),
            Source::EndOfLifeDate { link, .. } => link.as_deref(),
            Source::Override(s) if is_url(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// 沒有日期的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingReason {
    NotFilled,
    NoBrandOrModel,
    ModelNotInCatalog,
    Unmapped { version: String },
    PendingVersion,
    NoEolData,
    SlugNotFound,
    NotSynced,
}

impl MissingReason {
    pub fn text(&self) -> String {
        match self {
            MissingReason::NotFilled => "未填".into(),
            MissingReason::NoBrandOrModel => "未填廠牌或型號".into(),
            MissingReason::ModelNotInCatalog => "目錄沒有此型號".into(),
            MissingReason::Unmapped { version } => format!("無法對應版本分支（{version}）"),
            MissingReason::PendingVersion => "版本待補，無法判斷 EoL".into(),
            MissingReason::NoEolData => "無 EoL 資料".into(),
            MissingReason::SlugNotFound => "endoflife.date 查無此產品".into(),
            MissingReason::NotSynced => {
                "尚未取得 EoL 資料（執行 cvewatch sync --source eol）".into()
            }
        }
    }
}

/// 項目的判定結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemState {
    Dated {
        date: NaiveDate,
        source: Source,
    },
    /// endoflife.date 標示已 EoL、日期未公布
    EolUndated {
        source: Source,
    },
    /// 分支沒有 EoL 日期且未標示 EoL
    NotAnnounced {
        cycle: String,
    },
    Missing(MissingReason),
}

/// 依今天與提醒天數的狀態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 已到（含當天）；`days_ago` 為 None 表示日期未公布
    Past {
        days_ago: Option<i64>,
    },
    Soon {
        days_left: i64,
    },
    Later {
        days_left: i64,
    },
    None,
}

impl Status {
    pub fn is_past(self) -> bool {
        matches!(self, Status::Past { .. })
    }

    pub fn is_soon(self) -> bool {
        matches!(self, Status::Soon { .. })
    }
}

/// 一個生命週期項目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleItem {
    pub kind: ItemKind,
    /// 「硬體 EoL」「保固到期」或產品顯示名稱
    pub label: String,
    /// 軟體：元件版本；硬體：對應到的目錄型號
    pub detail: Option<String>,
    /// 對應到的版本分支（軟體）
    pub cycle: Option<String>,
    pub state: ItemState,
    pub status: Status,
}

impl LifecycleItem {
    pub fn date(&self) -> Option<NaiveDate> {
        match &self.state {
            ItemState::Dated { date, .. } => Some(*date),
            _ => None,
        }
    }

    pub fn source(&self) -> Option<&Source> {
        match &self.state {
            ItemState::Dated { source, .. } | ItemState::EolUndated { source } => Some(source),
            _ => None,
        }
    }

    /// 「PAN-OS 10.2.9（分支 10.2）」「硬體 EoL（Aruba XYZ-100）」「保固到期」。
    pub fn subject(&self) -> String {
        let mut s = self.label.clone();
        if let Some(d) = &self.detail {
            if matches!(self.kind, ItemKind::Software { .. }) {
                s.push(' ');
                s.push_str(d);
            } else {
                s.push_str(&format!("（{d}）"));
            }
        }
        if let Some(c) = &self.cycle {
            s.push_str(&format!("（分支 {c}）"));
        }
        s
    }

    /// 狀態字樣：「已 EoL」「即將 EoL」「已到期」「即將到期」；其他為 None。
    pub fn status_word(&self) -> Option<&'static str> {
        match (self.kind.is_eol(), self.status) {
            (true, Status::Past { .. }) => Some("已 EoL"),
            (true, Status::Soon { .. }) => Some("即將 EoL"),
            (false, Status::Past { .. }) => Some("已到期"),
            (false, Status::Soon { .. }) => Some("即將到期"),
            _ => None,
        }
    }

    /// 「即將 EoL（剩 50 天）」「已到期（10 天前）」「剩 400 天」；沒有日期時為 None。
    pub fn status_text(&self) -> Option<String> {
        let days = match self.status {
            Status::Past { days_ago: Some(0) } => Some("今天".to_string()),
            Status::Past { days_ago: Some(n) } => Some(format!("{n} 天前")),
            Status::Past { days_ago: None } => Some("日期未公布".to_string()),
            Status::Soon { days_left } | Status::Later { days_left } => {
                Some(format!("剩 {days_left} 天"))
            }
            Status::None => None,
        }?;
        Some(match self.status_word() {
            Some(word) => format!("{word}（{days}）"),
            None => days,
        })
    }

    /// 一行說明（CLI、TUI 與摘要共用）：
    /// 「PAN-OS 10.2.9（分支 10.2）EoL 2027-03-01 即將 EoL（剩 50 天），來源 endoflife.date」、
    /// 「保固到期 2026-12-31 已到期（10 天前）」、「保固到期 未填」。
    pub fn line(&self) -> String {
        let subject = self.subject();
        let eol_word = if self.kind.is_eol() && !matches!(self.kind, ItemKind::Hardware) {
            "EoL "
        } else {
            ""
        };
        // 全形括號結尾時不加空白：「PAN-OS 10.2.9（分支 10.2）EoL 2027-03-01」
        let sep = if subject.ends_with('）') { "" } else { " " };
        let mut out = match &self.state {
            ItemState::Dated { date, .. } => {
                format!("{subject}{sep}{eol_word}{}", format_date(*date))
            }
            ItemState::EolUndated { .. } => subject,
            ItemState::NotAnnounced { .. } => format!("{subject}{sep}未公布 EoL"),
            ItemState::Missing(reason) => format!("{subject}{sep}{}", reason.text()),
        };
        if let Some(status) = self.status_text() {
            out.push(' ');
            out.push_str(&status);
        }
        if let Some(src) = self.source().and_then(Source::text) {
            out.push_str("，來源 ");
            out.push_str(&src);
        }
        out
    }
}

/// 一個資產的全部生命週期項目（硬體、各清冊元件、保固、合約、授權）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetLifecycle {
    pub items: Vec<LifecycleItem>,
}

/// 資產的生命週期旗標（清單標示、篩選與計數）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LifecycleFlags {
    pub eol: bool,
    pub eol_soon: bool,
    pub expired: bool,
    pub expiring: bool,
}

impl LifecycleFlags {
    /// 「保固或合約到期」篩選（含即將到期與已到期）。
    pub fn expiry(self) -> bool {
        self.expired || self.expiring
    }

    pub fn any(self) -> bool {
        self.eol || self.eol_soon || self.expiry()
    }
}

impl AssetLifecycle {
    pub fn flags(&self) -> LifecycleFlags {
        let mut f = LifecycleFlags::default();
        for item in &self.items {
            match (item.kind.is_eol(), item.status) {
                (true, Status::Past { .. }) => f.eol = true,
                (true, Status::Soon { .. }) => f.eol_soon = true,
                (false, Status::Past { .. }) => f.expired = true,
                (false, Status::Soon { .. }) => f.expiring = true,
                _ => {}
            }
        }
        f
    }
}

/// 生命週期篩選（GUI 下拉選單、TUI `l` 鍵）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleFilter {
    Eol,
    EolSoon,
    /// 保固、維護合約或軟體授權即將到期或已到期
    Expiry,
}

impl LifecycleFilter {
    pub const ALL: [LifecycleFilter; 3] = [
        LifecycleFilter::Eol,
        LifecycleFilter::EolSoon,
        LifecycleFilter::Expiry,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LifecycleFilter::Eol => "已 EoL",
            LifecycleFilter::EolSoon => "即將 EoL",
            LifecycleFilter::Expiry => "保固或合約到期",
        }
    }

    pub fn matches(self, flags: LifecycleFlags) -> bool {
        match self {
            LifecycleFilter::Eol => flags.eol,
            LifecycleFilter::EolSoon => flags.eol_soon,
            LifecycleFilter::Expiry => flags.expiry(),
        }
    }
}
