# Proposal

## Why

資產說明代碼建立後不能修改（16 的規格明定 MUST NOT），因為代碼同時是資料庫外鍵，也是 ISMS 編號的前三碼。ISMS 清冊的系統代碼調整（例如 `NET` 改為 `NWS`）或建立時打錯代碼，目前只能新增另一個資產說明，再逐一修改每個資產的歸屬、ISMS 編號與資產編號，步驟多又容易漏改。改為允許修改代碼，並在同一個交易內連動更新所有相關資料。

## What Changes

- core 新增「修改資產說明」操作：代碼與名稱可以一起修改
  - 新代碼的驗證與新增時相同（三碼英數字、不可與其他資產說明的代碼重複、不可等於其他資產說明的名稱）；名稱規則不變
  - 代碼改變時連動更新（同一交易，任一步驟失敗時資料庫不變）：
    - 所有歸屬此資產說明的資產（含 retired）改為歸屬新代碼
    - 這些資產的 ISMS 編號前三碼改為新代碼（`NET-HW-001` → `NWS-HW-001`）
    - 資產編號等於舊 ISMS 編號（不分大小寫）的資產，資產編號一併改為新 ISMS 編號；比照資產更名，元件、命中與處理紀錄都保留，不觸發重新比對
  - 任何一個新資產編號已被其他資產（含 retired，不分大小寫）使用時，整批拒絕並列出衝突的編號
  - 提供不寫入的預覽，列出受影響的資產數，以及 ISMS 編號與資產編號的變更
- GUI 資產說明對話框：「改名」改為「編輯」，同一列可修改代碼與名稱後一次儲存；代碼改變且有資產歸屬時，先顯示確認（受影響資產數、編號變更，以及舊清冊、VEX、ISMS 清冊不會對應的提醒），確定後才寫入
- 新增 CLI `cvewatch asset system recode <舊代碼> <新代碼> [--dry-run]`：輸出受影響的資產與編號變更；`--dry-run` 只預覽
- 修改代碼之後，外部仍使用舊代碼或舊編號的資料**不會**自動對應：舊清冊、已匯出的 VEX、尚未更新的 ISMS 清冊都需要使用者另外更新（完成時提示重新匯出清冊）
- **BREAKING**（規格層面）：`asset-inventory` 原本規定資產說明代碼 MUST NOT 修改，`gui-console` 規定代碼欄只在新增時可輸入，兩者都改為可依「修改資產說明代碼」的規則修改

## Capabilities

### New Capabilities

（無）

### Modified Capabilities

- `asset-inventory`：「Asset system」允許修改代碼；新增「Changing a system code」（連動規則、衝突時整批拒絕、預覽）與「Changing a system code from the command line」
- `gui-console`：「GUI asset system management」的「改名」改為可同時修改代碼與名稱的「編輯」，代碼改變且有資產歸屬時先確認

## Out of scope

- 合併兩個資產說明（新代碼已存在時一律拒絕，不提供合併）
- 保留舊代碼別名，或在匯入清冊、VEX、ISMS 清冊時把舊代碼／舊編號對應到新的
- 修改 repo 外的 ISMS 清冊檔案
- TUI 的資產說明管理
- CLI 修改資產說明名稱、新增或刪除資產說明
- 修改已送出或佇列中的通知、已建立的 Redmine 議題內容
- 修改代碼時通知其他已開啟的 TUI／GUI 視窗重新載入

## Impact

- `crates/core/src/assets/systems.rs`（必要時拆出 `systems_edit.rs`）：修改的驗證（排除自身）、預覽計畫與套用；外鍵在交易內延後檢查（`PRAGMA defer_foreign_keys`），不需要 migration
- `crates/core/src/assets/edit.rs`／`repo.rs`：重用資產更名（`rename_in`）與 ISMS 編號更新
- `crates/frontend/src/workspace/systems.rs`：`rename_system` 改為可修改代碼的 `edit_system`，提供預覽；選取中的資產被改編號時改選新編號
- `crates/gui/src/dialogs/asset_systems.rs`：「編輯」列與確認畫面
- `crates/cvewatch`：`AssetAction::System { action: SystemAction::Recode }` 子命令與輸出
- README：新增資產說明的段落（目前沒有），包含修改代碼的影響與 CLI 用法
