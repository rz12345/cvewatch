# Proposal

## Why

cvewatch 需要一個穩定的骨架：單一執行檔、可預期的設定與資料位置、安全存放 API 金鑰，以及能隨版本演進的 SQLite 結構。後續五個 change（資產、CVE 同步、比對、TUI、通知）都建立在這層之上，先把它定下來可以避免之後反覆改動資料結構。

## What Changes

- 建立 Rust workspace 與 `cvewatch` 執行檔，提供子命令骨架：`init`、`sync`、`tui`、`asset`、`config`、`db`
- 定義設定檔 `cvewatch.toml` 的位置、格式與驗證規則（資料目錄預設 `%LOCALAPPDATA%\cvewatch`）
- 秘密（NVD API key、SMTP 密碼、Redmine API key）一律存放於 Windows Credential Manager，提供 `cvewatch config set-secret` 寫入
- 建立 SQLite 資料庫、版本化 migration、啟動時自動升級
- **資料庫加密**：以 SQLCipher（AES-256）加密整個資料庫，金鑰由系統隨機產生並存入 Credential Manager，使用者不需記密碼；提供 `db export-key` / `db import-key` 作為復原金鑰
- **加密備份**：`cvewatch db backup` 預設產出加密副本（沿用資料庫金鑰，或 `--passphrase` 改用口令），明文備份須明確加 `--plain` 並確認；`db restore` 可用 `--passphrase` 或 `--recovery-key`（復原金鑰）解開備份
- 日誌不得記錄資產版本與命中明細等敏感內容（只記數量與 ID）
- 建立結構化日誌（檔案輪替）與統一錯誤碼，供排程執行時排錯
- HTTP client 統一設定：User-Agent、逾時、重試、尊重 `HTTPS_PROXY`

## Capabilities

### New Capabilities

- `app-config`: 設定檔載入與驗證、資料目錄、秘密存放、CLI 子命令與結束碼
- `data-store`: SQLite 資料庫生命週期、schema migration、備份與並行存取規則

### Modified Capabilities

（無）

## Out of scope

- 任何資產、CVE、比對的商業邏輯（由 02–04 處理）
- TUI 畫面（05）
- 非 Windows 平台的秘密存放（程式碼保持可跨平台編譯，但只驗證 Windows）

## Impact

- 新增 Cargo workspace：`crates/cvewatch`（bin）、`crates/core`（lib）
- 主要相依：clap、serde、toml、rusqlite（`bundled-sqlcipher-vendored-openssl`）、keyring、reqwest（rustls）、tracing、tracing-appender、anyhow/thiserror、rand、zeroize
- 建置需求：SQLCipher 內建的 OpenSSL 在 Windows MSVC 上編譯需要 Perl（Strawberry Perl）與 NASM，README 需說明
