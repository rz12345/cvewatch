# Tasks

## 1. Workspace 與 CLI 骨架

- [x] 1.1 建立 Cargo workspace（`crates/core`、`crates/cvewatch`），設定 edition 2021、`[profile.release]` 開 LTO 與 strip — 完成條件：`cargo build --release` 產出 `cvewatch.exe`
- [x] 1.2 以 clap derive 建立子命令 `init`、`sync`、`tui`、`asset`、`config`、`db`（尚未實作者回傳「not implemented」與結束碼 1）— 完成條件：`cvewatch --help` 列出全部子命令
- [x] 1.3 定義 `ExitCode` enum（0/1/2/3/4/5）與 `main` 的錯誤對應
- [x] 1.4 測試：`assert_cmd` 驗證 `--help` 與未知子命令的結束碼

## 2. 設定與路徑

- [x] 2.1 以 `directories` 取得設定與資料目錄，實作 `--config` / `CVEWATCH_CONFIG` / 預設的解析順序
- [x] 2.2 定義 `Config` 結構（serde + toml），未知欄位收集成警告，型別錯誤回傳結束碼 2
- [x] 2.3 實作 `cvewatch init`：建立目錄、寫出範例 `cvewatch.toml`、OneDrive 路徑警告（資料庫建立與金鑰見 6.1）
- [x] 2.4 測試：解析順序、型別錯誤、未知欄位警告、init 冪等（重跑不覆蓋既有設定）

## 3. 秘密存放

- [x] 3.1 以 `keyring` 實作 `SecretStore`（get/set/delete），service = `cvewatch`
- [x] 3.2 實作 `config set-secret <key>`（`rpassword` 不回顯）與 `config check`
- [x] 3.3 實作日誌與錯誤訊息的秘密遮罩（URL query、header 值）
- [x] 3.4 測試：以 trait 注入 mock store 測試；遮罩函式單元測試

## 4. 資料庫

- [x] 4.1 rusqlite（`bundled-sqlcipher-vendored-openssl`）連線工廠：先 `PRAGMA key`（raw key）並驗證，再設 WAL、foreign_keys、busy_timeout；確認 Windows MSVC 建置（Perl、NASM）並寫入 README
- [x] 4.2 Migration runner：`schema_version` 表、內嵌 SQL、單一交易、版本過新時拒絕
- [x] 4.3 `0001_init.sql`：`schema_version`、`sync_lock`、`sync_run`（id、開始/結束時間、來源、狀態、筆數、錯誤訊息）
- [x] 4.4 sync 鎖：取得/釋放/過期取代
- [x] 4.5 測試：migration 成功/失敗回復/版本過新；兩個連線同時讀寫；鎖過期取代

## 6. 加密與金鑰

- [x] 6.1 `init` 產生 32-byte 金鑰（OsRng）存入 `db_key`，以 zeroize 包裝；開啟時金鑰遺失/錯誤 → 結束碼 5
- [x] 6.2 `db export-key`（確認、Base32 分組顯示、記錄 `db_key_exported_at`）、`db import-key`（先試開再寫入）、`db rekey`（新金鑰先存 `db_key_pending`，中斷後開啟時自動恢復）
- [x] 6.3 `config check` 與 `init` 的未匯出提醒
- [x] 6.4 `db backup`：預設 `sqlcipher_export` 同金鑰、`--passphrase`、`--plain` + `PLAIN` 確認
- [x] 6.5 `db restore <file> [--passphrase]`：只讀備份副本、先完整驗證（schema 版本、migration）再替換、保留 `.before-restore`（已存在則加時間戳記）、以本機金鑰重新加密
- [x] 6.6 日誌過濾：審查所有 tracing 呼叫不含版本/命中/回應本文；加上 lint 測試（grep 禁用欄位名稱）
- [x] 6.7 測試：以 rusqlite 未帶 key 開啟失敗；檔案位元組中搜尋不到 asset_tag/CVE ID；錯誤金鑰不修改檔案；rekey 後舊鑰失效；三種備份與還原往返
- [x] 6.8 `db restore <file> --recovery-key`：不回顯輸入 Base32 復原金鑰，解開以舊資料庫金鑰加密的備份；與 `--passphrase` 互斥 — 完成條件：`rekey` 後可還原 `rekey` 前的備份
- [x] 6.9 測試：`rekey` 後以舊復原金鑰還原舊備份；錯誤復原金鑰回報結束碼 5 且現有資料庫不變；`--passphrase` 與 `--recovery-key` 不可並用

## 5. HTTP 與日誌

- [x] 5.1 `HttpClient` 包裝 reqwest（rustls）：UA、逾時、429/5xx 指數退避、`Retry-After`（非冪等方法不因 5xx/連線錯誤重試）
- [x] 5.2 tracing + tracing-appender 每日輪替，啟動時刪除超過保留天數的檔案
- [x] 5.3 測試：以 `wiremock` 模擬 429 → 200，驗證重試次數與等待；日誌清理
