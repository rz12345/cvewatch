# Tasks

前提：18-isms-import 已 archive。測試不呼叫真實外部 API、不存取真實 Credential Manager；core 測試使用記憶體資料庫，GUI 測試使用 `egui_kittest::Harness` 與 `cvewatch_frontend::test_support` 替身。

## 1. core：驗證與預覽

- [x] 1.1 `SystemDirectory` 的衝突檢查改以原代碼排除自身（design D2）；`validate_new` 與新的 `validate_edit` 共用同一個內部函式。完成條件：`systems.rs` 單元測試斷言 `NET`「網路服務」改為 `NWS`「網路服務」不算名稱重複；新代碼等於其他資產說明的代碼或名稱、格式不符時回傳 `SystemField::Code` 錯誤；`validate_new` 既有測試仍通過
- [x] 1.2 新增 `assets/systems_edit.rs`：`SystemEdit`、`RecodedAsset`、`SystemEditPlan`、`SystemEditError` 與 `plan_system_edit`（design D1、D3）；`tag_in_use` 改為 `pub(crate)`。完成條件：`crates/core/tests/system_recode.rs` 斷言 spec 的 Preview before writing（3 個資產、2 筆 ISMS 編號、1 筆資產編號變更，資料庫不變）；只改名稱時 `assets` 為空；`NET-hw-001` 這類寫法只換前三碼
- [x] 1.3 衝突檢查：新資產編號已被其他資產（含 retired、不分大小寫）使用時，收集全部衝突並回傳 `Fields`。完成條件：測試重現 spec 的 New asset tag already used 與 New code already used，資料庫不變
- [x] 1.4 core 測試：`cargo test -p cvewatch-core` 通過（含 1.1～1.3）

## 2. core：套用

- [x] 2.1 `edit_system`：交易內 `PRAGMA defer_foreign_keys = ON`、更新 `asset_system`、所有歸屬資產的 `system_code`、ISMS 編號（`AssetRepo::set_isms_id`）與資產編號（`AssetRepo::rename`），在交易內重算計畫並回傳；寫入 tracing 日誌（design D4）。移除 `rename_system`／`validate_rename`，呼叫處改用 `edit_system`／`validate_edit`。完成條件：測試重現 spec 的 Changing a code cascades to assets（含 retired、沒有 ISMS 編號的資產）與 Changing the code and name together，並以 `PRAGMA foreign_key_check` 確認沒有孤兒列
- [x] 2.2 保留紀錄與比對：完成條件：測試重現 spec 的 Renamed asset keeps its records（元件、版本歷史、命中、處理事件都在新編號下），且「比對待補」未被設定；既有 Renaming a system 測試（只改名稱、ISMS 編號不變）仍通過
- [x] 2.3 core 測試：`cargo test -p cvewatch-core` 通過（含 2.1～2.2）

## 3. frontend：`edit_system` 與預覽

- [x] 3.1 `Workspace::rename_system` 改為 `edit_system`，新增 `preview_system_edit`、`SystemEdited { refreshed, renamed }` 與常數 `RECODE_NOTE`（design D5）。完成條件：`crates/frontend/tests/asset_systems.rs` 斷言改代碼的狀態列訊息「已將資產說明代碼 NET 改為 NWS（…）」、`renamed` 列出 `NET-HW-001 → NWS-HW-001`；只改名稱時訊息與現在相同
- [x] 3.2 frontend 測試：`cargo test -p cvewatch-frontend` 通過（含 3.1，並斷言預覽有衝突時回傳 `Fields`、資料不變）

## 4. GUI：編輯列與確認

- [x] 4.1 `dialogs/asset_systems.rs`：「改名 X」改為「編輯 X」，編輯列含代碼與名稱兩個欄位、「儲存」「取消編輯」；更新模組說明註解（design D6）。完成條件：kittest 重現 spec 的 Editing a code without assets 與 Code already used in the dialog
- [x] 4.2 確認畫面：代碼改變且有資產時顯示受影響資產數、編號變更與 `RECODE_NOTE`，「確定修改」寫入、「返回」或 Esc 回到編輯列並保留輸入；寫入後選取中的資產換成新編號。完成條件：kittest 重現 spec 的 Confirming a code change、Going back from the confirmation、Selected asset follows its new tag、Conflicting asset tag in the dialog
- [x] 4.3 GUI 測試：`cargo test -p cvewatch-gui` 全部通過（含 4.1～4.2，並更新 `asset_systems.rs` 中使用「改名」「儲存名稱」的既有測試）

## 5. CLI：`cvewatch asset system recode`

- [x] 5.1 `cli.rs` 新增 `AssetAction::System { action: SystemAction::Recode { code, new_code, dry_run } }` 與中文說明；`commands/` 新增輸出函式（design D7 的格式，可放 `commands/asset_system.rs`）；更新模組開頭的子命令清單註解。完成條件：`cvewatch asset system recode --help` 顯示說明、兩個參數與 `--dry-run`
- [x] 5.2 輸出格式以純函式產生（輸入 `SystemEditPlan`、是否 dry-run），便於測試。完成條件：單元測試斷言 spec 的 Changing a code on the command line 與 Dry run 的輸出內容；沒有資產歸屬時只有標題與結果行
- [x] 5.3 CLI 測試：`crates/cvewatch/tests/asset_system.rs` 以 `assert_cmd` 驗證 `--help`、少給參數時以錯誤結束、尚未 init 時提示 `cvewatch init`；`tests/cli.rs` 的 asset 子命令 help 清單加入 `system`。Unknown system code 等邏輯由 1.x、2.x 的 core 測試涵蓋（CLI 測試不能開啟加密資料庫，與 `asset_rename.rs` 相同作法）。完成條件：`cargo test -p cvewatch --test asset_system` 通過

## 6. 文件與整合驗證

- [x] 6.1 README：目前沒有資產說明的說明，在資產編修段落後新增一小段：資產說明可在 GUI 新增、編輯與刪除；修改代碼的連動（歸屬、ISMS 編號、等於 ISMS 編號的資產編號）、衝突時整批拒絕、舊清冊／VEX／ISMS 清冊不會對應；CLI 範例 `cvewatch asset system recode NET NWS --dry-run`。完成條件：README 出現「資產說明」段落與 `asset system recode`
- [x] 6.2 全專案檢查：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`openspec validate 19-system-code-edit --strict`。完成條件：四個指令皆成功

## Workflow follow-up

- archive 時同步主 spec：`asset-inventory`（修改 Asset system，新增 Changing a system code、Changing a system code from the command line）、`gui-console`（修改 GUI asset system management，新增 GUI system code change confirmation）
- archive 後執行 `openspec validate --all --strict` 確認通過
- 之後依序 propose 20-asset-identity-network、21-component-roles、22-asset-lifecycle-eol、23-isms-export
