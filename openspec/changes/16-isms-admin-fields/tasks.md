# Tasks

前提：15-scrub-model-names 已 archive。測試不呼叫真實外部 API、不存取真實 Credential Manager；core 測試使用記憶體資料庫與 `tests/fixtures/`，GUI 測試使用 `egui_kittest::Harness` 與 `cvewatch_frontend::test_support` 替身。fixture 與測試只用虛構的單位、人名與系統代碼（例如 `NET`、`SYS`、王小明），commit 前執行 `scripts/check-sensitive.ps1`。每完成一個任務群組就執行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。

## 1. core：資料模型與 migration

- [x] 1.1 `migrations/0008_isms_admin.sql`（design D7）並在 `migrate.rs` 登錄。完成條件：新增 `tests/db_migration_admin.rs`，以 0007 版本資料庫建立 `FW-01`（owner「王小明」、ISMS 編號 `SYS-HW-003`、2 筆命中）、retired 的 `NAS-02`（ISMS 編號 `sys-hw-005`）與 `NAS-01`（沒有 ISMS 編號），升級後斷言資產說明只有 `SYS`（名稱 `SYS`），`FW-01`、`NAS-02` 歸屬它，`FW-01` 保管人為「王小明」，`NAS-01` 沒有資產說明，個資皆為 0，命中數不變
- [x] 1.2 `classes.rs`：`AssetClass` 新增 `personal_data`（內建全為 false），`config.rs` 的 `[asset_classes.<代碼>]` 新增選填的 `personal_data`，並更新範例設定註解。完成條件：單元測試重現 spec 的 Personal data default for a class；未指定時新增子類別為 false
- [x] 1.3 `model.rs`：`AssetRecord` 新增 `system_code`、`responsible_unit`、`custodian`、`user_unit`、`compliance`、`personal_data`，移除 `owner`，並新增 `protection_level()`（design D6）；更新 `repo.rs` 的讀寫。完成條件：`cargo build -p cvewatch-core` 通過；單元測試重現 spec 的 Compliance raises the level、Compliance not rated
- [x] 1.4 新模組 `assets/systems.rs`：`AssetSystem`、`SystemDirectory`（依代碼或名稱查詢，不分大小寫、全形轉半形）、讀取、`create_system`、`rename_system`、`delete_system`（design D1、D5）。完成條件：測試重現 spec 的 Creating a system、Duplicate system code、Malformed system code、Renaming a system、Deleting a system in use
- [x] 1.5 測試：`cargo test -p cvewatch-core` 通過（同時更新既有測試中 `owner` 的用法）

## 2. core：欄位驗證

- [x] 2.1 `record.rs` 的 `validate_record` 改為接收 `RecordContext`（design D2）；新欄位的解析放在新檔 `record_admin.rs`，包括單位文字、法規遵循性 1–4，以及個資 Y/N/是/否與子類別預設值。完成條件：單元測試重現 spec 的 Recording the custodian and units、Compliance out of range、Default from the class、Explicit value、Invalid value；`record.rs` 低於 800 行
- [x] 2.2 資產說明歸屬規則：由 ISMS 編號帶出、依名稱解析、系統代碼不符、尚未定義；錯誤標示 `System` 或 `IsmsId` 欄位。完成條件：單元測試重現 spec 的 System derived from the ISMS ID、System given by name、ISMS ID from another system、Unknown system、Asset without a system
- [x] 2.3 `edit.rs` 的建立與欄位修改改用 `RecordContext`（從資料庫載入資產說明）。完成條件：`tests/asset_edit.rs` 新增 spec 的 Adding a new firewall（新預設值）與 Moving an asset to another system
- [x] 2.4 測試：`cargo test -p cvewatch-core` 通過

## 3. core：清冊匯入、匯出與範本

