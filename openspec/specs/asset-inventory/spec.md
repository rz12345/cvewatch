# asset-inventory Specification

## Purpose

管理 cvewatch 所追蹤的設備與其上的軟體/韌體元件：以 Excel/CSV 清冊匯入（可重複且可預覽）與畫面編修維護資產，資料庫為目前狀態、清冊為交換格式（可匯出後再匯入）。

## Requirements

### Requirement: Asset and component model
系統 SHALL 以 `asset_tag` 唯一識別資產，每個資產 SHALL 可擁有多個元件，每個元件記錄產品與原始版本字串。

#### Scenario: Server with several tracked components
- **WHEN** 清冊中 `SRV-01` 的主要產品為 ESXi 8.0 U3c，components 工作表另列 iDRAC 7.10.30.00 與 BIOS 2.16.0
- **THEN** 系統建立一個資產與三個元件

### Requirement: Spreadsheet import
系統 SHALL 從 .xlsx 或 .csv 匯入資產，以 `asset_tag` 新增或更新，並 SHALL 支援以設定檔將自訂欄名對應到標準欄位。

#### Scenario: Chinese column headers
- **WHEN** Excel 欄名為「財產編號」，設定檔 `[import.columns]` 將其對應為 `asset_tag`
- **THEN** 匯入時以該欄作為 `asset_tag`

#### Scenario: Re-importing the same file
- **WHEN** 同一份檔案連續匯入兩次
- **THEN** 第二次的結果為「新增 0、更新 0」

### Requirement: Import dry-run
系統 SHALL 提供 `--dry-run`，只輸出新增、更新、版本變更、未辨識產品、清冊中消失的資產與版本待補的元件數量與明細，不寫入資料庫。

#### Scenario: Previewing a version change
- **WHEN** Excel 將 `FW-01` 的版本從 `11.1.4-h7` 改為 `11.1.6`，以 `--dry-run` 匯入
- **THEN** 輸出顯示 `FW-01` 版本 `11.1.4-h7 → 11.1.6`，資料庫內容不變

### Requirement: Atomic import with row validation
匯入 SHALL 在單一交易內完成；任何一列缺少必填欄位時 SHALL 中止整批並列出錯誤列號。

#### Scenario: Missing required field
- **WHEN** 第 12 列沒有 `product`
- **THEN** 匯入中止，錯誤訊息包含「第 12 列：缺少 product」，資料庫維持匯入前狀態

### Requirement: Retiring assets
清冊中不再出現的資產 SHALL 只在使用 `--prune` 時標為退役；退役資產 SHALL 保留歷史紀錄且不再參與比對。

#### Scenario: Asset removed from spreadsheet without prune
- **WHEN** `NAS-02` 已不在 Excel 中，匯入時未加 `--prune`
- **THEN** `NAS-02` 維持 active，匯入報表列出它為「清冊中消失」

#### Scenario: Asset removed with prune
- **WHEN** 同上但加上 `--prune`
- **THEN** `NAS-02` 狀態變為 retired 並記錄退役時間

### Requirement: Version change history
元件版本變更時，系統 SHALL 記錄舊版本與變更時間。版本待補的元件填上版本時沒有舊版本，MUST NOT 新增歷史紀錄。

#### Scenario: Firmware upgraded
- **WHEN** 重匯後 `NAS-01` 的 DSM 版本由 `7.2.1-69057` 變為 `7.2.2-72806`
- **THEN** 該元件的歷史中有一筆 `7.2.1-69057` 及變更時間

#### Scenario: Pending version filled
- **WHEN** `NET-HW-003` 的 DSM 元件由版本待補填為 `7.2.2-72806`
- **THEN** 該元件的版本歷史沒有新增紀錄

### Requirement: Exposure attribute
每個資產 SHALL 有曝露度（internet、dmz、internal、mgmt 之一，預設 internal）。

#### Scenario: Invalid exposure value
- **WHEN** 某列曝露度填寫 `public`
- **THEN** 匯入中止並指出該列曝露度不是允許的值

#### Scenario: Exposure left blank
- **WHEN** 某列曝露度留空
- **THEN** 該資產的曝露度為 internal

