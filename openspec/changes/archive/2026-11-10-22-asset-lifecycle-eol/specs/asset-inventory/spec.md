## MODIFIED Requirements

### Requirement: Blank template export
系統 SHALL 提供 `cvewatch asset template <path.xlsx>`，產出含 `assets`、`components`、`systems`、`ips` 與 `eol` 工作表、標準欄名與範例列的空白範本（`eol` 範例列的資產與產品沿用 `assets` 的範例列，範本本身 SHALL 可匯入），並 SHALL 附一張 `classes` 工作表列出目前生效的子類別代碼、名稱、描述、C/I/A 範本值與是否含個人資料的預設值；匯入時 `classes` 工作表 SHALL 被忽略。

#### Scenario: Generating a template
- **WHEN** 使用者執行 `cvewatch asset template D:\inv.xlsx`
- **THEN** 檔案包含 `assets`、`components`、`systems`、`ips`、`eol`、`classes` 六個工作表，`assets`、`components`、`systems`、`ips` 與 `eol` 第一列為標準欄名，`assets` 的表頭含 `brand`、`model`、`serial_number`、`property_number`、`hostname`、`management_url`、`warranty_end`、`support_end`、`license_end`

#### Scenario: Class sheet follows configuration
- **WHEN** 設定檔新增子類別 `HW12`，使用者產出範本
- **THEN** `classes` 工作表包含 `HW12` 與其名稱、範本值

### Requirement: Inventory export
系統 SHALL 提供 `cvewatch asset export <path.xlsx>`，把所有 active 資產匯出為含 `assets`、`components`、`systems`、`ips` 與 `eol` 工作表、標準欄名的 Excel：每個資產一列 `assets`（含識別欄位與生命週期日期，日期寫 `YYYY-MM-DD` 文字；第一個元件寫在該列的 product / version），其餘清冊元件與未辨識產品列在 `components`，`systems` 列出所有資產說明，`ips` 每筆 IP 一列，`eol` 每筆 EoL 覆寫一列（硬體覆寫的 `product` 空白，元件覆寫寫產品的顯示名稱）。匯出 MUST NOT 包含 retired 資產與主機套件元件；目的檔已存在時 SHALL 拒絕且不覆蓋。

#### Scenario: Round trip without changes
- **WHEN** 使用者執行 `cvewatch asset export D:\inv.xlsx` 後，立刻以 `cvewatch asset import D:\inv.xlsx --dry-run` 匯入同一檔案
- **THEN** 預覽結果為「新增 0、更新 0」、沒有版本變更、資產說明變更、IP 變更、EoL 覆寫變更與清冊中消失的資產

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

#### Scenario: Addresses are exported
- **WHEN** `SRV-01` 有 IP `192.0.2.10`（用途「管理」）與 `2001:db8::10`，`NAS-01` 沒有 IP，執行匯出
- **THEN** `ips` 工作表有 `SRV-01` 的兩列（依儲存順序、`purpose` 分別為「管理」與空白），沒有 `NAS-01` 的列

## ADDED Requirements

### Requirement: Lifecycle date fields
資產 SHALL 有三個選填日期：保固到期 `warranty_end`、維護合約到期 `support_end`、軟體授權到期 `license_end`。日期 SHALL 接受 `YYYY-MM-DD`、`YYYY/MM/DD` 與試算表的日期儲存格，並以 `YYYY-MM-DD` 儲存；其他寫法或不存在的日期 SHALL 被拒絕並指出欄位與原文。清冊匯入、新增資產與編修資產 SHALL 使用相同的驗證。升級到此版本時，既有資產（含 retired）的三個日期 SHALL 為未填，元件、命中與處理紀錄 MUST NOT 改變。

#### Scenario: Recording warranty and support dates
- **WHEN** 匯入的清冊中 `SRV-01` 的 `warranty_end` 為 `2027/12/31`、`support_end` 為 `2027-06-30`、`license_end` 空白
- **THEN** `SRV-01` 的保固到期為 2027-12-31、維護合約到期為 2027-06-30、軟體授權到期未填

#### Scenario: Spreadsheet date cell
- **WHEN** `warranty_end` 欄是 Excel 日期格式的儲存格，顯示為 2027/12/31
- **THEN** 保固到期儲存為 2027-12-31

#### Scenario: Invalid date
- **WHEN** 匯入的清冊中 `SRV-01` 的 `support_end` 為 `2027-02-30`
- **THEN** 匯入中止並指出該列 `support_end` 的「2027-02-30」不是有效日期，資料庫不變

