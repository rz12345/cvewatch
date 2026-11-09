# Spec Delta

## ADDED Requirements

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

## MODIFIED Requirements

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

### Requirement: Version change history
元件版本變更時，系統 SHALL 記錄舊版本與變更時間。版本待補的元件填上版本時沒有舊版本，MUST NOT 新增歷史紀錄。

#### Scenario: Firmware upgraded
- **WHEN** 重匯後 `NAS-01` 的 DSM 版本由 `7.2.1-69057` 變為 `7.2.2-72806`
- **THEN** 該元件的歷史中有一筆 `7.2.1-69057` 及變更時間

#### Scenario: Pending version filled
- **WHEN** `NET-HW-003` 的 DSM 元件由版本待補填為 `7.2.2-72806`
- **THEN** 該元件的版本歷史沒有新增紀錄

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

### Requirement: Resolving unresolved products
系統 SHALL 可把資產的一筆未辨識產品改為指定的目錄產品（沿用原版本字串，成為該資產的元件並移除該筆未辨識產品），或直接刪除該筆未辨識產品。

#### Scenario: Mapping to a catalog product
- **WHEN** `PVE-01` 有未辨識產品 `PVE 8.2.4`，使用者將它改為目錄產品 Proxmox VE
- **THEN** `PVE-01` 多一個 Proxmox VE `8.2.4` 元件，未辨識產品清單不再列出這一筆

#### Scenario: Mapping a pending unresolved product
- **WHEN** `NET-HW-004` 有一筆版本待補的未辨識產品「Zyxel ABCD」，使用者將它改為目錄產品 Zyxel 防火牆
- **THEN** `NET-HW-004` 多一個版本待補的 Zyxel 防火牆元件，未辨識產品清單不再列出這一筆