### Requirement: Asset class
每個資產 SHALL 屬於一個 ISMS 子類別，資料以子類別代碼（例如 `HW01`）存放；大類 SHALL 由代碼前兩碼推得（`HW` 實體、`SW` 軟體）。內建子類別 SHALL 為 ISMS 資訊資產清冊 v2.1 的 HW01–HW11 與 SW01–SW07。清冊與編修的類別 SHALL 接受子類別代碼（不分大小寫，全形字視同半形）或子類別名稱；不屬於已定義子類別的值 SHALL 被拒絕。

#### Scenario: Class given by name
- **WHEN** 某列 category 填寫「資安設備」
- **THEN** 該資產的類別為 `HW03`，大類為實體（HW）

#### Scenario: Class code in lower case
- **WHEN** 某列 category 填寫 `hw05`
- **THEN** 該資產的類別為 `HW05`

#### Scenario: Invalid category value
- **WHEN** 某列 category 填寫 `IF01`（ISMS 的資訊類，不屬於 HW 或 SW）或 `printer`
- **THEN** 匯入中止並指出該列 category 不是允許的值

### Requirement: Legacy category values
清冊匯入 SHALL 接受舊版 category 值並轉為子類別代碼：network → `HW02`、security → `HW03`、virtualization → `HW01`、server → `HW01`、storage → `HW05`、host → `HW01`。轉換時匯入報表 SHALL 對該列給出警告，指出原值與轉換後的代碼；匯入不因此中止。

#### Scenario: Old inventory with English category
- **WHEN** 匯入舊清冊，`FW-01` 的 category 為 `security`
- **THEN** 匯入成功，`FW-01` 的類別為 `HW03`，報表列出「security 已轉為 HW03 資安設備」警告

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

### Requirement: CIA rating and asset value
每個資產 SHALL 有機密性、完整性、可用性（各為 1–4）。清冊或編修時留空的項目 SHALL 帶入該子類別的範本值；子類別沒有範本值（內建的 SW01 應用系統）時三項 SHALL 為必填。資產價值 SHALL 為三者的最大值，且 SHALL 隨 C/I/A 變動。

#### Scenario: Template values for a server
- **WHEN** 匯入一列類別 `HW01`、C/I/A 留空的資產
- **THEN** 機密性 2、完整性 3、可用性 4，資產價值 4

#### Scenario: Explicit values
- **WHEN** 某列類別 `HW05`，機密性 4、完整性與可用性留空
- **THEN** 機密性 4、完整性 3、可用性 4，資產價值 4

#### Scenario: Value out of range
- **WHEN** 某列可用性填寫 `5`
- **THEN** 匯入中止並指出該列可用性必須是 1–4

#### Scenario: Application system without ratings
- **WHEN** 某列類別 `SW01`，C/I/A 留空
- **THEN** 匯入中止並指出該列 SW01 必須填寫機密性、完整性與可用性

### Requirement: Legacy criticality column
清冊中的 `criticality` 欄 SHALL 被忽略，匯入報表 SHALL 給出一次警告說明該欄已停用並改用機密性、完整性、可用性；匯入不因此中止。

#### Scenario: Old inventory with criticality
- **WHEN** 匯入的清冊有 `criticality` 欄，`FW-01` 填 `3`
- **THEN** 匯入成功，`FW-01` 的 C/I/A 依前述規則決定，報表有一筆「criticality 欄已停用」警告

### Requirement: ISMS asset ID
每個資產 SHALL 可有選填的 ISMS 資產編號，格式 SHALL 為三碼英數字系統代碼、`-`、兩碼大類、`-`、三碼數字（例如 `SYS-HW-001`），大類 MUST 與資產的子類別大類相同。ISMS 編號 MUST NOT 與其他資產（含 retired，不分大小寫）重複；留空表示沒有 ISMS 編號。清冊沒有 `isms_id` 欄時，既有資產的 ISMS 編號 SHALL 保留，匯入報表 SHALL 給出一次提醒。`asset_tag` 仍是資產的識別。

#### Scenario: Valid ISMS ID
- **WHEN** 類別 `HW03` 的 `FW-01` 填寫 ISMS 編號 `SYS-HW-003`
- **THEN** 資產儲存成功，`asset_tag` 仍為 `FW-01`

