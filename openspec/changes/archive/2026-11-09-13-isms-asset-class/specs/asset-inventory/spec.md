# Spec Delta

## ADDED Requirements

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

## MODIFIED Requirements

### Requirement: Blank template export
系統 SHALL 提供 `cvewatch asset template <path.xlsx>`，產出含 `assets` 與 `components` 工作表、標準欄名與範例列的空白範本，並 SHALL 附一張 `classes` 工作表列出目前生效的子類別代碼、名稱、描述與 C/I/A 範本值；匯入時 `classes` 工作表 SHALL 被忽略。

#### Scenario: Generating a template
- **WHEN** 使用者執行 `cvewatch asset template D:\inv.xlsx`
- **THEN** 檔案包含 `assets`、`components`、`classes` 三個工作表，`assets` 與 `components` 第一列為標準欄名

#### Scenario: Class sheet follows configuration
- **WHEN** 設定檔新增子類別 `HW12`，使用者產出範本
- **THEN** `classes` 工作表包含 `HW12` 與其名稱、範本值

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

## REMOVED Requirements

### Requirement: Exposure and criticality attributes
**Reason**: 重要性（1–3）改為 ISMS 的 C/I/A 與資產價值（1–4），category 改為 ISMS 子類別；曝露度規則移到「Exposure attribute」，類別規則移到「Asset class」與「Legacy category values」。
**Migration**: 升級時依「Migrating existing assets」自動轉換；舊清冊的 category 值依「Legacy category values」轉換，`criticality` 欄依「Legacy criticality column」忽略。
