# Spec Delta

## MODIFIED Requirements

### Requirement: Editing asset fields
系統 SHALL 可修改 active 資產的 name、category、曝露度、重要性、location、owner、notes，驗證規則與清冊匯入相同；`asset_tag` SHALL 只能依 Renaming an asset 的規則修改。

#### Scenario: Raising exposure
- **WHEN** 將 `NAS-01` 的曝露度由 internal 改為 internet
- **THEN** 資產欄位更新，之後的比對以 internet 曝露度計算該資產命中的風險等級

#### Scenario: Invalid criticality
- **WHEN** 將重要性改為 `5`
- **THEN** 系統拒絕並說明重要性只能是 1–3，資產不變

## ADDED Requirements

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
