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