#### Scenario: Upgrading keeps assets unchanged
- **WHEN** 升級前 `FW-EDGE` 有 2 筆命中
- **THEN** 升級後 `FW-EDGE` 的三個日期為未填，命中數仍為 2

### Requirement: EoL overrides
資產 SHALL 可手動覆寫硬體 EoL，清冊元件 SHALL 可手動覆寫軟體 EoL；每筆覆寫 SHALL 有 EoL 日期（規則同 Lifecycle date fields）與非空白的來源說明。同一資產的硬體、同一元件 MUST NOT 有兩筆覆寫。元件被移除時其覆寫 SHALL 一併移除；版本修改 MUST NOT 移除覆寫。

#### Scenario: Override without a source
- **WHEN** 為 `SW-CORE` 覆寫硬體 EoL 2027-06-30，來源空白
- **THEN** 系統拒絕並說明覆寫必須填寫來源

#### Scenario: Override follows a version change
- **WHEN** `SRV-01` 的 ESXi 元件有軟體 EoL 覆寫，使用者把 ESXi 版本由 `8.0 U2` 改為 `8.0 U3`
- **THEN** 覆寫保留

### Requirement: Lifecycle columns and EoL sheet
清冊 `assets` 工作表 SHALL 可包含 `warranty_end`、`support_end`、`license_end` 欄；清冊 SHALL 可包含選填的 `eol` 工作表（`asset_tag`、`product`、`eol`、`source`），`product` 空白表示硬體覆寫，否則為該資產清冊元件的產品（依產品目錄別名解析）。範本與匯出 SHALL 包含這三欄與 `eol` 工作表（匯出寫出 active 資產的所有覆寫）。清冊缺少某個日期欄時既有值 SHALL 保留；沒有 `eol` 工作表時既有覆寫 SHALL 保留；兩者都 SHALL 在匯入報表以一則提醒列出。

#### Scenario: Importing overrides
- **WHEN** 清冊的 `eol` 工作表有 `SW-CORE`、空白、`2027-06-30`、「原廠延長支援函」與 `SRV-01`、`ESXi`、`2028-12-31`、「延長支援合約」
- **THEN** `SW-CORE` 有硬體 EoL 覆寫，`SRV-01` 的 ESXi 元件有軟體 EoL 覆寫

#### Scenario: Override for a product the asset does not have
- **WHEN** `eol` 工作表有 `FW-EDGE`、`Synology DSM`，而 `FW-EDGE` 沒有 DSM 元件
- **THEN** 匯入中止並指出 `eol` 工作表該列的資產沒有此產品的元件，資料庫不變

#### Scenario: Re-importing an inventory without lifecycle data
- **WHEN** `SRV-01` 有保固到期與硬體覆寫，使用者重匯沒有 `warranty_end` 欄、也沒有 `eol` 工作表的舊清冊
- **THEN** `SRV-01` 的保固到期與覆寫不變，報表有一則提醒列出缺少 `warranty_end` 欄與 `eol` 工作表

#### Scenario: Overrides removed in the sheet
- **WHEN** 清冊有 `eol` 工作表，但其中沒有 `SW-CORE` 的列，而資料庫中 `SW-CORE` 有硬體覆寫
- **THEN** 匯入後 `SW-CORE` 沒有硬體覆寫，預覽列出該覆寫將移除

#### Scenario: Round trip with lifecycle data
- **WHEN** 資產有生命週期日期與覆寫，使用者匯出後立刻以 `--dry-run` 匯入同一檔案
- **THEN** 預覽結果為「新增 0、更新 0」，沒有覆寫變更

### Requirement: Editing lifecycle fields
新增資產與編修 active 資產時，系統 SHALL 可設定或清除三個生命週期日期、硬體 EoL 覆寫與各清冊元件的軟體 EoL 覆寫，驗證規則 SHALL 與清冊匯入相同；這些欄位與其他欄位一起儲存時 SHALL 在同一交易內完成。修改生命週期欄位 MUST NOT 觸發重新比對。

#### Scenario: Setting a support date on screen
- **WHEN** 將 `SRV-01` 的維護合約到期由未填改為 2027-06-30 並儲存
- **THEN** `SRV-01` 的維護合約到期為 2027-06-30，命中不重新比對

#### Scenario: Clearing a hardware override
- **WHEN** 清除 `SW-CORE` 的硬體 EoL 覆寫並儲存
- **THEN** `SW-CORE` 的硬體 EoL 改由目錄的型號清單決定