- [x] 3.1 `columns.rs`：新增六個 `StandardField`，依 design D3 調整欄序；`Owner` 改為 `Custodian` 的別名欄，兩者並存時回報表頭錯誤；新增 `SheetKind::Systems`。完成條件：單元測試斷言 `owner` 表頭解析為 `custodian` 並產生一則警告、兩者並存時報錯、自訂欄名對應到 `owner` 時同樣適用
- [x] 3.2 `sheet.rs`／`validate.rs`：讀取選填的 `systems` 工作表並驗證代碼、名稱與重複；`Inventory.kept_fields` 取代 `keep_isms_ids`（design D4）。完成條件：測試重現 spec 的 Duplicate name in the systems sheet；缺少 `system`、`custodian` 欄時只產生一則列出兩欄的提醒，isms_id 的提醒文案不變
- [x] 3.3 `plan.rs`／`import.rs`：匯入計畫加入資產說明的新增與改名（`--dry-run` 明細會列出），寫入時在同一交易內先處理資產說明；缺欄時保留既有值。完成條件：測試重現 spec 的 Importing a new system with its assets、Renaming a system by import、Re-importing an older inventory、Importing an owner column、Both owner and custodian columns
- [x] 3.4 `export.rs`、`template.rs`：寫出新欄位（個資為 Y/N）與 `systems` 工作表（所有資產說明）；範本的 `classes` 工作表加上個資預設值。完成條件：測試重現 spec 的 Generating a template（四個工作表）、Systems are exported；匯出後 `--dry-run` 重匯為「新增 0、更新 0」，且沒有資產說明變更
- [x] 3.5 fixtures：`gen_inventory_fixtures.rs` 產出含 `systems` 工作表與新欄位的清冊，以及只有 `owner` 欄的舊清冊；更新使用這些檔案的測試。完成條件：重新產生後 `git diff --stat tests/fixtures` 只列出預期檔案，`scripts/check-sensitive.ps1` 沒有命中
- [x] 3.6 測試：`cargo test -p cvewatch-core` 全部通過

## 4. CLI、TUI 與文件

- [x] 4.1 CLI `asset show`：「負責人」改為「保管人」，並顯示資產說明（代碼 名稱）、權責單位、使用單位、法規遵循性、防護等級（未評定）與是否含個人資料；`asset import` 的報表輸出資產說明的變更。完成條件：`crates/cvewatch/tests` 新增案例，斷言 `asset show FW-01` 的輸出含這些欄位
- [x] 4.2 TUI 資產詳情：「負責人」改為「保管人」，加上資產說明。完成條件：`crates/tui` 的畫面測試斷言詳情列出「保管人 王小明」與「資產說明 NET 網路服務」
- [x] 4.3 `docs/` 的清冊欄位說明：新欄位、`systems` 工作表、`owner` 改名為 `custodian`、升級後到 GUI 為資產說明改名。完成條件：文件中的範例欄名與 `cvewatch asset template` 產出的表頭一致
- [x] 4.4 測試：`cargo test` 全部通過

## 5. 前端與 GUI

- [x] 5.1 `frontend`：`asset_form.rs` 改為 `asset_form/` 模組，新增 `asset_form/admin.rs`，包含新欄位、個資「依類別」與防護等級的即時計算，驗證錯誤對應到欄位（design D8）。完成條件：`crates/frontend/tests/asset_edit.rs` 新增 ISMS ID from another system 與 Protection level follows compliance 的表單層測試；各檔案低於 800 行
- [x] 5.2 `frontend::data`：快照載入資產說明，新增 `asset_groups`；`workspace` 新增資產說明的新增、改名與刪除操作。完成條件：單元測試斷言依代碼排序、「未歸屬」在最後、空組與只有 retired 的組（未顯示退役時）不輸出、每組的未處理總數正確
- [x] 5.3 GUI 資產清單依資產說明分組，可收合，收合時標題顯示未處理數；工具列加入「資產說明…」。完成條件：`crates/gui/tests` 重現 spec 的 Assets under their systems、Collapsed group shows open findings、Group with only retired assets，並斷言跨組選取後按 Enter 開啟該資產的編輯
- [x] 5.4 `dialogs/asset_systems.rs`：資產說明管理對話框。完成條件：GUI 測試重現 spec 的 Adding a system from the window、Duplicate name in the dialog、System still in use
- [x] 5.5 資產對話框的「ISMS 管理」區塊（`dialogs/asset_admin.rs`）與詳情欄位。完成條件：GUI 測試重現 spec 的 ISMS ID from another system、Protection level follows compliance、Personal data follows the class、Administrative fields in the detail pane；`asset_edit.rs` 低於 800 行
- [x] 5.6 測試：`cargo test` 全部通過（手動目視檢查 GUI 移到 Workflow follow-up，見 design 的 apply 決定）

## Workflow follow-up

- 使用者手動啟動 `cvewatchw.exe`，確認資產清單分組、「資產說明…」對話框與資產對話框的「ISMS 管理」區塊中文顯示正常、版面沒有被截斷

- 依 `/opsx-auto` 流程：code review 一輪 → 判斷架構變更（新增 `asset_system` 表，屬於資料模型變更，視需要以 Archify 更新 `docs/architecture/`）→ commit → archive（同步主 spec、`openspec validate --all --strict`）→ commit
- archive 後 propose 17-isms-draft-import（ISMS 清冊轉 cvewatch 草稿），並在其 design 決定同一資產重複產品的處理