#### Scenario: Major class mismatch
- **WHEN** 類別 `HW03` 的資產填寫 ISMS 編號 `SYS-SW-003`
- **THEN** 匯入或編修被拒絕，並指出 ISMS 編號的大類 SW 與資產類別 HW03 不符

#### Scenario: Malformed ISMS ID
- **WHEN** 某列 ISMS 編號填寫 `SYSHW001`
- **THEN** 匯入中止並指出該列 ISMS 編號格式應為「系統代碼-大類-流水號」，例如 `SYS-HW-001`

#### Scenario: Duplicate ISMS ID
- **WHEN** `NAS-01` 填寫 `sys-hw-005`，而 retired 的 `NAS-02` 已使用 `SYS-HW-005`
- **THEN** 系統拒絕並指出該 ISMS 編號已被 `NAS-02` 使用

#### Scenario: Old inventory without ISMS column
- **WHEN** `FW-01` 的 ISMS 編號為 `SYS-HW-003`，使用者重匯沒有 `isms_id` 欄的舊清冊
- **THEN** `FW-01` 的 ISMS 編號仍為 `SYS-HW-003`，報表有一筆「清冊沒有 isms_id 欄，保留資料庫中既有資產的 ISMS 編號」提醒

### Requirement: Migrating existing assets
升級到此版本時，系統 SHALL 依 Legacy category values 的對照表轉換既有資產（含 retired）的類別，以轉換後子類別的內建範本值設定 C/I/A，並在 notes 末尾附記「舊重要性：N」。資產的元件、版本歷史、命中與處理紀錄 MUST NOT 改變。

#### Scenario: Upgrading a database with a firewall
- **WHEN** 升級前 `FW-01` 的 category 為 `security`、重要性 3、notes 為「機房 A 主防火牆」
- **THEN** 升級後 `FW-01` 類別為 `HW03`、C/I/A 為 2/3/4、notes 為「機房 A 主防火牆；舊重要性：3」，命中數與升級前相同

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

#### Scenario: Pending version is exported blank
- **WHEN** `NET-HW-003` 的 DSM 元件為版本待補，執行匯出後再以 `--dry-run` 匯入同一檔案
- **THEN** 檔案中該元件的 `version` 為空白，預覽結果為「新增 0、更新 0」，該元件仍為版本待補

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

### Requirement: Editing components
系統 SHALL 可對 active 資產新增目錄產品元件、修改元件版本、移除元件；一次儲存的多項編修 SHALL 在單一交易內完成，任何一項失敗時 MUST NOT 寫入其中任何一項。同一資產 MUST NOT 有兩個相同產品的元件；版本修改 SHALL 依 Version change history 記錄舊版本；移除後資產 SHALL 仍至少保留一個清冊元件或未辨識產品。主機套件元件 MUST NOT 被編修。

#### Scenario: Upgrading firmware on screen
- **WHEN** 將 `FW-01` 的 PAN-OS 版本由 `11.1.4-h7` 改為 `11.1.6`
- **THEN** 元件版本為 `11.1.6`，版本歷史中有一筆 `11.1.4-h7` 及變更時間

#### Scenario: Adding a duplicate product
- **WHEN** `SRV-01` 已有 iDRAC9 元件，再新增 iDRAC9 元件
- **THEN** 系統拒絕並說明該資產已有此產品，請改為修改版本

#### Scenario: Removing the last component
- **WHEN** `FW-01` 只剩一個元件且沒有未辨識產品，使用者移除該元件
- **THEN** 系統拒絕並說明資產至少需要一個元件，可改為刪除資產

#### Scenario: One failing change saves nothing
- **WHEN** 一次儲存 `FW-01` 的名稱變更與一個不在產品目錄中的元件
- **THEN** 系統拒絕並說明原因，名稱與元件都不變

#### Scenario: Version text the scheme cannot read
- **WHEN** 將 ESXi 元件版本改為 `8.0 latest`
- **THEN** 系統儲存該版本並顯示與匯入相同的版本格式警告

