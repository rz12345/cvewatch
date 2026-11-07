# Design

## Context

- 筆電可能在排程時間關機、休眠或離線
- 秘密存在 Windows Credential Manager，排程必須以同一位使用者執行
- 使用者已用 Redmine 管理案件，Email 為機構 SMTP

## Goals / Non-Goals

**Goals:**
- 緊急事件在下一次 sync 後即通知，且同一事件只通知一次
- 筆電離線一段時間後恢復，不遺漏也不洗版

**Non-Goals:**
- 秒級即時性

## Decisions

### 設定

```toml
[schedule]
sync_every_hours = 4
digest_at = "08:30"

[notify]
urgent_cvss = 9.0            # 與 KEV 一起定義「緊急」
allow_insecure_transport = false   # 只能手動改設定檔；TUI 不提供開關
ca_file = ""                 # 選填：機構自簽 CA 的 PEM 檔，用來驗證內部 SMTP/Redmine 憑證

[notify.email]
enabled = true
smtp_host = "smtp.example.edu.tw"
smtp_port = 587
security = "starttls"        # starttls | tls（allow_insecure_transport = true 時才可用 none）
username = "rz12345"         # 空字串表示不驗證；密碼取自 Credential Manager: smtp_password
from = "cvewatch <rz12345@example.edu.tw>"
to = ["rz12345@example.edu.tw"]

[notify.redmine]
enabled = true
base_url = "https://redmine.example.edu.tw"
project_id = "infra-sec"
tracker_id = 1
priority_map = { P1 = 5, P2 = 4 }  # Redmine priority id
assigned_to_id = 0                 # 0 = 不指派
add_note_on_triage = true
```

以上欄位都可以在 TUI「設定」畫面編輯（見下節），也可以直接改檔；兩邊寫的是同一個 `cvewatch.toml`。

### TUI 設定畫面

主畫面鍵 `6` 進入，分三個分頁：**Email**、**Redmine**、**排程與門檻**。

```
┌ 設定 ─ [Email] Redmine  排程與門檻 ─────────────────────────────┐
│ 啟用           [x]                                               │
│ SMTP 主機      smtp.example.edu.tw                               │
│ 連接埠         587          加密  (•) STARTTLS ( ) TLS           │
│ 帳號           rz12345                                           │
│ 密碼           ●●●●●●  已設定        [Enter 變更] [Del 清除]     │
│ 寄件者         cvewatch <rz12345@example.edu.tw>                 │
│ 收件者         rz12345@example.edu.tw  [+ 新增]                  │
│                                                                  │
│ [t] 測試寄信   [Ctrl+S] 儲存   [Esc] 放棄變更                    │
└──────────────────────────────────────────────────────────────────┘
```

**Redmine 分頁的 ID 挑選**：填好網址與 API key 後按 `l`（載入），呼叫

| 用途 | API |
|---|---|
| 專案 | `GET /projects.json?limit=100`（分頁） |
| tracker | `GET /projects/{id}.json?include=trackers`（只列該專案啟用的 tracker） |
| 優先權 | `GET /enumerations/issue_priorities.json` |
| 指派對象 | `GET /projects/{id}/memberships.json` |

