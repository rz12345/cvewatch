//! 命令列定義（clap derive）。

use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "cvewatch",
    version,
    about = "資產 × CVE 比對工具",
    long_about = "資產 × CVE 比對工具：匯入資產清冊、同步 CVE、規則比對、風險排序與通知。\n\n\
                  結束碼：0 成功、1 一般錯誤、2 設定錯誤、3 另一個 sync 正在執行、\
                  4 部分失敗（部分來源失敗，或資料已寫入但自動比對失敗）、5 資料庫金鑰遺失或錯誤。"
)]
pub struct Cli {
    /// 設定檔路徑（優先於環境變數 CVEWATCH_CONFIG）
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// 建立資料目錄、範例設定檔、加密資料庫與金鑰（可重複執行，不覆蓋既有檔案）
    Init,
    /// 同步 CVE 資料來源（cvelistV5、NVD、原廠 CSAF、KEV、EPSS、OSV）
    Sync(SyncArgs),
    /// 查詢本機 CVE 紀錄
    Cve {
        #[command(subcommand)]
        action: CveAction,
    },
    /// 重新比對所有使用中的資產元件與本機 CVE（sync 與資產匯入後會自動執行）
    Match,
    /// 比對命中：列出、檢視證據與時間軸、變更處理狀態、來源矛盾
    Finding {
        #[command(subcommand)]
        action: FindingAction,
    },
    /// 等待區：資料不足以判定的 CVE
    Pending {
        #[command(subcommand)]
        action: PendingAction,
    },
    /// 開啟文字介面（尚未實作）
    Tui,
    /// 資產清冊：匯入 Excel/CSV 與套件清單、檢視資產與產品目錄
    Asset {
        #[command(subcommand)]
        action: AssetAction,
    },
    /// 設定與秘密
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// 資料庫金鑰、備份與還原
    Db {
        #[command(subcommand)]
        action: DbAction,
    },
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Command::Init => "init",
            Command::Sync(_) => "sync",
            Command::Cve { .. } => "cve",
            Command::Match => "match",
            Command::Finding { .. } => "finding",
            Command::Pending { .. } => "pending",
            Command::Tui => "tui",
            Command::Asset { .. } => "asset",
            Command::Config { .. } => "config",
            Command::Db { .. } => "db",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// 將秘密存入 Windows Credential Manager（輸入不回顯）
    SetSecret {
        /// 秘密鍵
        #[arg(value_parser = ["nvd_api_key", "smtp_password", "redmine_api_key"])]
        key: String,
    },
    /// 檢查設定、秘密是否已設定、資料庫與復原金鑰狀態
    Check,
}

#[derive(Debug, Subcommand)]
pub enum DbAction {
    /// 顯示資料庫復原金鑰（需輸入 YES 確認；不寫入任何檔案）
    ExportKey,
    /// 輸入復原金鑰並寫回憑證庫（先驗證能開啟資料庫）
    ImportKey,
    /// 以新金鑰重新加密資料庫（舊復原金鑰隨即失效）
    Rekey,
    /// 產生加密備份（預設沿用資料庫金鑰）
    Backup {
        /// 備份檔路徑（不可已存在）
        path: PathBuf,
        /// 改以口令加密，可在其他電腦還原
        #[arg(long, conflicts_with = "plain")]
        passphrase: bool,
        /// 產生未加密備份（需輸入 PLAIN 確認）
        #[arg(long)]
        plain: bool,
    },
    /// 從備份還原（原資料庫保留為 cvewatch.db.before-restore）
    Restore {
        /// 備份檔路徑
        file: PathBuf,
        /// 備份以口令加密
        #[arg(long, conflicts_with = "recovery_key")]
        passphrase: bool,
        /// 備份以舊的資料庫金鑰加密（rekey 之前或其他電腦）：輸入當時的復原金鑰
        #[arg(long)]
        recovery_key: bool,
    },
}