### Requirement: Resolving unresolved products
系統 SHALL 可把資產的一筆未辨識產品改為指定的目錄產品（沿用原版本字串，成為該資產的元件並移除該筆未辨識產品），或直接刪除該筆未辨識產品。

#### Scenario: Mapping to a catalog product
- **WHEN** `PVE-01` 有未辨識產品 `PVE 8.2.4`，使用者將它改為目錄產品 Proxmox VE
- **THEN** `PVE-01` 多一個 Proxmox VE `8.2.4` 元件，未辨識產品清單不再列出這一筆

#### Scenario: Mapping a pending unresolved product
- **WHEN** `NET-HW-004` 有一筆版本待補的未辨識產品「Zyxel ABCD」，使用者將它改為目錄產品 Zyxel 防火牆
- **THEN** `NET-HW-004` 多一個版本待補的 Zyxel 防火牆元件，未辨識產品清單不再列出這一筆

### Requirement: Edits are matched automatically
每次資產或元件編修 SHALL 在寫入的同一交易記錄「比對待補」，寫入後 SHALL 自動執行比對；比對失敗時已完成的編修 MUST NOT 回復，「比對待補」保留到下次比對成功。

#### Scenario: Version fixed and finding resolved
- **WHEN** `FW-01` 有一筆 open 命中只影響 `11.1.4-h7`，使用者將版本改為 `11.1.6`
- **THEN** 編修完成後自動比對，該命中依既有比對規則轉為已處理

#### Scenario: Matching fails after an edit
- **WHEN** 編修寫入成功但自動比對失敗
- **THEN** 編修保留，使用者看到需要執行 `cvewatch match` 的警告，下次匯入或 sync 會補跑比對

### Requirement: Retiring and restoring an asset
系統 SHALL 可將 active 資產標為 retired（行為與匯入時 `--prune` 相同：保留歷史紀錄且不再參與比對），並 SHALL 可將 retired 資產恢復為 active，恢復後其元件重新參與比對。

#### Scenario: Deleting an asset on screen
- **WHEN** 使用者刪除 `NAS-02`
- **THEN** `NAS-02` 狀態變為 retired 並記錄退役時間，其命中與處理紀錄仍保留

#### Scenario: Restoring a retired asset
- **WHEN** 使用者恢復 retired 的 `NAS-02`
- **THEN** `NAS-02` 變為 active、退役時間清除，自動比對後重新列出其命中

### Requirement: Permanently deleting an asset
系統 SHALL 只允許永久刪除 retired 資產；永久刪除 SHALL 在單一交易內移除該資產、其所有元件、版本歷史、未辨識產品、命中與命中處理紀錄，無法復原。CVE 資料、等待區與 Redmine 議題對應 MUST NOT 被刪除。刪除後該 `asset_tag` SHALL 可再被新增或匯入使用。

#### Scenario: Purging a retired asset
- **WHEN** 使用者永久刪除 retired 的 `NAS-02`
- **THEN** 資料庫中不再有 `NAS-02` 的資產、元件、命中與處理紀錄，其他資產與 CVE 資料不變

#### Scenario: Active asset cannot be purged
- **WHEN** 對 active 的 `FW-01` 要求永久刪除
- **THEN** 系統拒絕並說明需先標為退役，資料庫不變

#### Scenario: Reusing a purged tag
- **WHEN** `NAS-02` 已永久刪除後，清冊中又出現 `NAS-02`
- **THEN** 匯入將其視為新增的資產

### Requirement: Renaming an asset
系統 SHALL 可將 active 資產的 `asset_tag` 改為新編號；新編號 SHALL 去除前後空白且不可空白。新編號已被其他資產（含 retired，不分大小寫）使用時 SHALL 拒絕；只改變大小寫 SHALL 允許。更名後該資產的元件、版本歷史、未辨識產品、命中與處理紀錄 SHALL 保留不變。更名 MUST NOT 改變比對結果。retired 資產 MUST NOT 可更名。

#### Scenario: Renaming a NAS
- **WHEN** 將 `NAS-01`（有 3 筆命中，其中 1 筆已標為 ignored 並附備註）更名為 `NAS-11`
- **THEN** 資產編號為 `NAS-11`，3 筆命中與處理紀錄、元件與版本歷史都在 `NAS-11` 之下，資料庫中不再有 `NAS-01`