以清單挑選後，設定檔存的仍是數字 ID；畫面顯示「名稱 (#ID)」。API 無法連線時可手動輸入 ID。

**儲存規則**
- 欄位即時驗證（連接埠 1–65535、Email 格式、網址必須 https、門檻 0–10、時間 HH:MM），有錯誤時不能儲存，錯誤欄位標紅
- 一般欄位以 `toml_edit` 寫回 `cvewatch.toml`，保留使用者原有註解與排列；寫入採「寫暫存檔再 rename」避免中斷時損壞
- 密碼與 API key **只寫入 Credential Manager**，畫面只顯示「已設定 / 未設定」，任何時候都不回顯；輸入框為遮罩
- 儲存成功後若排程時間有變，提示「排程已變更，按 `i` 重新安裝排程」（呼叫 `schedule install`）
- 測試按鍵使用**尚未儲存的**表單值，讓使用者先測再存；測試函式與 `notify test-email` / `test-redmine` 共用
- 離開設定畫面時若有未儲存變更，詢問儲存 / 放棄 / 取消

### 傳輸安全

- SMTP：`starttls` 使用 lettre 的 `Tls::Required`（伺服器不支援 STARTTLS 就失敗，不降級為明文）；`tls` 為 SMTPS（465）
- Redmine：`base_url` 必須為 `https://`
- 憑證一律驗證，**不提供**略過驗證的選項；內部 CA 以 `ca_file` 加入信任（rustls 載入 PEM，與系統根憑證並用）
- 例外：`allow_insecure_transport = true` 時才接受 `security = "none"` 與 `http://` Redmine。此開關只能手動編輯設定檔（避免在 TUI 誤觸），啟用時：
  - TUI 頂端常駐紅色警示列「明文傳輸已啟用」
  - 每次 sync/digest 在日誌寫 WARN
  - `config check` 列為警告
- 即使啟用例外，SMTP 密碼與 Redmine API key 仍**拒絕**經明文連線送出（無驗證的內部轉送仍可用）

### 緊急命中的定義

```
urgent(finding) :=
  finding.status == open AND (cve in KEV OR display_cvss >= notify.urgent_cvss)
事件：
  NEW_URGENT        新建立且符合 urgent
  BECAME_KEV        既有 open/重新開啟的命中，其 CVE 新列入 KEV
```

`probable` 命中也會觸發（信件與議題中標示「版本待確認」），因為緊急漏洞寧可多報。

### 去重：notification_log

`notification_log(channel, event_kind, cve_id, finding_id NULL, sent_at)`，UNIQUE(channel, event_kind, finding_id)。
緊急信件以「一次 sync 一封」合併當次所有新事件。

### Outbox

```
outbox(id, channel ('email'|'redmine'), payload_json, created_at, attempts, last_error, next_attempt_at)
```
送出失敗 → attempts+1、指數退避（5 分、30 分、2 小時、之後每 6 小時），10 次後標為 dead 並在摘要與 TUI Dashboard 顯示。
每次 `sync` 與 `notify digest` 開始時先清 outbox。

### 每日摘要

`cvewatch notify digest`：範圍為「上次成功寄出的摘要時間」到現在（首次為過去 24 小時）。
內容段落：
1. 同步狀態（各來源最後成功時間；失敗者列出錯誤）
2. 新命中（依 P1→P4，每筆：等級、CVE、CVSS/EPSS/KEV、資產、元件@版本、判定來源、信心）
3. 狀態自動變化（自動 resolved / ignored / reopened）
4. 等待區：新進入、移出（原因）、逾期
5. outbox dead 項目
沒有任何內容時仍寄一封「無新事項」，讓使用者知道系統有在跑（可設定 `send_empty = false` 關閉）。

### Redmine

- `POST {base_url}/issues.json`，header `X-Redmine-API-Key`
- 一個 CVE 一張議題：`redmine_issue(cve_id PK, issue_id, created_at)`
- 主旨：`[cvewatch][P1][KEV] CVE-2026-XXXX PAN-OS — FW-01`（多資產時：`— 3 項資產`）
- 描述（Textile/Markdown 依 Redmine 設定，MVP 用純文字）：CVE 摘要、CVSS/EPSS/KEV（到期日）、受影響資產表、判定依據、原廠修正版本、參考連結
- 同 CVE 新增受影響資產 → `PUT /issues/{id}.json` 帶 `notes`
- `add_note_on_triage = true` 時，cvewatch 中變更狀態 → 加註記「FW-01：resolved（備註…）」
- 422 驗證錯誤（例如 tracker 不存在）不重試，直接 dead 並在 `notify test-redmine` 給出明確訊息

### Windows 排程

`cvewatch schedule install` 產生 Task Scheduler XML，透過 `schtasks /Create /TN "cvewatch\sync" /XML <file> /F` 建立：

- `LogonType = InteractiveToken`（以目前使用者、登入時執行 → 可讀 Credential Manager，不需儲存密碼）
- `StartWhenAvailable = true`（錯過時間開機後補跑）
- `RunOnlyIfNetworkAvailable = true`
- `DisallowStartIfOnBatteries = false`、`StopIfGoingOnBatteries = false`
- `MultipleInstancesPolicy = IgnoreNew`（搭配 01 的 sync 鎖）
- `ExecutionTimeLimit = PT1H`
- 動作：`cvewatchw.exe sync`、`cvewatchw.exe notify digest`

`cvewatchw.exe`：`#![windows_subsystem = "windows"]`，以 `CREATE_NO_WINDOW` 啟動同目錄的 `cvewatch.exe` 並傳回其結束碼。

`schedule status` 以 `schtasks /Query /TN cvewatch\sync /XML` 解析下次/上次執行時間與結果。

## Risks / Trade-offs

- InteractiveToken 代表使用者未登入時不會執行 → 筆電情境可接受；文件註明
- 機構 SMTP 可能限制外部寄送或需要特定認證 → `notify test-email` 先驗證；支援無驗證內部轉送；若轉送主機不支援 TLS，需手動開 `allow_insecure_transport`，且此時不送密碼
- Redmine 自訂欄位各單位不同 → MVP 不寫自訂欄位，之後可加 `custom_fields` 對應

## 實作時的決定（/opsx-auto 依建議處理）

以下為實作中需要取捨、依「最保守、最不會漏報」原則自行決定的事項。

### 緊急事件判定

- **以資料狀態比對 `notification_log`**，不只看單次比對的結果：`cvewatch match`、資產匯入、等待區套用產生的緊急命中也會在下一次 sync 時通知。只看使用中資產、未移除元件的未處理命中
- **事件種類**：從未通知過的命中，若 KEV 加入日期晚於命中建立日期 → `BECAME_KEV`，否則 `NEW_URGENT`；已通知過但當時不在 KEV、現在列入 → `BECAME_KEV`（每個命中只一次）
- **首次啟用通知**時，既有的緊急命中會合併為一封信通知（寧可多報）；Redmine 同樣依 CVE 開案。
  code review 指出大量既有命中時會一次開出許多議題；依「不漏報」原則維持此行為，於 README 提醒啟用前先處理既有命中
- Email 停用期間不判定、不記錄 Email 事件；之後啟用時一併通知

### 資料表（相對上文的擴充）

- `notification_log` 加 `id`、`kev`（通知當時是否在 KEV）、`queued_at`、`outbox_id`，`sent_at` 可為 NULL：**排入佇列時即寫入**（避免下次 sync 重複排入），送出成功才填 `sent_at`
- `outbox` 加 `kind`（`urgent` / `digest` / `create_issue` / `add_note`）、`dead_at` 與 `claimed_until`；送出成功即刪除
- 送出前先「認領」項目（`claimed_until` = 現在 + 15 分鐘，以讀取時的值做條件式更新），排程 sync 與摘要同時執行時不會重複送出；
  認領中的項目不會被其他程序送出或刪除，程序中止時 15 分鐘後失效
- 清佇列時某管道遇到暫時性失敗（伺服器無法連線、逾時）或設定未就緒後，本次不再嘗試該管道的其餘項目（不計次數），
  避免每筆都等到逾時而拖住 sync
- `notify urgent` 與 `notify digest` 先排入再清一次佇列（「開始時先清佇列」與排入後的送出合併）：
  摘要先取代尚未寄出的舊摘要再送出，不會先寄出舊摘要又寄新摘要；無法連線時只等一次逾時。`sync` 仍在開始時先清佇列
- TUI Dashboard 顯示無法送出的通知數（spec「Retry failed sends」）

### 錯誤分類

- Email 寄送失敗（含驗證失敗）一律依退避重送：修正設定後即可送出
- **設定未就緒**（缺少 smtp_password／redmine_api_key、CA 檔錯誤、http Redmine 等，送出端無法建立）時項目保留在佇列、
  **不計失敗次數**，每次回報原因（結束碼 4、`config check` 列為警告）；修正設定後即送出，不會因此變成無法送出而漏報
- Redmine：HTTP 401、403、404、422 與轉址（3xx）不重試（dead），其他（連線、逾時、5xx 等）重送
- 加註記得到 404（議題已在 Redmine 刪除）時清除該 CVE 的議題對應，之後的緊急事件重新開案
- Redmine 已回應建立成功、但本機無法記錄議題編號時仍視為已送出（寫錯誤日誌），不重送 POST 以免建立第二張
- 建立議題的請求重送前（先前曾失敗，或先前的程序送出途中中止而認領過期——兩者都可能 Redmine 已建立但回應沒收到），先以
  `GET /issues.json?project_id=…&status_id=*&f[]=subject&op[subject]=~&v[subject][]=<CVE>` 查詢主旨含此 CVE 的議題，
  找到即記錄對應、不再 POST；比對主旨時 CVE ID 須完整（`CVE-2026-1234` 不會比對到 `CVE-2026-12345`）
- 佇列中的建立請求遇到已記錄、但在 Redmine 已刪除的議題（加註記得到 404）時，清除對應後改為建立新議題
- 通知失敗不改變 `cvewatch sync` 的結束碼（只警告並寫日誌，通知留在佇列）；`cvewatch notify *` 有任一則送出失敗時結束碼為 4

### 設定

- `[notify.email] enabled` 與 `[notify.redmine] enabled` 預設 **false**；`send_empty` 放在 `[notify.email]`（預設 true）；`priority_map` 預設空（未對應的等級用 Redmine 預設優先權）
- `project_id` 可寫數字或識別碼；TUI 由清單挑選時寫入數字
- 格式與安全性錯誤（連接埠、Email 格式、門檻、HH:MM、未啟用例外卻使用 `none` / `http`）在載入時拒絕（結束碼 2）；啟用但欄位不完整不阻擋載入（否則無法開 TUI 修正），於送出、測試與 `config check` 時說明
- `config check` 新增通知區段（各管道是否可用、CA 檔是否存在），例外開關啟用時列為警告

### 每日摘要

- 新摘要**取代尚未送出（非 dead）的舊摘要**，範圍一律從上次成功寄出算起，離線數天也只寄一封
- 另一個程序正在寄出先前的摘要（認領中）時，本次不另產生摘要（避免內容重疊的兩封）
- 「無新事項」的判斷不含同步狀態與逾期清單，但**有來源同步失敗時不算無新事項**
- 新命中與狀態自動變化以 SQL 先依期間起點篩選（時間皆為 RFC 3339 UTC 字串），再於程式中精確比對
- 無法送出的通知只列本期內標為 dead 的項目，另附目前總數（避免每封摘要重複列出全部）
- Email 停用時 `notify digest` 不產生摘要，但仍清送出佇列（Redmine 項目）

### Redmine

- 狀態變更註記只**排入佇列**（TUI 與 `finding set` 不連網），於下次 sync 或摘要時送出；與狀態變更同一交易。一個 CVE 一則註記，列出所有變更的資產
- 只對狀態、原因或理由實際改變的命中加註記（`finding set` 設為相同狀態時仍記錄事件，但不產生註記）
- 等待區「判定不相關」一併忽略的 probable 命中也加註記，與等待區處理同一交易（`pending_ops::resolve_with`）
- 註記所屬議題仍在佇列中等待建立時延後（不計次數）；建立議題的請求已 dead 時註記標為 dead
- 建立議題送出時若該 CVE 已有議題（例如並行建立），改為加註記，不建立第二張
- 清單分頁以實際收到的筆數累計 offset（伺服器可能限制每頁筆數）
- Redmine client 使用同步 HTTP（reqwest blocking，`https_only`），**不跟隨轉址**（reqwest 跨主機轉址時不移除自訂 header，跟隨會把 API key 送到其他主機）；`ca_file` 以 `tls_certs_merge` 與系統根憑證並用
- 判斷 CVE 是否已有議題（或建立中）時一次載入全部對應，不逐一解析佇列內容；緊急命中每次只查詢一次，
  兩個管道的判定與議題的受影響資產清單共用

### SMTP

- lettre 以 rustls + aws-lc-rs + rustls-platform-verifier 建置：系統根憑證加上 `ca_file`
- `security = "none"` 時，設定了帳號**或**憑證庫中有 smtp_password 都拒絕寄送（spec「Plain SMTP with password configured」）

### Windows 排程

- 定期 sync 用 `TimeTrigger` + 無限重複（`Interval = PT{n}H`），摘要用每日 `CalendarTrigger`；XML 以 UTF-16 LE（含 BOM）寫入暫存檔交給 `schtasks /Create /XML /F`
- **`schedule status` 改用 PowerShell `Get-ScheduledTaskInfo`**：`schtasks /Query /XML` 只有工作定義、沒有上次／下次執行時間，`schtasks /V /FO CSV` 的時間格式又隨系統語系而異；建立與移除仍用 schtasks
- 明確指定的設定檔（`--config` 或 `CVEWATCH_CONFIG`）先轉為絕對路徑，再以 `--config` 寫入排程動作（排程的工作目錄是執行檔所在資料夾）
- 設定檔中相對的 `ca_file` 以設定檔所在目錄為準（與執行時的工作目錄無關）
- 每次 sync 只建立一個通知端（秘密只讀取一次），開始時清佇列與結束時的緊急通知共用
- `cvewatchw.exe` 無法啟動 `cvewatch.exe` 時以結束碼 1 結束（沒有主控台可顯示）

### TUI 設定畫面

- 鍵位：`Tab`/`Shift+Tab`（或 ←/→）切換分頁、`Enter`/`Space` 編輯或切換、`Del` 清除、`t` 測試、`l` 載入清單、`i` 重新安裝排程、`Esc` 離開、`Ctrl+S` 儲存（輸入中按下會先套用輸入值）；分頁標籤為 `[6]Settings`
- TUI 只編輯 P1／P2 的優先權對應；設定檔中其他等級（P3／P4）的對應儲存時原樣保留
- 啟用 Email／Redmine 時必填欄位（主機、寄件者、收件者；網址、專案、tracker）未填不能儲存
- 設定檔已手動啟用例外時，原有的 `none` / `http` 值在表單中可保留，但 TUI 不接受新的 http 網址，加密方式只能切換 STARTTLS / TLS
- 只改寫有變更的鍵（沿用原值的行尾註解），寫入前先以 core 的設定解析器驗證，再以暫存檔 + rename 取代
- 儲存時**先寫憑證庫再寫設定檔**：憑證庫失敗時什麼都不寫；設定檔失敗時秘密已儲存、其餘欄位仍為未儲存
- 開啟 TUI 時不讀憑證庫；進入設定畫面時才讀取秘密是否已設定，讀取失敗時在狀態列說明（顯示為未設定）
- `i` 以已儲存的設定執行 `cvewatch schedule install`（子程序）；排程欄位尚未儲存時提示先儲存
- 連續切換專案時可同時載入多個專案的 tracker／成員清單，結果回來時只採用目前表單專案的清單
- 連線動作以 `NotifyBackend` 抽象、在背景執行緒執行；TUI 測試以替身驗證（使用未儲存值、不阻塞），Redmine 清單的 HTTP 行為在 core 測試以 HTTPS 替身驗證

### 測試

- Redmine 一律 https（http 被拒），**以本機 rustls HTTPS 替身取代 wiremock**（wiremock 只提供明文 HTTP）；自簽 CA 以 rcgen 產生
- SMTP 以本機假伺服器（明文轉送與 SMTPS、AUTH PLAIN）驗證送出、STARTTLS 不降級、驗證失敗訊息與 `ca_file`
- 建立／移除真實排程工作的整合測試標記 `#[ignore]`，需在使用者機器上手動執行
