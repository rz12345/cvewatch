# Proposal

## Why

13 已讓資產的類別、C/I/A 與 ISMS 編號對齊機構的 ISMS「資訊資產清冊」，但清冊還有幾欄 cvewatch 沒有：資產說明（資產屬於哪個系統／服務）、權責單位、保管人、使用單位、法規遵循性、資通系統防護等級、是否含個人資料。使用者因此仍要在 ISMS 清冊另外維護這些欄位。在 GUI 裡也看不出哪些資產屬於同一個系統。

清冊的結構是「一個資產說明對應多個資產（資產名稱＋類別），資產描述對應元件與版本」。cvewatch 已有資產與元件，缺的是資產上面的「資產說明」這一層，以及管理欄位。後續的 17（ISMS 清冊轉 cvewatch 草稿）與 21（匯出 ISMS 格式）都需要這些欄位。

## What Changes

- 新增**資產說明**（ISMS 的系統／服務，例如 `NET` 網路服務）：
  - 每個資產說明有三碼英數字代碼與名稱，兩者都不分大小寫唯一。代碼建立後不可修改，名稱可以修改
  - 資產可歸屬一個資產說明（選填）。資產有 ISMS 編號時，ISMS 編號前三碼 MUST 與它的資產說明代碼相同；資產說明留空時，由 ISMS 編號前三碼帶出
  - GUI 可新增、改名、刪除資產說明；仍有資產（含 retired）歸屬的資產說明不可刪除
  - 清冊新增 `systems` 工作表（`code`、`name`）與 `assets.system` 欄（填代碼或名稱）。範本與匯出都包含 `systems` 工作表
- 新增 ISMS 管理欄位：
  - 權責單位 `responsible_unit`、使用單位 `user_unit`：選填文字
  - **BREAKING** 保管人 `custodian` 取代 `owner`。既有資料的 owner 轉為保管人；清冊的 `owner` 欄仍可匯入，視為 `custodian` 並給出一次警告
  - 法規遵循性 `compliance`：選填 1–4，留空表示未評定
  - 資通系統防護等級：有法規遵循性時為 MAX(C, I, A, D)，未評定時不顯示數值；由系統計算，不能直接輸入
  - 是否含個人資料 `personal_data`：是／否。留空時帶入子類別的預設值（內建子類別都是「否」），設定檔 `[asset_classes.<代碼>]` 可用 `personal_data` 覆寫
- 清冊沒有上述某一欄時，既有資產的該欄值 SHALL 保留，匯入報表給出一次提醒（比照 13 的 `isms_id`）
- 既有資料轉換：有 ISMS 編號的資產，依編號前三碼建立資產說明（名稱暫時等於代碼）並歸屬；owner 轉為保管人
- GUI：資產清單依資產說明分組（未歸屬的資產另成一組）；資產對話框新增資產說明下拉選單與管理欄位；資產詳情顯示這些欄位與防護等級
- CLI `asset show` 與 TUI 資產畫面：「負責人」改為「保管人」，並顯示資產說明

## Capabilities

### New Capabilities

（無）

### Modified Capabilities

- `asset-inventory`：新增「Asset system」「Asset system membership」「Inventory systems sheet」「ISMS administrative fields」「Protection level」「Personal data flag」「Inventory without new columns」「Legacy owner column」「Migrating owner and ISMS systems」；修改「Asset class configuration」（個資預設值）、「Blank template export」「Inventory export」（`systems` 工作表）、「Creating an asset」「Editing asset fields」（新欄位）
- `gui-console`：新增「GUI asset system management」「GUI asset list grouped by system」；修改「GUI asset class and rating input」（新欄位輸入）、「GUI asset presentation」（詳情顯示新欄位，清單依資產說明分組）

## Out of scope

- ISMS 清冊轉 cvewatch 清冊草稿（17-isms-draft-import）
- 同一資產的同一產品出現多次（例如同一款交換器分別用在核心與邊緣）：17 設計草稿轉換時再決定
- 廠牌、型號、序號、財產編號、IP、主機名稱、管理介面（18-asset-identity-network）
- 元件角色（19-component-roles）
- 保固、維護合約與 EoL（20-asset-lifecycle-eol）
- 匯出 ISMS v2.1 格式的清冊（21-isms-export）。ISMS 的「資產名稱」到時由子類別名稱帶出，本 change 不新增欄位，cvewatch 的 `name` 維持為辨識用的簡稱
- 防護等級或法規遵循性影響風險等級
- 權責單位、保管人、使用單位的選項清單（維持自由文字）
- CLI 的資產說明管理指令（以 GUI 或清冊 `systems` 工作表維護）
- 修改資產說明代碼

## Impact

- `crates/core/src/db/migrations/0008_isms_admin.sql`：新增 `asset_system` 表、`asset` 的新欄位；owner 轉保管人；依 ISMS 編號建立資產說明
- `crates/core/src/assets/`：`model.rs`（`AssetRecord` 新欄位、`AssetSystem`）、`columns.rs`（新標準欄位、`owner` 停用別名、`systems` 工作表）、`validate.rs`、`plan.rs`、`import.rs`、`repo.rs`、`export.rs`、`template.rs`、`edit.rs`、`classes.rs`（個資預設值）；新增資產說明的存取與編修模組
- `crates/core/src/config.rs`：`[asset_classes]` 的 `personal_data`；`[import.columns]` 註解列出新欄名
- `crates/frontend`：`asset_form.rs`、`display.rs`、`data.rs`（依資產說明分組）、`workspace/assets.rs`；`crates/gui`：資產清單、詳情、編輯對話框、新的資產說明管理對話框；`crates/tui`：資產詳情；`crates/cvewatch`：`asset show`
- `tests/fixtures` 的清冊與 `gen_inventory_fixtures.rs`；`docs/` 的清冊欄位說明
- 既有使用者：舊清冊仍可匯入（`owner` 有警告，新欄位保留既有值）；建議升級後匯出一次新格式清冊