#### Scenario: Tag used by another asset
- **WHEN** 將 `NAS-01` 更名為 `nas-02`，而 `NAS-02` 已存在（active 或 retired）
- **THEN** 系統拒絕並說明該編號已被使用，資產不變

#### Scenario: Changing only letter case
- **WHEN** 將 `fw-01` 更名為 `FW-01`
- **THEN** 資產編號變為 `FW-01`

#### Scenario: Blank new tag
- **WHEN** 將 `NAS-01` 更名為 `"  "`
- **THEN** 系統拒絕並說明資產編號不可空白，資產不變

#### Scenario: Renaming a retired asset
- **WHEN** 將 retired 的 `NAS-02` 更名
- **THEN** 系統拒絕並說明需先恢復該資產

### Requirement: Old asset tag is not remembered
更名後系統 MUST NOT 保留舊編號與資產的對應：舊編號 SHALL 可再被新增或匯入使用；清冊匯入、VEX 匯入與 CLI 中出現的舊編號 MUST NOT 對應到更名後的資產。已送出或已排入送出佇列的通知內容 SHALL 保持原樣，之後產生的通知與 Redmine 註記 SHALL 使用新編號。

#### Scenario: Importing the old inventory after a rename
- **WHEN** `NAS-01` 已更名為 `NAS-11`，使用者以 `--prune` 匯入仍寫 `NAS-01` 的舊清冊
- **THEN** 系統新增 active 資產 `NAS-01`，`NAS-11` 因不在清冊中被標為退役，其命中與處理紀錄保留

#### Scenario: VEX with the old tag
- **WHEN** `NAS-01` 已更名為 `NAS-11`，匯入 bom-ref 為 `cvewatch:NAS-01:synology-dsm` 的 VEX
- **THEN** 該判定列為未對應，`NAS-11` 的命中不變

#### Scenario: Reusing the old tag
- **WHEN** `NAS-01` 已更名為 `NAS-11`，使用者新增資產 `NAS-01`
- **THEN** 系統建立新的 active 資產 `NAS-01`，與 `NAS-11` 各自獨立

### Requirement: Import rejects tags differing only in letter case
清冊匯入時，若某列的 `asset_tag` 與既有資產（含 retired）的編號只差大小寫，系統 SHALL 拒絕整批匯入並指出該列與既有編號，資料庫 MUST NOT 變更；完全相同的編號照常更新。

#### Scenario: Old inventory after a case-only rename
- **WHEN** `FW-01` 已更名為 `fw-01`，使用者匯入仍寫 `FW-01` 的清冊
- **THEN** 匯入以錯誤結束，說明第 2 列的 `FW-01` 與既有資產 `fw-01` 只差大小寫，資料庫不變

### Requirement: Renaming from the command line
`cvewatch asset rename <舊編號> <新編號>` SHALL 依 Renaming an asset 的規則更名，舊編號 SHALL 完全相同（區分大小寫）才對應。成功時 SHALL 顯示舊、新編號，並提示舊清冊與已匯出的 VEX 不會對應到新編號、建議以 `cvewatch asset export` 重新匯出清冊；失敗時 SHALL 以錯誤結束並說明原因，資料庫 MUST NOT 變更。

#### Scenario: Renaming on the command line
- **WHEN** 執行 `cvewatch asset rename NAS-01 NAS-11`
- **THEN** 輸出「已將 NAS-01 更名為 NAS-11」與重新匯出清冊的提示，資產編號為 `NAS-11`

#### Scenario: Unknown asset
- **WHEN** 執行 `cvewatch asset rename NAS-99 NAS-11`，而沒有 `NAS-99`
- **THEN** 指令以錯誤結束並說明找不到資產 `NAS-99`，資料庫不變

