# Proposal

## Why

比對結果只在使用者打開 TUI 時才看得到，緊急漏洞（已遭利用或 CVSS ≥ 9）可能延遲好幾天才被注意到。需要讓 sync 自動定期執行，並在有緊急命中時立即寄信、在 Redmine 開案，其餘則彙整成每日摘要，讓處理紀錄回到使用者既有的工作流程。

## What Changes

- **排程**：`cvewatch schedule install` 在 Windows 工作排程器建立兩個工作 —「定期 sync」（預設每 4 小時）與「每日摘要」（預設 08:30）；錯過時間開機後補跑、僅在有網路時執行；提供 `schedule status|uninstall`
- **無視窗執行**：新增 `cvewatchw.exe`（Windows GUI subsystem 包裝程式），讓排程執行時不跳出主控台視窗
- **緊急通知**：sync 後若出現「緊急命中」（未處理且 CVE 在 KEV 或 CVSS ≥ 9；或既有命中新列入 KEV）→ 立即寄送 Email
- **每日摘要 Email**：自上次摘要以來的新命中（依等級）、自動狀態變化、等待區進出與逾期、同步失敗
- **Redmine 自動開案**：緊急命中依 CVE 開一張議題（列出所有受影響資產）；同一 CVE 之後有新資產受影響 → 在原議題加註記；cvewatch 中狀態變更時可選擇加註記（不關閉議題）
- **送出佇列（outbox）**：Email 與 Redmine 失敗時保留並於下次執行重送，同一事件不重複通知
- **TUI 設定畫面**：在 `cvewatch tui` 新增「設定」畫面，可自行設定 SMTP（主機、連接埠、加密、帳號、密碼、寄件者、收件者）、Redmine（網址、API key、專案、tracker、優先權對應、指派對象）、緊急門檻與排程時間；Redmine 的專案/tracker/優先權/成員由 API 取回後以清單挑選，不必手動查 ID；密碼與 API key 以遮罩輸入並存入 Windows Credential Manager；提供「測試寄信」「測試 Redmine」按鍵
- CLI：`cvewatch notify digest|urgent|test-email|test-redmine|outbox`（與 TUI 設定共用同一套驗證與測試函式）

## Capabilities

### New Capabilities

- `sync-scheduling`: Windows 工作排程的安裝、狀態、移除與無視窗執行
- `email-alerts`: 緊急通知與每日摘要的條件、內容、去重與重送
- `redmine-integration`: 緊急命中自動開案、補充註記與失敗重送

### Modified Capabilities

- `tui-console`: 新增「設定」畫面（通知與排程設定、秘密輸入、連線測試）；由 05 建立，本 change 只新增需求

## Out of scope

- Telegram / Teams / Slack 推播
- 由 Redmine 狀態回寫 cvewatch
- HTML 樣式化信件（MVP 為純文字 + 簡單表格）

## Impact

- `crates/core/src/notify/{email.rs,redmine.rs,outbox.rs,digest.rs}`、`crates/cvewatch/src/schedule.rs`、新 bin `cvewatchw`
- migration `0005_notify.sql`（notification_log、outbox、redmine_issue）
- 新相依：lettre（rustls）、quick-xml（產生 Task Scheduler XML）、toml_edit（TUI 儲存設定時保留設定檔註解與格式）
- `crates/tui/src/screens/settings.rs`
- 設定檔新增 `[notify.email]`、`[notify.redmine]`、`[schedule]`；秘密 `smtp_password`、`redmine_api_key`
