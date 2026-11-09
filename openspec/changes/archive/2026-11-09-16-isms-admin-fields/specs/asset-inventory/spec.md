## ADDED Requirements

### Requirement: Asset system
系統 SHALL 維護「資產說明」（ISMS 的系統或服務），每個資產說明有代碼與名稱。代碼 SHALL 為三碼英數字，不分大小寫（全形字視同半形）並以大寫儲存；名稱 MUST NOT 為空。代碼與名稱 MUST NOT 與其他資產說明重複（名稱比對時全形視同半形、不分大小寫）；名稱 MUST NOT 等於其他資產說明的代碼。代碼建立後 MUST NOT 修改，名稱 SHALL 可修改。仍有資產（含 retired）歸屬的資產說明 MUST NOT 被刪除。

#### Scenario: Creating a system
- **WHEN** 新增資產說明，代碼 `net`、名稱「網路服務」
- **THEN** 系統建立代碼為 `NET`、名稱為「網路服務」的資產說明

#### Scenario: Duplicate system code
- **WHEN** 已有資產說明 `NET`，再新增代碼 `Net`、名稱「網路服務二」
- **THEN** 系統拒絕並指出代碼 `NET` 已被使用

#### Scenario: Malformed system code
- **WHEN** 新增資產說明，代碼 `NET1`
- **THEN** 系統拒絕並說明代碼必須是三碼英數字

#### Scenario: Name equal to another system's code
- **WHEN** 已有資產說明 `NET`，新增代碼 `LAB`、名稱 `net`
- **THEN** 系統拒絕並指出名稱 `net` 與資產說明 `NET` 的代碼相同

#### Scenario: Renaming a system
- **WHEN** 將資產說明 `NET` 的名稱改為「校園網路服務」
- **THEN** 歸屬 `NET` 的資產都顯示新名稱，資產的 ISMS 編號不變

#### Scenario: Deleting a system in use
- **WHEN** 刪除資產說明 `NET`，而 retired 的 `SW-CORE-02` 仍歸屬 `NET`
- **THEN** 系統拒絕並說明仍有 1 個資產歸屬此資產說明

### Requirement: Asset system membership
每個資產 SHALL 可歸屬一個資產說明（選填）。資產有 ISMS 編號時，ISMS 編號的系統代碼 MUST 與其資產說明代碼相同；資產說明留空時 SHALL 由 ISMS 編號的系統代碼帶出。指定或帶出的資產說明不存在時 SHALL 拒絕。清冊與編修 SHALL 接受資產說明的代碼或名稱（不分大小寫）。

#### Scenario: System derived from the ISMS ID
- **WHEN** 匯入一列 ISMS 編號 `NET-HW-002`、資產說明留空的資產，`NET` 已存在
- **THEN** 該資產歸屬資產說明 `NET`

#### Scenario: System given by name
- **WHEN** 編修 `FW-01` 時資產說明填「網路服務」
- **THEN** `FW-01` 歸屬資產說明 `NET`

#### Scenario: ISMS ID from another system
- **WHEN** 資產說明為 `NET` 的資產填寫 ISMS 編號 `SYS-HW-003`
- **THEN** 匯入或編修被拒絕，並指出 ISMS 編號的系統代碼 `SYS` 與資產說明 `NET` 不符

#### Scenario: Unknown system
- **WHEN** 匯入時第 8 列的資產說明為 `LAB`，資料庫與清冊的 `systems` 工作表都沒有 `LAB`
- **THEN** 匯入中止並指出第 8 列的資產說明 `LAB` 尚未定義

#### Scenario: Asset without a system
- **WHEN** 新增資產時資產說明與 ISMS 編號都留空
- **THEN** 資產儲存成功，不歸屬任何資產說明

### Requirement: Inventory systems sheet
清冊 SHALL 可包含 `systems` 工作表（標準欄名 `code`、`name`）。匯入時該表的每一列 SHALL 新增資產說明，或更新同代碼資產說明的名稱；資料庫中有、但該表沒有的資產說明 SHALL 保留。`systems` 工作表的驗證錯誤 SHALL 中止整批並列出工作表與列號。清冊沒有 `systems` 工作表時，資產說明 SHALL 維持不變。