### Requirement: Asset system
系統 SHALL 維護「資產說明」（ISMS 的系統或服務），每個資產說明有代碼與名稱。代碼 SHALL 為三碼英數字，不分大小寫（全形字視同半形）並以大寫儲存；名稱 MUST NOT 為空。代碼與名稱 MUST NOT 與其他資產說明重複（名稱比對時全形視同半形、不分大小寫）；名稱 MUST NOT 等於其他資產說明的代碼。名稱 SHALL 可修改；代碼 SHALL 只能依 Changing a system code 的規則修改。仍有資產（含 retired）歸屬的資產說明 MUST NOT 被刪除。

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

### Requirement: Changing a system code
資產說明的代碼 SHALL 可修改，名稱可同時修改。新代碼 SHALL 依 Asset system 驗證，並 MUST NOT 與其他資產說明的代碼相同、MUST NOT 等於其他資產說明的名稱；與原代碼相同時視為只改名稱。代碼改變時，系統 SHALL 在同一個交易內：把所有歸屬的資產（含 retired）改為歸屬新代碼；把這些資產 ISMS 編號的系統代碼改為新代碼；資產編號等於舊 ISMS 編號（不分大小寫）的資產，依 Renaming an asset 的保留規則把資產編號改為新 ISMS 編號。ISMS 編號的系統代碼依 Asset system membership 的寫法判斷（第一個 `-` 之前），系統代碼不是原代碼的 ISMS 編號 MUST NOT 被更動。任何一個新資產編號或新 ISMS 編號已被其他資產（含 retired，不分大小寫）使用，或同一次修改中有兩個資產會得到相同的新編號時，系統 SHALL 拒絕並列出衝突的編號，資料庫 MUST NOT 變更。系統 SHALL 可在寫入前產生預覽，列出受影響的資產數與每筆 ISMS 編號、資產編號的變更。

#### Scenario: Changing a code cascades to assets
- **WHEN** `NET` 有 active 的 `NET-HW-001`（ISMS 編號 `NET-HW-001`）、retired 的 `SW-CORE-02`（ISMS 編號 `NET-SW-002`）與沒有 ISMS 編號的 `FW-LAB`，將 `NET` 的代碼改為 `nws`
- **THEN** 資產說明代碼為 `NWS`、名稱不變；三個資產都歸屬 `NWS`；ISMS 編號改為 `NWS-HW-001` 與 `NWS-SW-002`；`NET-HW-001` 更名為 `NWS-HW-001`，`SW-CORE-02` 與 `FW-LAB` 的資產編號不變

#### Scenario: Renamed asset keeps its records
- **WHEN** `NET` 改為 `NWS` 後，資產 `NET-HW-001` 已更名為 `NWS-HW-001`
- **THEN** `NWS-HW-001` 保留原有的元件、版本歷史、命中與處理紀錄，比對結果不變

#### Scenario: New code already used
- **WHEN** 已有資產說明 `NET` 與 `SYS`，將 `NET` 的代碼改為 `sys`
- **THEN** 系統拒絕並指出代碼 `SYS` 已被使用，資料庫不變

#### Scenario: New asset tag already used
- **WHEN** 將 `NET` 改為 `NWS`，`NET-HW-001` 需更名為 `NWS-HW-001`，而另一個 retired 資產的編號是 `nws-hw-001`
- **THEN** 系統拒絕並列出衝突的編號 `NWS-HW-001`，資產說明、歸屬、ISMS 編號與資產編號都不變

#### Scenario: New IDs collide within the change
- **WHEN** `NET` 有資產 `NET-HW-001` 與 `ＮＥＴ-HW-001`（全形，ISMS 編號各與資產編號相同），將 `NET` 改為 `NWS`
- **THEN** 系統拒絕並指出資產編號與 ISMS 編號 `NWS-HW-001` 在這次修改中重複，資料庫不變

#### Scenario: Preview before writing
- **WHEN** 預覽將 `NET` 改為 `NWS`，`NET` 有 3 個資產歸屬、其中 2 個有 ISMS 編號、1 個資產編號等於 ISMS 編號
- **THEN** 預覽列出 3 個受影響的資產、2 筆 ISMS 編號變更與 1 筆資產編號變更，資料庫不變

#### Scenario: Changing the code and name together
- **WHEN** 將 `NET`「網路服務」同時改為代碼 `NWS`、名稱「校園網路服務」
- **THEN** 資產說明為 `NWS`「校園網路服務」，名稱比對時不把自己原本的名稱視為重複

