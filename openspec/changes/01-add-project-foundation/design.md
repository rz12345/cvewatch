# Design

## Context

單人、單機、Windows 環境。同步程序由工作排程器在背景執行，TUI 由使用者手動開啟，兩者可能同時存取同一個 SQLite 檔。

## Goals / Non-Goals

**Goals:**
- `cargo build --release` 產出單一 `cvewatch.exe`，不需額外安裝 SQLite 或 OpenSSL
- 設定與秘密分離；設定檔可以放進 git 或 Obsidian，不會洩漏金鑰
- sync 與 tui 同時執行不會損壞資料

**Non-Goals:**
- 多使用者、遠端資料庫、Web 介面

## Decisions

### Workspace 結構

```
cvewatch/
├─ Cargo.toml                # [workspace]
├─ crates/
│  ├─ core/                  # lib：設定、DB、模型、HTTP、日誌（後續 change 在此擴充模組）
│  │  └─ src/{config.rs, db/, http.rs, secrets.rs, logging.rs, error.rs}
│  └─ cvewatch/              # bin：clap 子命令，只做參數解析與呼叫 core
└─ tests/fixtures/
```

後續 change 會在 `core` 新增 `assets/`、`ingest/`、`matching/`、`notify/` 模組，TUI 放 `crates/tui`。

### 檔案位置

| 項目 | 預設路徑 | 覆寫方式 |
|---|---|---|
| 設定檔 | `%APPDATA%\cvewatch\cvewatch.toml` | `--config <path>` 或 `CVEWATCH_CONFIG` |
| 資料目錄 | `%LOCALAPPDATA%\cvewatch\` | 設定檔 `data_dir` |
| 資料庫 | `<data_dir>\cvewatch.db` | — |
| 日誌 | `<data_dir>\logs\cvewatch.YYYY-MM-DD.log`，保留 30 天 | 設定檔 `log.retention_days` |
| cvelistV5 快取 | `<data_dir>\cache\cvelist\` | — |

使用 `directories` crate 取得路徑，確保非 Windows 也能編譯。

### 設定檔範例（cvewatch.toml）

```toml
data_dir = 'D:\cvewatch-data'   # 選填

[http]
timeout_secs = 30
max_retries = 3
user_agent = "cvewatch/0.1 (+internal tool)"

[sources]
nvd_enabled = true
osv_enabled = true
paloalto_enabled = true

