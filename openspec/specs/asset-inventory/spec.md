# asset-inventory Specification

## Purpose

管理 cvewatch 所追蹤的設備與其上的軟體/韌體元件：以 Excel/CSV 清冊匯入（可重複且可預覽）與畫面編修維護資產，資料庫為目前狀態、清冊為交換格式（可匯出後再匯入）。

## Requirements

### Requirement: Asset and component model
系統 SHALL 以 `asset_tag` 唯一識別資產，每個資產 SHALL 可擁有多個元件，每個元件記錄產品與原始版本字串。

#### Scenario: Server with several tracked components
- **WHEN** 清冊中 `R6515-01` 的主要產品為 ESXi 8.0 U3c，components 工作表另列 iDRAC 7.10.30.00 與 BIOS 2.16.0
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
系統 SHALL 提供 `--dry-run`，只輸出新增、更新、版本變更、未辨識產品、清冊中消失的資產數量與明細，不寫入資料庫。

#### Scenario: Previewing a version change
- **WHEN** Excel 將 `FW-01` 的版本從 `11.1.4-h7` 改為 `11.1.6`，以 `--dry-run` 匯入
- **THEN** 輸出顯示 `FW-01` 版本 `11.1.4-h7 → 11.1.6`，資料庫內容不變

### Requirement: Atomic import with row validation
匯入 SHALL 在單一交易內完成；任何一列缺少必填欄位時 SHALL 中止整批並列出錯誤列號。

#### Scenario: Missing required field
- **WHEN** 第 12 列沒有 `version`
- **THEN** 匯入中止，錯誤訊息包含「第 12 列：缺少 version」，資料庫維持匯入前狀態

### Requirement: Retiring assets
清冊中不再出現的資產 SHALL 只在使用 `--prune` 時標為退役；退役資產 SHALL 保留歷史紀錄且不再參與比對。

#### Scenario: Asset removed from spreadsheet without prune
- **WHEN** `NAS-02` 已不在 Excel 中，匯入時未加 `--prune`
- **THEN** `NAS-02` 維持 active，匯入報表列出它為「清冊中消失」

#### Scenario: Asset removed with prune
- **WHEN** 同上但加上 `--prune`
- **THEN** `NAS-02` 狀態變為 retired 並記錄退役時間

### Requirement: Version change history
元件版本變更時，系統 SHALL 記錄舊版本與變更時間。

#### Scenario: Firmware upgraded
- **WHEN** 重匯後 `NAS-01` 的 DSM 版本由 `7.2.1-69057` 變為 `7.2.2-72806`
- **THEN** 該元件的歷史中有一筆 `7.2.1-69057` 及變更時間

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
設定檔 SHALL 可覆寫內建子類別的名稱、描述與 C/I/A 範本值，並 SHALL 可新增子類別；新增的代碼 MUST 為 `HW` 或 `SW` 加兩位數字，否則設定檔載入失敗並指出該代碼。子類別名稱 MUST NOT 與其他子類別重複，也 MUST NOT 是子類別代碼的形式。修改範本值 MUST NOT 改變既有資產的 C/I/A。

#### Scenario: Raising a template value
- **WHEN** 設定檔將 `HW05` 的機密性範本值改為 3，之後匯入一筆 C/I/A 留空的 `HW05` 資產
- **THEN** 新資產的機密性為 3，既有 `HW05` 資產的機密性不變

#### Scenario: Adding a class
- **WHEN** 設定檔新增 `HW12`，名稱「實驗儀器」、範本值 2/3/3
- **THEN** 清冊可使用 `HW12` 或「實驗儀器」作為 category

#### Scenario: Class code outside HW and SW
- **WHEN** 設定檔新增代碼 `PE06`
- **THEN** 設定檔載入失敗，錯誤訊息指出 `PE06` 必須以 HW 或 SW 開頭並接兩位數字

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
每個資產 SHALL 可有選填的 ISMS 資產編號，格式 SHALL 為三碼英數字系統代碼、`-`、兩碼大類、`-`、三碼數字（例如 `ALL-HW-001`），大類 MUST 與資產的子類別大類相同。ISMS 編號 MUST NOT 與其他資產（含 retired，不分大小寫）重複；留空表示沒有 ISMS 編號。清冊沒有 `isms_id` 欄時，既有資產的 ISMS 編號 SHALL 保留，匯入報表 SHALL 給出一次提醒。`asset_tag` 仍是資產的識別。