#### Scenario: Importing a new system with its assets
- **WHEN** 清冊的 `systems` 工作表有 `NET`「網路服務」，`assets` 工作表有兩列資產說明為 `NET` 的資產
- **THEN** 匯入後有資產說明 `NET`，兩個資產都歸屬它

#### Scenario: Renaming a system by import
- **WHEN** 資產說明 `NET` 名稱為「網路服務」，匯入的 `systems` 工作表把 `NET` 寫成「校園網路服務」
- **THEN** `NET` 的名稱改為「校園網路服務」，`--dry-run` 預覽列出這項名稱變更

#### Scenario: Duplicate name in the systems sheet
- **WHEN** `systems` 工作表第 3 列與第 5 列的名稱都是「網路服務」
- **THEN** 匯入中止並指出 `systems` 工作表第 5 列的名稱重複，資料庫不變

### Requirement: ISMS administrative fields
每個資產 SHALL 有選填的權責單位、保管人、使用單位（文字，前後空白會去除，空字串視為未填）與法規遵循性（1–4，留空表示未評定）。法規遵循性不在 1–4 時 SHALL 拒絕。

#### Scenario: Recording the custodian and units
- **WHEN** 匯入一列權責單位「網路組」、保管人「王小明」、使用單位「系統組」的資產
- **THEN** 資產記錄這三個值，`asset show` 列出它們

#### Scenario: Compliance out of range
- **WHEN** 匯入時第 4 列的 `compliance` 為 `5`
- **THEN** 匯入中止並指出第 4 列的法規遵循性必須是 1–4

### Requirement: Protection level
資產有法規遵循性時，資通系統防護等級 SHALL 為 C、I、A 與法規遵循性四者的最大值；沒有法規遵循性時 SHALL 視為未評定，MUST NOT 顯示數值。防護等級 SHALL 隨 C/I/A 與法規遵循性變動，MUST NOT 由清冊或編修直接指定。

#### Scenario: Compliance raises the level
- **WHEN** 資產 C/I/A 為 2/3/3、法規遵循性為 4
- **THEN** 資產價值為 3，防護等級為 4

#### Scenario: Compliance not rated
- **WHEN** 資產 C/I/A 為 2/3/4、法規遵循性留空
- **THEN** 資產價值為 4，防護等級顯示為未評定

### Requirement: Personal data flag
每個資產 SHALL 標示是否含個人資料。清冊與編修 SHALL 接受 `Y`、`N`、`是`、`否`（不分大小寫）；留空時 SHALL 帶入該子類別的預設值。內建子類別的預設值 SHALL 皆為「否」。其他值 SHALL 被拒絕。修改子類別預設值 MUST NOT 改變既有資產。

#### Scenario: Default from the class
- **WHEN** 匯入一列類別 `HW05`、`personal_data` 留空的資產
- **THEN** 該資產標示為不含個人資料

#### Scenario: Explicit value
- **WHEN** 匯入一列 `personal_data` 為 `是` 的資產
- **THEN** 該資產標示為含個人資料

#### Scenario: Invalid value
- **WHEN** 匯入時第 6 列的 `personal_data` 為 `maybe`
- **THEN** 匯入中止並指出第 6 列的是否含個人資料只能填 Y、N、是或否

### Requirement: Inventory without new columns
清冊的 `assets` 工作表缺少 `system`、`responsible_unit`、`custodian`、`user_unit`、`compliance` 或 `personal_data` 欄時，既有資產的對應值 SHALL 保留，新增的資產 SHALL 以未填或預設值建立；匯入報表 SHALL 以一則提醒列出缺少的欄位。

#### Scenario: Re-importing an older inventory
- **WHEN** `FW-01` 的保管人為「王小明」、資產說明為 `NET`，使用者重匯沒有 `custodian` 與 `system` 欄的舊清冊
- **THEN** `FW-01` 的保管人與資產說明不變，報表有一則提醒列出缺少 `system` 與 `custodian` 欄

### Requirement: Legacy owner column
清冊的 `owner` 欄 SHALL 視為 `custodian` 匯入，匯入報表 SHALL 給出一次警告說明 `owner` 已改名為 `custodian`（保管人）。同一個工作表同時有 `owner` 與 `custodian` 欄時，匯入 SHALL 中止並說明只能保留其中一欄。

#### Scenario: Importing an owner column
- **WHEN** 匯入的清冊有 `owner` 欄，`NAS-01` 填「王小明」
- **THEN** `NAS-01` 的保管人為「王小明」，報表有一則「owner 欄已改名為 custodian（保管人）」警告

