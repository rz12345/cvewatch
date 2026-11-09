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