#### Scenario: Valid ISMS ID
- **WHEN** 類別 `HW03` 的 `FW-01` 填寫 ISMS 編號 `ALL-HW-003`
- **THEN** 資產儲存成功，`asset_tag` 仍為 `FW-01`

#### Scenario: Major class mismatch
- **WHEN** 類別 `HW03` 的資產填寫 ISMS 編號 `ALL-SW-003`
- **THEN** 匯入或編修被拒絕，並指出 ISMS 編號的大類 SW 與資產類別 HW03 不符

#### Scenario: Malformed ISMS ID
- **WHEN** 某列 ISMS 編號填寫 `ALLHW001`
- **THEN** 匯入中止並指出該列 ISMS 編號格式應為「系統代碼-大類-流水號」，例如 `ALL-HW-001`

#### Scenario: Duplicate ISMS ID
- **WHEN** `NAS-01` 填寫 `all-hw-005`，而 retired 的 `NAS-02` 已使用 `ALL-HW-005`
- **THEN** 系統拒絕並指出該 ISMS 編號已被 `NAS-02` 使用

#### Scenario: Old inventory without ISMS column
- **WHEN** `FW-01` 的 ISMS 編號為 `ALL-HW-003`，使用者重匯沒有 `isms_id` 欄的舊清冊
- **THEN** `FW-01` 的 ISMS 編號仍為 `ALL-HW-003`，報表有一筆「清冊沒有 isms_id 欄，保留資料庫中既有資產的 ISMS 編號」提醒

### Requirement: Migrating existing assets
升級到此版本時，系統 SHALL 依 Legacy category values 的對照表轉換既有資產（含 retired）的類別，以轉換後子類別的內建範本值設定 C/I/A，並在 notes 末尾附記「舊重要性：N」。資產的元件、版本歷史、命中與處理紀錄 MUST NOT 改變。

#### Scenario: Upgrading a database with a firewall
- **WHEN** 升級前 `FW-01` 的 category 為 `security`、重要性 3、notes 為「機房 A 主防火牆」
- **THEN** 升級後 `FW-01` 類別為 `HW03`、C/I/A 為 2/3/4、notes 為「機房 A 主防火牆；舊重要性：3」，命中數與升級前相同

### Requirement: Blank template export
系統 SHALL 提供 `cvewatch asset template <path.xlsx>`，產出含 `assets` 與 `components` 工作表、標準欄名與範例列的空白範本，並 SHALL 附一張 `classes` 工作表列出目前生效的子類別代碼、名稱、描述與 C/I/A 範本值；匯入時 `classes` 工作表 SHALL 被忽略。

#### Scenario: Generating a template
- **WHEN** 使用者執行 `cvewatch asset template D:\inv.xlsx`
- **THEN** 檔案包含 `assets`、`components`、`classes` 三個工作表，`assets` 與 `components` 第一列為標準欄名

#### Scenario: Class sheet follows configuration
- **WHEN** 設定檔新增子類別 `HW12`，使用者產出範本
- **THEN** `classes` 工作表包含 `HW12` 與其名稱、範本值

### Requirement: Inventory export
系統 SHALL 提供 `cvewatch asset export <path.xlsx>`，把所有 active 資產匯出為含 `assets` 與 `components` 工作表、標準欄名的 Excel：每個資產一列 `assets`（第一個元件寫在該列的 product / version），其餘清冊元件與未辨識產品列在 `components`。匯出 MUST NOT 包含 retired 資產與主機套件元件；目的檔已存在時 SHALL 拒絕且不覆蓋。

#### Scenario: Round trip without changes
- **WHEN** 使用者執行 `cvewatch asset export D:\inv.xlsx` 後，立刻以 `cvewatch asset import D:\inv.xlsx --dry-run` 匯入同一檔案
- **THEN** 預覽結果為「新增 0、更新 0」、沒有版本變更與清冊中消失的資產

