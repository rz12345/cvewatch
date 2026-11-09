//! `cvewatch init` 寫出的範例設定檔（由 [`crate::config`] 移出，避免該檔過長）。

use crate::config::{
    DEFAULT_LOG_RETENTION_DAYS, DEFAULT_MAX_RETRIES, DEFAULT_TIMEOUT_SECS, DEFAULT_USER_AGENT,
};
use crate::lifecycle::DEFAULT_REMINDER_DAYS;
use crate::matching::pending::DEFAULT_STALE_DAYS;
use crate::matching::scoring::{
    DEFAULT_CVSS_HIGH, DEFAULT_CVSS_MEDIUM, DEFAULT_EPSS_HIGH, DEFAULT_EPSS_MEDIUM,
    DEFAULT_EXPOSURE_BUMP,
};
use crate::notify::settings::{DEFAULT_SMTP_PORT, DEFAULT_URGENT_CVSS};
use crate::schedule::{DEFAULT_DIGEST_AT, DEFAULT_SYNC_EVERY_HOURS};

/// `cvewatch init` 寫出的範例設定檔。
pub fn example_config() -> String {
    let exposure_bump = format!(
        "[{}]",
        DEFAULT_EXPOSURE_BUMP
            .iter()
            .map(|e| format!("\"{e}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    format!(
        r#"# cvewatch 設定檔
# 秘密（NVD API key、SMTP 密碼、Redmine API key）不放在這裡，
# 請使用 `cvewatch config set-secret <key>` 存入 Windows Credential Manager。

# 資料目錄（資料庫、日誌、快取）。預設為 %LOCALAPPDATA%\cvewatch。
# 不建議放在 OneDrive 等雲端同步資料夾內（SQLite WAL 檔不應被同步）。
# data_dir = 'D:\cvewatch-data'

[http]
timeout_secs = {DEFAULT_TIMEOUT_SECS}
max_retries = {DEFAULT_MAX_RETRIES}
user_agent = "{DEFAULT_USER_AGENT}"

[sources]
nvd_enabled = true
osv_enabled = true
paloalto_enabled = true     # 內建 Palo Alto CSAF 公告（RSS）
eol_enabled = true          # endoflife.date 的軟體版本分支 EoL（`cvewatch sync --source eol`）

# 新增提供 CSAF 2.0 公告的原廠（不需更新程式）：
# [[sources.csaf]]
# id = "vendor-x"
# discovery = "provider_metadata"     # 或 rss（需 csaf_url_template，{{id}} 為 RSS item link 最後一段）
# url = "https://vendor-x.example/.well-known/csaf/provider-metadata.json"

[log]
retention_days = {DEFAULT_LOG_RETENTION_DAYS}
level = "info"

[import.columns]
# 現有清冊使用自訂欄名時，在此對應到標準欄位
# （asset_tag、name、category、product、version、exposure、confidentiality、integrity、
#   availability、isms_id、system、responsible_unit、custodian、user_unit、compliance、
#   personal_data、location、notes；舊欄名 owner 視為 custodian）
# "財產編號" = "asset_tag"
# "設備名稱" = "name"

# ISMS 子類別：內建 HW01–HW11、SW01–SW07（v2.1），可覆寫名稱、描述與 C/I/A 範本值，
# 或新增子類別（代碼為 HW 或 SW 加兩位數字；新增時 name 必填，C/I/A 三項全寫或全不寫）。
# personal_data 為是否含個人資料的預設值（true／false；內建皆為 false）。
# 修改範本值或預設值不會改變既有資產的 C/I/A 與是否含個人資料。
# [asset_classes.HW05]
# confidentiality = 3
#
# [asset_classes.HW06]
# personal_data = true
#
# [asset_classes.HW12]
# name = "實驗儀器"
# description = "示波器、量測儀器等具網路功能的設備"
# confidentiality = 2
# integrity = 3
# availability = 3

[scoring]
# 風險等級：P1 KEV；P2 EPSS ≥ epss_high 或 CVSS ≥ cvss_high；P3 EPSS ≥ epss_medium 或 CVSS ≥ cvss_medium；
# 其他為 P4。曝露度在 exposure_bump 內的資產上調一級。修改後下次比對（cvewatch match）重算全部等級。
epss_high = {DEFAULT_EPSS_HIGH:.2}
epss_medium = {DEFAULT_EPSS_MEDIUM:.2}
cvss_high = {DEFAULT_CVSS_HIGH:.1}
cvss_medium = {DEFAULT_CVSS_MEDIUM:.1}
exposure_bump = {exposure_bump}

[pending]
stale_days = {DEFAULT_STALE_DAYS}    # 在等待區超過此天數標為逾期

[lifecycle]
# EoL 與保固／維護合約／軟體授權到期的提前提醒天數（1–3650）；畫面標示與每日摘要共用
reminder_days = {DEFAULT_REMINDER_DAYS}

[notify]
# 緊急：未處理命中的 CVE 列入 KEV，或 CVSS ≥ urgent_cvss。sync 後立即寄信並在 Redmine 開案。
urgent_cvss = {DEFAULT_URGENT_CVSS:.1}
# 內部 SMTP / Redmine 使用機構自簽 CA 時，指定該 CA 的 PEM 檔（與系統根憑證並用）
ca_file = ""
# 明文傳輸例外（無加密 SMTP、http Redmine）只能在此手動開啟；
# 啟用後仍不會以明文送出 SMTP 密碼或 Redmine API key。
allow_insecure_transport = false

# 以下可在 `cvewatch tui` 的設定畫面（按 6）編輯並測試；
# SMTP 密碼與 Redmine API key 存入 Windows Credential Manager，不寫在這裡。
[notify.email]
enabled = false
smtp_host = ""
smtp_port = {DEFAULT_SMTP_PORT}
security = "starttls"        # starttls | tls（allow_insecure_transport = true 時才可用 none）
username = ""                # 空字串表示不驗證
from = ""                    # 例："cvewatch <you@example.edu.tw>"
to = []
send_empty = true            # 每日摘要沒有新事項時仍寄出「無新事項」

[notify.redmine]
enabled = false
base_url = ""                # 必須為 https
project_id = ""              # 數字 ID 或識別碼
tracker_id = 0
priority_map = {{}}            # 例：{{ P1 = 5, P2 = 4 }}（Redmine priority id；未對應者用預設優先權）
assigned_to_id = 0           # 0 = 不指派
add_note_on_triage = true    # cvewatch 中變更狀態時在議題加註記（不變更議題狀態）

[schedule]
# 修改後執行 `cvewatch schedule install` 套用到 Windows 工作排程器
sync_every_hours = {DEFAULT_SYNC_EVERY_HOURS}
digest_at = "{DEFAULT_DIGEST_AT}"
"#
    )
}