const CATEGORIES: [&str; 6] = [
    "network",
    "security",
    "virtualization",
    "server",
    "storage",
    "host",
];

#[derive(Debug, Subcommand)]
pub enum AssetAction {
    /// 匯入 Excel（assets / components 工作表）或 CSV 清冊；以 asset_tag 新增或更新
    Import {
        /// 清冊檔（.xlsx 或 .csv）
        file: PathBuf,
        /// CSV 匯入時的元件檔（欄位 asset_tag、product、version）
        #[arg(long, value_name = "CSV")]
        components: Option<PathBuf>,
        /// 只顯示差異，不寫入資料庫
        #[arg(long)]
        dry_run: bool,
        /// 將清冊中消失的資產標為退役
        #[arg(long)]
        prune: bool,
    },
    /// 匯入主機套件清單（CycloneDX / SPDX JSON、dpkg-query -W、rpm -qa），整批取代該資產的套件
    ImportPackages {
        /// 資產編號（須已存在且為 active）
        asset_tag: String,
        /// 套件清單檔
        file: PathBuf,
        /// 檔案格式
        #[arg(long, value_enum, default_value_t = PackageFormatArg::Auto)]
        format: PackageFormatArg,
        /// 發行版（dpkg / rpm 必填），例如 debian-12、ubuntu-22.04、rhel-9
        #[arg(long)]
        distro: Option<String>,
    },
    /// 列出資產
    List {
        /// 只列出指定類別
        #[arg(long, value_parser = CATEGORIES)]
        category: Option<String>,
        /// 只列出含未辨識產品的資產
        #[arg(long)]
        unresolved: bool,
    },
    /// 顯示單一資產的元件與版本歷史
    Show {
        /// 資產編號
        asset_tag: String,
    },
    /// 匯出空白 Excel 範本（assets、components 工作表）
    Template {
        /// 輸出路徑（.xlsx，不可已存在）
        path: PathBuf,
    },
    /// 產品目錄（內建種子 + <data_dir>\catalog.d\*.toml）
    Catalog {
        /// 列出清冊中無法辨識的產品寫法與出現的資產
        #[arg(long)]
        unresolved: bool,
        #[command(subcommand)]
        action: Option<CatalogAction>,
    },
}

#[derive(Debug, Subcommand)]
pub enum CatalogAction {
    /// 列出所有產品
    List,
    /// 顯示產品的別名、CPE、CNA、CSAF 識別
    Show {
        /// 產品 key，例如 paloalto-panos
        key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PackageFormatArg {
    Auto,
    Cyclonedx,
    Spdx,
    Dpkg,
    Rpm,
}

#[derive(Debug, Args)]
#[command(
    long_about = "同步 CVE 資料來源。順序：cvelist → nvd → csaf:* → kev → epss → osv；                  每個來源各自交易，任一來源失敗時其他來源照常更新，最後以結束碼 4 結束。

                  首次同步會下載 cvelistV5 baseline（約 650 MB），需數分鐘到十數分鐘。"
)]
pub struct SyncArgs {
    /// 只執行指定來源（可重複）：cvelist、nvd、csaf、csaf:<id>、kev、epss、osv
    #[arg(long, value_name = "NAME")]
    pub source: Vec<String>,
    /// 以最新 baseline 依目前產品目錄重新篩選（目錄變更後使用）
    #[arg(long)]
    pub rescan: bool,
    /// 覆寫游標起點（UTC 日期，YYYY-MM-DD）
    #[arg(long, value_name = "DATE", value_parser = parse_date)]
    pub since: Option<NaiveDate>,
    /// 列出最近 20 次各來源的執行結果（不執行同步）
    #[arg(long, conflicts_with_all = ["source", "rescan", "since"])]
    pub history: bool,
}

fn parse_date(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("日期格式應為 YYYY-MM-DD：{value}"))
}