#### Scenario: Both owner and custodian columns
- **WHEN** 匯入的 `assets` 工作表同時有 `owner` 與 `custodian` 欄
- **THEN** 匯入中止並說明 `owner` 與 `custodian` 只能保留一欄，資料庫不變

### Requirement: Migrating owner and ISMS systems
升級到此版本時，系統 SHALL 把既有資產（含 retired）的 owner 轉為保管人；並 SHALL 為既有 ISMS 編號中出現的每個系統代碼建立資產說明（名稱暫時等於代碼），讓有 ISMS 編號的資產歸屬對應的資產說明。既有資產的權責單位、使用單位與法規遵循性 SHALL 為未填，是否含個人資料 SHALL 為「否」。元件、命中與處理紀錄 MUST NOT 改變。

#### Scenario: Upgrading a database with ISMS IDs
- **WHEN** 升級前 `FW-01` 的 owner 為「王小明」、ISMS 編號為 `SYS-HW-003`，`NAS-01` 沒有 ISMS 編號
- **THEN** 升級後有資產說明 `SYS`（名稱 `SYS`），`FW-01` 歸屬它且保管人為「王小明」；`NAS-01` 不歸屬任何資產說明；兩者的命中數與升級前相同

## MODIFIED Requirements

### Requirement: Asset class configuration
設定檔 SHALL 可覆寫內建子類別的名稱、描述、C/I/A 範本值與是否含個人資料的預設值，並 SHALL 可新增子類別；新增的代碼 MUST 為 `HW` 或 `SW` 加兩位數字，否則設定檔載入失敗並指出該代碼。子類別名稱 MUST NOT 與其他子類別重複，也 MUST NOT 是子類別代碼的形式。修改範本值或預設值 MUST NOT 改變既有資產的 C/I/A 與是否含個人資料。新增子類別未指定是否含個人資料時 SHALL 預設為「否」。

#### Scenario: Raising a template value
- **WHEN** 設定檔將 `HW05` 的機密性範本值改為 3，之後匯入一筆 C/I/A 留空的 `HW05` 資產
- **THEN** 新資產的機密性為 3，既有 `HW05` 資產的機密性不變

#### Scenario: Adding a class
- **WHEN** 設定檔新增 `HW12`，名稱「實驗儀器」、範本值 2/3/3
- **THEN** 清冊可使用 `HW12` 或「實驗儀器」作為 category

#### Scenario: Class code outside HW and SW
- **WHEN** 設定檔新增代碼 `PE06`
- **THEN** 設定檔載入失敗，錯誤訊息指出 `PE06` 必須以 HW 或 SW 開頭並接兩位數字

#### Scenario: Personal data default for a class
- **WHEN** 設定檔將 `HW06` 的是否含個人資料預設值設為「是」，之後匯入一筆 `HW06`、`personal_data` 留空的資產
- **THEN** 新資產標示為含個人資料，既有 `HW06` 資產不變

### Requirement: Blank template export
系統 SHALL 提供 `cvewatch asset template <path.xlsx>`，產出含 `assets`、`components` 與 `systems` 工作表、標準欄名與範例列的空白範本，並 SHALL 附一張 `classes` 工作表列出目前生效的子類別代碼、名稱、描述、C/I/A 範本值與是否含個人資料的預設值；匯入時 `classes` 工作表 SHALL 被忽略。

#### Scenario: Generating a template
- **WHEN** 使用者執行 `cvewatch asset template D:\inv.xlsx`
- **THEN** 檔案包含 `assets`、`components`、`systems`、`classes` 四個工作表，`assets`、`components` 與 `systems` 第一列為標準欄名

#### Scenario: Class sheet follows configuration
- **WHEN** 設定檔新增子類別 `HW12`，使用者產出範本
- **THEN** `classes` 工作表包含 `HW12` 與其名稱、範本值

### Requirement: Inventory export
系統 SHALL 提供 `cvewatch asset export <path.xlsx>`，把所有 active 資產匯出為含 `assets`、`components` 與 `systems` 工作表、標準欄名的 Excel：每個資產一列 `assets`（第一個元件寫在該列的 product / version），其餘清冊元件與未辨識產品列在 `components`，`systems` 列出所有資產說明。匯出 MUST NOT 包含 retired 資產與主機套件元件；目的檔已存在時 SHALL 拒絕且不覆蓋。

