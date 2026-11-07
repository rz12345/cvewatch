# app-config Specification

## Purpose

定義 cvewatch 如何找到並驗證設定、存放秘密、提供命令列子命令與結束碼，讓手動執行與排程執行的行為一致。

## Requirements

### Requirement: Configuration file resolution
系統 SHALL 依序使用 `--config` 參數、`CVEWATCH_CONFIG` 環境變數、使用者設定目錄下的 `cvewatch.toml` 來決定設定檔；三者皆不存在時 SHALL 使用內建預設值執行。

#### Scenario: Explicit config path wins
- **WHEN** 使用者執行 `cvewatch --config D:\x\a.toml sync`，且 `CVEWATCH_CONFIG` 也有設定
- **THEN** 系統使用 `D:\x\a.toml`

#### Scenario: No config file present
- **WHEN** 三個位置都沒有設定檔
- **THEN** 系統以預設值執行，並在日誌記錄「使用預設設定」

### Requirement: Configuration validation
系統 SHALL 在載入設定時驗證型別與數值範圍；驗證失敗時 SHALL 以結束碼 2 結束，並輸出錯誤欄位與位置。未知欄位 SHALL 只產生警告。

#### Scenario: Invalid value type
- **WHEN** 設定檔中 `http.timeout_secs = "abc"`
- **THEN** 程式以結束碼 2 結束，錯誤訊息包含 `http.timeout_secs` 與行號

#### Scenario: Unknown key
- **WHEN** 設定檔包含未定義的欄位 `foo = 1`
- **THEN** 程式繼續執行，並輸出一筆指出 `foo` 的警告

### Requirement: Secrets are never stored in plain files
系統 SHALL 只從作業系統憑證庫讀取 NVD API key、SMTP 密碼與 Redmine API key；系統 MUST NOT 將秘密寫入設定檔、資料庫或日誌。

#### Scenario: Setting a secret
- **WHEN** 使用者執行 `cvewatch config set-secret nvd_api_key` 並輸入金鑰
- **THEN** 金鑰存入憑證庫，輸入過程不回顯，且資料目錄中任何檔案都找不到該字串

#### Scenario: Checking secrets
- **WHEN** 使用者執行 `cvewatch config check`
- **THEN** 系統列出每個秘密鍵為「已設定」或「未設定」，不顯示秘密內容

#### Scenario: Secret appears in an HTTP error
- **WHEN** 帶有 API key 的請求失敗並被寫入日誌
- **THEN** 日誌中的金鑰以 `***` 取代

### Requirement: Command-line interface
系統 SHALL 提供子命令 `init`、`sync`、`tui`、`asset`、`config`、`db`，每個子命令 SHALL 支援 `--help`。

#### Scenario: First-time initialization
- **WHEN** 使用者在全新環境執行 `cvewatch init`
- **THEN** 系統建立資料目錄、資料庫與範例設定檔，並印出各檔案路徑

#### Scenario: Data directory inside a sync folder
- **WHEN** `init` 偵測到資料目錄路徑包含 `OneDrive`
- **THEN** 系統顯示警告，說明 SQLite WAL 檔不應被雲端同步

### Requirement: Exit codes
系統 SHALL 以結束碼表達結果：0 成功、1 一般錯誤、2 設定錯誤、3 另一個 sync 正在執行、4 sync 完成但部分來源失敗、5 資料庫金鑰遺失或錯誤。

#### Scenario: Partial source failure
- **WHEN** sync 時 NVD 回應持續失敗而其他來源成功
- **THEN** 其他來源的資料照常寫入，程式以結束碼 4 結束

### Requirement: Outbound HTTP behaviour
系統 SHALL 對所有外部請求使用一致的 User-Agent 與逾時，對 HTTP 429 SHALL 重試，對冪等方法的 5xx 與連線錯誤 SHALL 重試，且 SHALL 遵守 `HTTPS_PROXY` 環境變數；系統 MUST NOT 對非冪等方法（如 POST）在 5xx 或連線錯誤時自動重試。

#### Scenario: Rate limited response
- **WHEN** 外部服務回應 429 並帶 `Retry-After: 30`
- **THEN** 系統等待至少 30 秒後重試，重試次數不超過設定的上限

#### Scenario: Server error on issue creation
- **WHEN** 以 POST 建立 Redmine 議題時伺服器回應 500
- **THEN** 系統不自動重試，直接回報錯誤，避免重複開案

### Requirement: Logs exclude sensitive inventory details
日誌 SHALL 只記錄筆數、CVE ID、asset_tag 與錯誤類型；在任何日誌層級 MUST NOT 記錄元件版本、命中明細、匯入檔原始內容或 HTTP 回應本文。

#### Scenario: Debug logging during import
- **WHEN** 以 debug 層級執行 `asset import`
- **THEN** 日誌記錄處理列數與 asset_tag，但不含任何版本字串或儲存格內容

### Requirement: Rotating log files
系統 SHALL 將日誌寫入資料目錄下的每日日誌檔，並 SHALL 刪除超過保留天數的舊檔。

#### Scenario: Old log cleanup
- **WHEN** 保留天數為 30，且存在 31 天前的日誌檔
- **THEN** 下次程式啟動後該檔被刪除
