## MODIFIED Requirements

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

## ADDED Requirements

### Requirement: Changing a system code
資產說明的代碼 SHALL 可修改，名稱可同時修改。新代碼 SHALL 依 Asset system 驗證，並 MUST NOT 與其他資產說明的代碼相同、MUST NOT 等於其他資產說明的名稱；與原代碼相同時視為只改名稱。代碼改變時，系統 SHALL 在同一個交易內：把所有歸屬的資產（含 retired）改為歸屬新代碼；把這些資產 ISMS 編號的系統代碼改為新代碼；資產編號等於舊 ISMS 編號（不分大小寫）的資產，依 Renaming an asset 的保留規則把資產編號改為新 ISMS 編號。任何一個新資產編號已被其他資產（含 retired，不分大小寫）使用時，系統 SHALL 拒絕並列出衝突的編號，資料庫 MUST NOT 變更。系統 SHALL 可在寫入前產生預覽，列出受影響的資產數與每筆 ISMS 編號、資產編號的變更。

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