#### Scenario: Round trip without changes
- **WHEN** 使用者執行 `cvewatch asset export D:\inv.xlsx` 後，立刻以 `cvewatch asset import D:\inv.xlsx --dry-run` 匯入同一檔案
- **THEN** 預覽結果為「新增 0、更新 0」、沒有版本變更、資產說明變更與清冊中消失的資產

#### Scenario: Unresolved product is kept
- **WHEN** `PVE-01` 有一筆未辨識產品 `Proxmox Backup 3.2`，執行匯出
- **THEN** `components` 工作表有一列 `PVE-01`、`Proxmox Backup`、`3.2` 原文，重匯後該資產仍列出同一筆未辨識產品

#### Scenario: Retired assets and packages are left out
- **WHEN** `NAS-02` 為 retired，`WEB-01` 另有 dpkg 匯入的套件元件，執行匯出
- **THEN** 檔案中沒有 `NAS-02`，`WEB-01` 只匯出清冊元件，不含套件

#### Scenario: Export target exists
- **WHEN** `D:\inv.xlsx` 已存在
- **THEN** 指令以錯誤結束並說明目的檔已存在，原檔內容不變

#### Scenario: Systems are exported
- **WHEN** 有資產說明 `NET` 與 `SYS`，只有 `NET` 有 active 資產
- **THEN** `systems` 工作表列出 `NET` 與 `SYS` 兩列，`assets` 工作表的 `system` 欄寫資產說明代碼

### Requirement: Creating an asset
系統 SHALL 可不經清冊新增一個 active 資產；欄位驗證（必填欄位、類別、曝露度、C/I/A、ISMS 編號、資產說明、法規遵循性與是否含個人資料的允許值與預設值）SHALL 與清冊匯入相同，且 SHALL 至少有一個以產品目錄產品與版本指定的元件。`asset_tag` 已被任何資產（含 retired，不分大小寫）使用時 SHALL 拒絕。

#### Scenario: Adding a new firewall
- **WHEN** 新增 `FW-02`，名稱 `fw02`、類別 `HW03`、元件 PAN-OS `11.2.3`，未指定曝露度、C/I/A 與是否含個人資料
- **THEN** 系統建立 active 資產 `FW-02`（曝露度 internal、C/I/A 2/3/4、資產價值 4、不含個人資料、防護等級未評定）與一個元件

#### Scenario: Tag used by a retired asset
- **WHEN** 新增 `NAS-02`，而 `NAS-02` 已存在且為 retired
- **THEN** 系統拒絕並說明該編號屬於已退役資產，可改為恢復該資產

#### Scenario: No component given
- **WHEN** 新增資產時沒有任何元件
- **THEN** 系統拒絕並說明至少需要一個元件，資料庫不變

### Requirement: Editing asset fields
系統 SHALL 可修改 active 資產的 name、類別、曝露度、機密性、完整性、可用性、ISMS 編號、資產說明、權責單位、保管人、使用單位、法規遵循性、是否含個人資料、location、notes，驗證規則與清冊匯入相同；修改類別 MUST NOT 自動改變已設定的 C/I/A 與是否含個人資料。`asset_tag` SHALL 只能依 Renaming an asset 的規則修改。

#### Scenario: Raising exposure
- **WHEN** 將 `NAS-01` 的曝露度由 internal 改為 internet
- **THEN** 資產欄位更新，之後的比對以 internet 曝露度計算該資產命中的風險等級

#### Scenario: Invalid criticality
- **WHEN** 將機密性改為 `5`
- **THEN** 系統拒絕並說明機密性只能是 1–4，資產不變

#### Scenario: Changing class keeps ratings
- **WHEN** `PVE-01` 類別為 `HW01`、C/I/A 為 3/3/4，使用者把類別改為 `HW05`
- **THEN** 類別變為 `HW05`，C/I/A 仍為 3/3/4

#### Scenario: Moving an asset to another system
- **WHEN** `FW-01` 歸屬 `SYS`、ISMS 編號為 `SYS-HW-003`，使用者把資產說明改為 `NET` 但未修改 ISMS 編號
- **THEN** 系統拒絕並指出 ISMS 編號的系統代碼 `SYS` 與資產說明 `NET` 不符，資產不變