#[derive(Debug, Subcommand)]
pub enum CveAction {
    /// 顯示整合後的 CVE 資料（描述、各來源 CVSS、affected、NVD、KEV、EPSS、原廠公告、參考連結）
    Show {
        /// CVE ID，例如 CVE-2024-3400
        id: String,
        /// 本機沒有時，直接向 CVE.org 取得單筆（不存入本機）
        #[arg(long)]
        fetch: bool,
    },
    /// 在 cvelistV5 baseline 索引中搜尋（含與目錄無關、未存入本機的 CVE），用來找出漏掉的別名
    Search {
        /// 關鍵字（比對 CVE ID、vendor / product 與描述，不分大小寫）
        keyword: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum FindingAction {
    /// 列出命中（依風險排序；`?` 表示 probable、`!` 表示已處理但仍在受影響範圍）
    List {
        /// 狀態（可逗號分隔）：open、resolved、ignored
        #[arg(long, value_delimiter = ',', value_parser = ["open", "resolved", "ignored"])]
        status: Vec<String>,
        /// 等級（可逗號分隔）：P1、P2、P3、P4
        #[arg(long, value_delimiter = ',', value_parser = ["P1", "P2", "P3", "P4"], ignore_case = true)]
        priority: Vec<String>,
        /// 只列出指定資產
        #[arg(long, value_name = "ASSET_TAG")]
        asset: Option<String>,
        /// 信心等級
        #[arg(long, value_parser = ["confirmed", "probable"])]
        confidence: Option<String>,
    },
    /// 顯示命中的證據、修正版本、來源矛盾與時間軸
    Show {
        /// 命中編號
        id: i64,
    },
    /// 變更處理狀態（一律記錄事件）
    #[command(long_about = concat!(
        "變更處理狀態（一律記錄事件）。\n\n",
        "設為 ignored 時必須以 --reason 指定原因：",
        "not_affected（判定不受影響）、risk_accepted（接受風險）、false_positive（比對錯誤）。\n",
        "原因為 not_affected 時另需 --justification（CycloneDX VEX）：",
        "code_not_present、code_not_reachable、requires_configuration、requires_dependency、",
        "requires_environment、protected_by_compiler、protected_at_runtime、",
        "protected_at_perimeter、protected_by_mitigating_control。\n\n",
        "例：cvewatch finding set 42 ignored --reason not_affected ",
        "--justification requires_configuration --note \"未啟用 GlobalProtect\"",
    ))]
    Set {
        /// 命中編號
        id: i64,
        /// 新狀態：open、resolved、ignored
        status: String,
        /// 忽略原因（ignored 必填）
        #[arg(long)]
        reason: Option<String>,
        /// 不受影響的理由（--reason not_affected 必填）
        #[arg(long)]
        justification: Option<String>,
        /// 備註（記錄於時間軸）
        #[arg(long, default_value = "")]
        note: String,
    },
    /// 列出來源矛盾：最終判定不受影響、但有其他來源判定受影響
    Conflicts,
}

#[derive(Debug, Subcommand)]
pub enum PendingAction {
    /// 列出等待區（列入 KEV 者置頂）
    List {
        /// 只列出逾期（超過 [pending] stale_days）的 CVE
        #[arg(long)]
        stale: bool,
    },
    /// 人工移出等待區：套用到資產（建立 manual 的 probable 命中）或標記為不相關
    Resolve {
        /// CVE ID
        cve: String,
        /// 套用到資產；資產有多個相關元件時可用 <ASSET_TAG>:<產品 key> 指定
        #[arg(
            long,
            value_name = "ASSET_TAG",
            conflicts_with = "dismiss",
            required_unless_present = "dismiss"
        )]
        apply: Option<String>,
        /// 標記為不相關（之後不再自動放回等待區）
        #[arg(long)]
        dismiss: bool,
        /// 備註
        #[arg(long, default_value = "")]
        note: String,
    },
}