[scoring]   # 由 04 使用
[notify]    # 由 06 使用
```

未知欄位 → 警告但不失敗；型別錯誤 → 結束碼 2 並指出行號（toml 錯誤訊息已含位置）。

### 秘密

`keyring` crate，service 名稱固定 `cvewatch`，帳號名為秘密鍵：`nvd_api_key`、`smtp_password`、`redmine_api_key`、`db_key`（資料庫金鑰，見下節）、`db_key_pending`（`db rekey` 進行中的新金鑰，完成後刪除）。
Credential Manager 底層以 DPAPI 依 Windows 使用者帳號加密。記憶體中的秘密以 `zeroize` 包裝，用完即清除。
`cvewatch config set-secret <key>` 從 stdin 讀取（不回顯），`cvewatch config check` 只顯示「已設定 / 未設定」。

### SQLite 與加密

- rusqlite `bundled-sqlcipher-vendored-openssl` feature（SQLCipher 4，AES-256-CBC + HMAC-SHA512，PBKDF2 由 raw key 略過）
- 開啟連線後**第一個**指令必須是 `PRAGMA key = "x'<64 hex>'"`（raw key，不經 KDF），接著 `SELECT count(*) FROM sqlite_master` 驗證金鑰正確，再設 `journal_mode=WAL`、`foreign_keys=ON`、`busy_timeout=5000`
- WAL 與 `-shm` 檔同樣被加密，TUI 讀取時 sync 仍可寫入
- Migration：`core/src/db/migrations/NNNN_name.sql`，以 `include_str!` 內嵌，`schema_version` 表記錄版本；啟動時在單一 transaction 內依序套用
- 寫入鎖：sync 開始時在 `sync_lock` 表寫入 PID 與時間；若已有未過期（< 2 小時）鎖則結束碼 3，避免兩個排程同時跑

### 資料庫金鑰生命週期

| 情境 | 行為 |
|---|---|
| `init` 首次建立 | `rand::rngs::OsRng` 產生 32 bytes → 存入 Credential Manager `db_key` → 以此建立加密資料庫；畫面提示立即執行 `db export-key` 保存復原金鑰 |
| 一般開啟 | 從 Credential Manager 讀 `db_key`；讀不到 → 結束碼 5，提示 `db import-key` |
| 金鑰錯誤 | SQLCipher 回報 `file is not a database` → 結束碼 5，不嘗試修改檔案 |
| `db export-key` | 需輸入 `YES` 確認，以 Base32 分組顯示（例如 `ABCD-EFGH-…`）供抄寫或存入密碼管理器；不寫入任何檔案 |
| `db import-key` | 不回顯輸入 → 先以該金鑰試開資料庫成功才寫入 Credential Manager |
| `db rekey` | ① 新金鑰先存入 `db_key_pending` → ② `PRAGMA rekey` → ③ 以新連線驗證新金鑰 → ④ 寫入 `db_key` 並刪除 `db_key_pending`，提示重新匯出。任何時間點中斷，憑證庫都至少保有一把能開啟資料庫的金鑰 |
| `rekey` 中斷後開啟 | `db_key` 無法開啟而 `db_key_pending` 可以 → 改用新金鑰並完成 ④，記錄警告、將復原金鑰標為未匯出 |

Windows 重灌或換帳號時 Credential Manager 會遺失，**復原金鑰是唯一還原方式**；`init` 與 `config check` 都會提醒是否已匯出（以 `db_key_exported_at` 記錄於資料庫）。

### 加密備份

- 預設：`ATTACH DATABASE '<path>' AS bak KEY "x'<db_key>'"; BEGIN; SELECT sqlcipher_export('bak'); COMMIT; DETACH`，在讀取交易內匯出以取得一致快照；副本以相同金鑰加密，可在 WAL 模式下線上執行；目的檔已存在時拒絕
- `--passphrase`：不回顯輸入兩次口令，以口令作為 SQLCipher 的 passphrase（走 SQLCipher 內建 PBKDF2-HMAC-SHA512，256,000 次）加密副本，方便在另一台電腦還原
- `--plain`：輸出未加密副本，須輸入 `PLAIN` 確認，並在檔名不含 `.plain` 時警告
- `db restore <file> [--passphrase | --recovery-key]`：備份金鑰依序判斷——明文（依檔頭 `SQLite format 3`）、`--passphrase` 口令、`--recovery-key` 復原金鑰（不回顯輸入 Base32；用於 `rekey` 之前或其他電腦以資料庫金鑰加密的備份）、否則本機 `db_key`。流程：
  1. 將備份複製為暫存副本，只讀副本（避免 SQLite 對備份檔本身回放或寫入旁邊的 journal/WAL）
  2. 以本機金鑰 `sqlcipher_export` 到 `cvewatch.db.restore-tmp`
  3. 以正式流程開啟暫存檔驗證：schema 版本高於程式則拒絕（`DbTooNew`）、migration 必須成功
  4. 合併現有資料庫 WAL 後改名為 `cvewatch.db.before-restore`；同名檔已存在時改用 `cvewatch.db.before-restore-YYYYMMDDHHMMSS`，不覆蓋先前保留的副本；舊 `-wal`/`-shm` 刪除或移動失敗即中止並移回
  5. 暫存檔改名為 `cvewatch.db`；失敗則將原資料庫移回
  - 1–3 任一步失敗都不動現有資料庫；其他程序開著資料庫時，Windows 上的改名會失敗，此時尚未變更任何檔案

### 日誌內容限制

日誌只記錄筆數、CVE ID、asset_tag 與錯誤類型；MUST NOT 記錄元件版本、命中明細、Excel 原始列內容或 HTTP 回應本文（debug 層級亦同，改記錄長度與雜湊）。

### 結束碼

| 碼 | 意義 |
|---|---|
| 0 | 成功 |
| 1 | 一般錯誤 |
| 2 | 設定錯誤 |
| 3 | 另一個 sync 正在執行 |
| 4 | 部分來源失敗（sync 完成但有來源出錯） |
| 5 | 資料庫金鑰遺失或錯誤 |

工作排程器可依結束碼判斷是否要通知。

### HTTP

`reqwest` + `rustls-tls`（避免 Windows 上 OpenSSL 相依）。reqwest 預設讀取 `HTTPS_PROXY`。重試規則：

- 429：所有方法都重試（伺服器未處理請求）
- 5xx 與連線錯誤：只對冪等方法（GET、HEAD、PUT、DELETE、OPTIONS）重試；POST 等非冪等方法不重試，避免 Redmine 重複開案
- 等待時間：429/503 帶 `Retry-After`（秒數或 HTTP-date）時依其值，否則指數退避（1s、4s、16s…），計算出的退避上限 15 分鐘；伺服器要求等待超過 15 分鐘則放棄並回報錯誤
- 錯誤訊息中的 URL 一律遮罩敏感 query 參數與帳密

## Risks / Trade-offs

- Credential Manager 綁定 Windows 使用者帳號：工作排程必須以同一帳號執行，否則讀不到秘密 → 在 06 的排程安裝指令中強制使用目前帳號，並在 `config check` 提示
- `db rekey` 在資料庫已換新金鑰、憑證庫尚未更新時中斷（當機、斷電） → `db_key_pending` 保存新金鑰，下次開啟自動完成更換
- `rekey` 之後，舊的資料庫金鑰備份無法以本機金鑰還原 → `db restore --recovery-key` 輸入當時的復原金鑰；`rekey` 完成時提示另做新備份
- 金鑰遺失等於資料遺失 → 強制提醒匯出復原金鑰；資料其實可由 Excel 重匯 + 重新 sync 重建，損失的是處理狀態與備註
- SQLCipher 需編譯 OpenSSL，Windows 建置環境較重（Perl、NASM）→ README 列出安裝步驟；CI 快取編譯結果
- 加密使讀寫慢約 5–15%，資料量小可忽略
- WAL 檔（`-wal`、`-shm`）若被同步軟體（OneDrive）同步可能損壞 → 文件中建議資料目錄不要放在 OneDrive 內，`init` 偵測到路徑含 `OneDrive` 時警告
