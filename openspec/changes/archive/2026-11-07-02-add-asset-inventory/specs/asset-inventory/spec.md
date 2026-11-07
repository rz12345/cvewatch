# Spec Delta

## Purpose

管理 cvewatch 所追蹤的設備與其上的軟體/韌體元件，以使用者維護的 Excel/CSV 清冊為唯一來源，匯入可重複且可預覽。

## ADDED Requirements

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

### Requirement: Exposure and criticality attributes
每個資產 SHALL 有曝露度（internet、dmz、internal、mgmt 之一，預設 internal）與重要性（1–3，預設 2）。

#### Scenario: Invalid exposure value
- **WHEN** 某列曝露度填寫 `public`
- **THEN** 匯入中止並指出該列曝露度不是允許的值

#### Scenario: Invalid category value
- **WHEN** 某列 category 填寫 `printer`（不屬於 network、security、virtualization、server、storage、host）
- **THEN** 匯入中止並指出該列 category 不是允許的值

### Requirement: Blank template export
系統 SHALL 提供 `cvewatch asset template <path.xlsx>`，產出含 `assets` 與 `components` 工作表、標準欄名與範例列的空白範本。

#### Scenario: Generating a template
- **WHEN** 使用者執行 `cvewatch asset template D:\inv.xlsx`
- **THEN** 檔案包含兩個工作表，第一列為標準欄名