#### Scenario: Unresolved product is kept
- **WHEN** `PVE-01` 有一筆未辨識產品 `Proxmox Backup 3.2`，執行匯出
- **THEN** `components` 工作表有一列 `PVE-01`、`Proxmox Backup`、`3.2` 原文，重匯後該資產仍列出同一筆未辨識產品

#### Scenario: Retired assets and packages are left out
- **WHEN** `NAS-02` 為 retired，`WEB-01` 另有 dpkg 匯入的套件元件，執行匯出
- **THEN** 檔案中沒有 `NAS-02`，`WEB-01` 只匯出清冊元件，不含套件

#### Scenario: Export target exists
- **WHEN** `D:\inv.xlsx` 已存在
- **THEN** 指令以錯誤結束並說明目的檔已存在，原檔內容不變

### Requirement: Creating an asset
系統 SHALL 可不經清冊新增一個 active 資產；欄位驗證（必填欄位、類別、曝露度、C/I/A、ISMS 編號的允許值與預設值）SHALL 與清冊匯入相同，且 SHALL 至少有一個以產品目錄產品與版本指定的元件。`asset_tag` 已被任何資產（含 retired，不分大小寫）使用時 SHALL 拒絕。

#### Scenario: Adding a new firewall
- **WHEN** 新增 `FW-02`，名稱 `fw02`、類別 `HW03`、元件 PAN-OS `11.2.3`，未指定曝露度與 C/I/A
- **THEN** 系統建立 active 資產 `FW-02`（曝露度 internal、C/I/A 2/3/4、資產價值 4）與一個元件

#### Scenario: Tag used by a retired asset
- **WHEN** 新增 `NAS-02`，而 `NAS-02` 已存在且為 retired
- **THEN** 系統拒絕並說明該編號屬於已退役資產，可改為恢復該資產

#### Scenario: No component given
- **WHEN** 新增資產時沒有任何元件
- **THEN** 系統拒絕並說明至少需要一個元件，資料庫不變

### Requirement: Editing asset fields
系統 SHALL 可修改 active 資產的 name、類別、曝露度、機密性、完整性、可用性、ISMS 編號、location、owner、notes，驗證規則與清冊匯入相同；修改類別 MUST NOT 自動改變已設定的 C/I/A。`asset_tag` SHALL 只能依 Renaming an asset 的規則修改。

#### Scenario: Raising exposure
- **WHEN** 將 `NAS-01` 的曝露度由 internal 改為 internet
- **THEN** 資產欄位更新，之後的比對以 internet 曝露度計算該資產命中的風險等級

#### Scenario: Invalid criticality
- **WHEN** 將機密性改為 `5`
- **THEN** 系統拒絕並說明機密性只能是 1–4，資產不變

#### Scenario: Changing class keeps ratings
- **WHEN** `PVE-01` 類別為 `HW01`、C/I/A 為 3/3/4，使用者把類別改為 `HW05`
- **THEN** 類別變為 `HW05`，C/I/A 仍為 3/3/4

### Requirement: Editing components
系統 SHALL 可對 active 資產新增目錄產品元件、修改元件版本、移除元件；一次儲存的多項編修 SHALL 在單一交易內完成，任何一項失敗時 MUST NOT 寫入其中任何一項。同一資產 MUST NOT 有兩個相同產品的元件；版本修改 SHALL 依 Version change history 記錄舊版本；移除後資產 SHALL 仍至少保留一個清冊元件或未辨識產品。主機套件元件 MUST NOT 被編修。

#### Scenario: Upgrading firmware on screen
- **WHEN** 將 `FW-01` 的 PAN-OS 版本由 `11.1.4-h7` 改為 `11.1.6`
- **THEN** 元件版本為 `11.1.6`，版本歷史中有一筆 `11.1.4-h7` 及變更時間

#### Scenario: Adding a duplicate product
- **WHEN** `R6515-01` 已有 iDRAC9 元件，再新增 iDRAC9 元件
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
