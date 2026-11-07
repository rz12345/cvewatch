# Spec Delta

## Purpose

定義 cvewatch 本機 SQLite 資料庫的建立、版本升級、備份與並行存取行為，確保排程同步與 TUI 同時使用時資料不損壞。

## ADDED Requirements

### Requirement: Automatic schema migration
系統 SHALL 在每次開啟資料庫時檢查 schema 版本，並在單一交易內依序套用尚未執行的 migration；任一 migration 失敗時 SHALL 回復整批並結束。

#### Scenario: Upgrading an older database
- **WHEN** 資料庫版本為 3，程式內建 migration 到 5
- **THEN** 系統依序套用 4、5，完成後版本為 5

#### Scenario: Migration failure
- **WHEN** migration 5 執行失敗
- **THEN** 資料庫維持在版本 3，程式以非零結束碼結束並記錄失敗的 migration

#### Scenario: Database newer than binary
- **WHEN** 資料庫版本高於程式內建的最高版本
- **THEN** 系統拒絕開啟並提示升級程式，不修改資料庫

### Requirement: Concurrent read during sync
系統 SHALL 允許 TUI 在 sync 寫入期間讀取資料庫而不出現鎖定錯誤。

#### Scenario: TUI open while sync runs
- **WHEN** 排程 sync 正在寫入，使用者同時開啟 TUI 瀏覽命中清單
- **THEN** TUI 正常顯示最後一次提交的資料

### Requirement: Single sync at a time
系統 SHALL 確保同一時間只有一個 sync 程序寫入；若已有未過期的 sync 鎖，新的 sync SHALL 以結束碼 3 結束。超過 2 小時的鎖 SHALL 視為過期並可被取代。

#### Scenario: Overlapping scheduled runs
- **WHEN** 上一次 sync 尚未結束，下一次排程啟動
- **THEN** 新程序以結束碼 3 結束，不寫入任何資料

#### Scenario: Stale lock after crash
- **WHEN** sync 鎖已存在 3 小時且持有程序不存在
- **THEN** 新 sync 取得鎖並記錄「取代過期鎖」

### Requirement: Database encryption at rest
資料庫（含 WAL 與 shm 檔）SHALL 以 SQLCipher AES-256 加密；系統 MUST NOT 建立或開啟未加密的工作資料庫。

#### Scenario: Reading the file without the key
- **WHEN** 以一般 SQLite 工具開啟 `cvewatch.db`
- **THEN** 無法讀取任何資料表，檔案內容中找不到任何 asset_tag 或 CVE ID 字串

### Requirement: Database key management
`init` SHALL 以作業系統安全亂數產生 256-bit 金鑰並只存入憑證庫；開啟資料庫時金鑰遺失或錯誤 SHALL 以結束碼 5 結束且不修改資料庫檔案。

#### Scenario: First initialization
- **WHEN** 使用者執行 `cvewatch init`
- **THEN** 資料庫以新金鑰加密建立，金鑰存入憑證庫，畫面提示匯出復原金鑰

#### Scenario: Key missing after Windows reinstall
- **WHEN** 憑證庫中沒有資料庫金鑰
- **THEN** 程式以結束碼 5 結束，提示使用 `db import-key` 輸入復原金鑰

### Requirement: Recovery key export and import
系統 SHALL 提供 `db export-key` 經確認後在畫面顯示復原金鑰且不寫入檔案，並 SHALL 提供 `db import-key` 只在該金鑰能成功開啟資料庫時才寫入憑證庫。

#### Scenario: Importing a wrong key
- **WHEN** 使用者以 `db import-key` 輸入錯誤的復原金鑰
- **THEN** 系統拒絕並保持憑證庫原狀

#### Scenario: Reminder to export
- **WHEN** 復原金鑰從未匯出，使用者執行 `config check`
- **THEN** 輸出警告「尚未匯出資料庫復原金鑰」

### Requirement: Key rotation
系統 SHALL 提供 `db rekey` 以新金鑰重新加密資料庫，成功後更新憑證庫並將復原金鑰標為未匯出。

#### Scenario: Interrupted rotation
- **WHEN** `db rekey` 在資料庫已換成新金鑰、憑證庫尚未更新時中斷
- **THEN** 下次開啟時系統以暫存的新金鑰開啟資料庫並完成憑證庫更新，資料不遺失

#### Scenario: Rotating the key
- **WHEN** 使用者執行 `cvewatch db rekey`
- **THEN** 舊金鑰無法再開啟資料庫，新金鑰可以，並提示重新匯出復原金鑰

### Requirement: Encrypted online backup
`cvewatch db backup <path>` SHALL 在不中斷其他讀取者的情況下產出一致且加密的副本，預設使用資料庫金鑰；加上 `--passphrase` 時 SHALL 改以使用者輸入的口令加密。

#### Scenario: Backup while TUI is open
- **WHEN** TUI 開啟中，使用者執行 `cvewatch db backup D:\bak\cw.db`
- **THEN** 產生內容完整、需資料庫金鑰才能開啟的副本

#### Scenario: Portable backup
- **WHEN** 使用者執行 `db backup D:\bak\cw.db --passphrase` 並輸入口令
- **THEN** 副本只能以該口令開啟，與本機憑證庫無關

### Requirement: Plain backup requires explicit confirmation
未加密備份 SHALL 只在使用 `--plain` 且使用者輸入 `PLAIN` 確認後產生。

#### Scenario: Plain backup without confirmation
- **WHEN** 使用者執行 `db backup out.db --plain` 但確認時輸入其他文字
- **THEN** 不產生任何檔案

### Requirement: Restore from backup
系統 SHALL 提供 `db restore <file>`，以本機金鑰、口令（`--passphrase`）或復原金鑰（`--recovery-key`）開啟備份，並在完整驗證（含 schema 版本與 migration）通過後才覆蓋現有資料庫；覆蓋前 SHALL 保留原資料庫副本，且 MUST NOT 覆蓋先前保留的副本。系統 MUST NOT 修改備份檔本身。

#### Scenario: Restoring a passphrase backup on a new laptop
- **WHEN** 使用者在新電腦執行 `cvewatch init` 後 `db restore cw.db --passphrase` 並輸入正確口令
- **THEN** 資料以本機資料庫金鑰重新加密寫入，原資料庫改名為 `cvewatch.db.before-restore`

#### Scenario: Restoring a backup made before key rotation
- **WHEN** 使用者執行 `db rekey` 後，以 `db restore old.db --recovery-key` 輸入當時的復原金鑰
- **THEN** 資料以目前的本機資料庫金鑰重新加密寫入

#### Scenario: Wrong recovery key
- **WHEN** 使用者以 `--recovery-key` 輸入無法開啟該備份的復原金鑰
- **THEN** 程式以結束碼 5 結束，現有資料庫維持原狀

#### Scenario: Backup from a newer version
- **WHEN** 備份的 schema 版本高於程式內建的最高版本
- **THEN** 系統拒絕還原並提示升級程式，現有資料庫維持原狀

#### Scenario: Repeated restore
- **WHEN** 已存在 `cvewatch.db.before-restore` 時再次執行 `db restore`
- **THEN** 原資料庫以加上時間戳記的名稱保留，先前的 `cvewatch.db.before-restore` 不被覆蓋