### Requirement: Changing a system code from the command line
`cvewatch asset system recode <舊代碼> <新代碼>` SHALL 依 Changing a system code 的規則修改代碼，代碼 SHALL 不分大小寫對應。指令 SHALL 輸出受影響的資產數與每筆 ISMS 編號、資產編號的變更；成功時 SHALL 顯示舊、新代碼，並提示舊清冊、已匯出的 VEX 與尚未更新的 ISMS 清冊不會對應到新代碼或新編號、建議以 `cvewatch asset export` 重新匯出清冊。加上 `--dry-run` 時 SHALL 只輸出預覽，資料庫 MUST NOT 變更。失敗時 SHALL 以錯誤結束並說明原因，資料庫 MUST NOT 變更。

#### Scenario: Changing a code on the command line
- **WHEN** 執行 `cvewatch asset system recode net nws`，`NET` 有 1 個資產 `NET-HW-001`（ISMS 編號相同）
- **THEN** 輸出 ISMS 編號與資產編號 `NET-HW-001 → NWS-HW-001`、「已將資產說明代碼 NET 改為 NWS」與重新匯出清冊的提示

#### Scenario: Dry run
- **WHEN** 執行 `cvewatch asset system recode NET NWS --dry-run`
- **THEN** 輸出相同的變更清單並說明未寫入，資料庫不變

#### Scenario: Unknown system code
- **WHEN** 執行 `cvewatch asset system recode LAB NWS`，而沒有資產說明 `LAB`
- **THEN** 指令以錯誤結束並說明找不到資產說明 `LAB`，資料庫不變

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

### Requirement: Pending component version
清冊元件與未辨識產品的版本 SHALL 可以是空白，狀態為「版本待補」。版本待補的元件 SHALL 依 vuln-matching 的 Pending versions are not matched 處理。主機套件元件 MUST NOT 是版本待補。資產只有版本待補的元件時，仍 SHALL 滿足「至少一個清冊元件或未辨識產品」的要求。

#### Scenario: Asset with only pending components
- **WHEN** 匯入後 `NET-HW-003` 只有一個版本待補的 Synology DSM 元件
- **THEN** `NET-HW-003` 為 active 資產，元件版本顯示為版本待補

### Requirement: Blank version on import
清冊匯入時，`version` 空白的元件列 SHALL 成為版本待補，MUST NOT 中止匯入。資料庫中該元件已有版本、檔案中為空白時，元件 SHALL 改為版本待補，預覽 SHALL 將其列為版本變更（例如 `11.1.6 → 待補`）。預覽與完成摘要 SHALL 列出匯入後版本待補的元件數。

#### Scenario: New component without version
- **WHEN** 清冊中 `NAS-03` 的 Synology DSM 元件 `version` 空白
- **THEN** 匯入成功，`NAS-03` 有一個版本待補的 DSM 元件，摘要列出「版本待補 1 個元件」

#### Scenario: Known version cleared in the spreadsheet
- **WHEN** 資料庫的 `FW-01` PAN-OS 版本為 `11.1.6`，重匯的清冊中該元件 `version` 空白
- **THEN** 預覽列出 `FW-01` 版本 `11.1.6 → 待補`；匯入後該元件為版本待補，版本歷史中有一筆 `11.1.6`

### Requirement: Filling pending versions
在畫面或指令編修元件時，版本待補的元件 SHALL 可以填上版本，填上後成為一般元件並依 Edits are matched automatically 自動比對。已有版本的元件 MUST NOT 被改成空白版本；新增資產與加入元件時 MUST 填寫版本。

#### Scenario: Filling a pending version
- **WHEN** `NET-HW-003` 的 DSM 元件為版本待補，使用者把版本填為 `7.2.2-72806` 並儲存
- **THEN** 元件版本為 `7.2.2-72806`，該元件開始參與比對

#### Scenario: Clearing a known version
- **WHEN** 使用者把 `FW-01` 的 PAN-OS 版本 `11.1.6` 清成空白並儲存
- **THEN** 系統拒絕並在版本欄旁顯示「必填」，元件不變
