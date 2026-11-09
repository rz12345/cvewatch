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
    /// 開啟文字介面：Dashboard、命中篩選與處理、資產、等待區、同步紀錄（按 ? 看鍵位）
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
    /// 通知：緊急信、每日摘要、Redmine 開案、連線測試與送出佇列
    Notify {
        #[command(subcommand)]
        action: NotifyAction,
    },
    /// Windows 工作排程：定期 sync 與每日摘要
    Schedule {
        #[command(subcommand)]
        action: ScheduleAction,
    },
    /// CycloneDX VEX：匯出與匯入命中的處理判斷（是否受影響、為什麼）
    Vex {
        #[command(subcommand)]
        action: VexAction,
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
            Command::Notify { .. } => "notify",
            Command::Schedule { .. } => "schedule",
            Command::Vex { .. } => "vex",
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
pub enum NotifyAction {
    /// 偵測尚未通知的緊急命中，寄出緊急信並在 Redmine 開案（sync 結束時會自動執行）
    Urgent,
    /// 寄送每日摘要（涵蓋上次成功寄出的摘要至今；排程每天執行）
    Digest,
    /// 以目前設定寄送測試信，顯示 SMTP 錯誤細節
    TestEmail,
    /// 驗證 Redmine API key、專案與 tracker（不建立議題）
    TestRedmine,
    /// 列出送出佇列（等待重送與無法送出的通知）
    Outbox,
}

#[derive(Debug, Subcommand)]
pub enum VexAction {
    /// 將命中與處理狀態匯出為 CycloneDX 1.6 JSON VEX（檔案不可已存在）
    #[command(long_about = concat!(
        "將命中與處理狀態匯出為 CycloneDX 1.6 JSON VEX（檔案不可已存在）。

",
        "狀態對應：未處理 → in_triage；已處理 → resolved（response: update）；",
        "忽略／不受影響 → not_affected（附 justification）；忽略／接受風險 → exploitable",
        "（response: will_not_fix）；忽略／比對錯誤 → false_positive。備註輸出為 detail。

",
        "匯出檔含資產清單與處理狀態，與資料庫同等敏感；--encrypt 以口令加密為 <檔名>.age。",
    ))]
    Export {
        /// 輸出檔（.json）
        file: PathBuf,
        /// 只匯出指定資產
        #[arg(long, value_name = "ASSET_TAG")]
        asset: Option<String>,
        /// 狀態（可逗號分隔）：open、resolved、ignored
        #[arg(long, value_delimiter = ',', value_parser = ["open", "resolved", "ignored"])]
        status: Vec<String>,
        /// 等級（可逗號分隔）：P1、P2、P3、P4
        #[arg(long, value_delimiter = ',', value_parser = ["P1", "P2", "P3", "P4"], ignore_case = true)]
        priority: Vec<String>,
        /// 以口令加密（age 格式，輸出 <檔名>.age，不產生明文檔）
        #[arg(long)]
        encrypt: bool,
    },
    /// 匯入 CycloneDX 1.4–1.6 JSON VEX，套用到本機已有的命中（不新建命中）
    #[command(long_about = concat!(
        "匯入 CycloneDX 1.4–1.6 JSON VEX，套用到本機已有的命中（不新建命中）。

",
        "元件對應順序：cvewatch bom-ref → purl → CPE → 名稱 + 版本（產品目錄別名）；命中以 CVE ID 對應",
        "（VEX id 不是 CVE 時查 references 中的 CVE 別名）。
",
        "本機最後一次人為變更晚於 VEX 的 lastUpdated（缺少時以 metadata.timestamp）時不套用，",
        "列為「本機較新」；加 --force 才覆蓋。not_affected 缺少 justification 或缺少 analysis 的項目不套用。
",
        "套用的變更記錄於時間軸，操作者為 import、備註含檔名。age 加密檔（.age）會要求輸入口令。",
    ))]
    Import {
        /// VEX 檔（.json 或 .json.age）
        file: PathBuf,
        /// 只報告將套用、已相同、未對應、本機較新、格式不完整的數量與明細，不寫入資料庫
        #[arg(long)]
        dry_run: bool,
        /// 本機較新時仍以檔案內容覆蓋
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ScheduleAction {
    /// 建立（或以新設定取代）`cvewatch\sync` 與 `cvewatch\digest` 兩個排程工作
    Install,
    /// 顯示兩個工作的上次執行時間、結果碼與下次執行時間
    Status,
    /// 移除兩個排程工作
    Uninstall,
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

/// `asset list --category`：子類別代碼（`HW01`）或大類（`HW`、`SW`），不分大小寫。
fn class_filter(raw: &str) -> Result<String, String> {
    cvewatch_core::assets::ClassFilter::parse(raw)
        .map(|_| raw.trim().to_string())
        .ok_or_else(|| format!("「{raw}」不是子類別代碼（例如 HW01、SW02）或大類（HW、SW）"))
}

#[derive(Debug, Subcommand)]
pub enum AssetAction {
    /// 匯入 Excel（assets / components / systems 工作表）或 CSV 清冊；以 asset_tag 新增或更新
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
        /// 只列出指定類別：子類別代碼（例如 HW01）或大類（HW 實體、SW 軟體）
        #[arg(long, value_parser = class_filter, value_name = "CODE|HW|SW")]
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
    /// 更改資產編號（元件、命中與處理紀錄保留；舊編號不再對應此資產）
    Rename {
        /// 目前的資產編號（須為使用中資產，區分大小寫）
        asset_tag: String,
        /// 新的資產編號（不可與其他資產重複，含已退役、不分大小寫）
        new_tag: String,
    },
    /// 匯出使用中資產為 Excel 清冊（與匯入相同格式；不含已退役資產與主機套件）
    Export {
        /// 輸出路徑（.xlsx，不可已存在）
        path: PathBuf,
    },
    /// 由 ISMS 資訊資產清冊產生 cvewatch 清冊草稿（不寫入資料庫；補上版本後以 asset import 匯入）
    IsmsDraft {
        /// ISMS 資訊資產清冊（.xlsx）
        workbook: PathBuf,
        /// 草稿輸出路徑（.xlsx，不可已存在）
        output: PathBuf,
    },
    /// 匯出空白 Excel 範本（assets、components、systems 工作表與 classes 子類別說明）
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
