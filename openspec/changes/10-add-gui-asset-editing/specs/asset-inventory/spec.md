# Spec Delta

## ADDED Requirements

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
系統 SHALL 可不經清冊新增一個 active 資產；欄位驗證（必填欄位、category、曝露度、重要性的允許值與預設值）SHALL 與清冊匯入相同，且 SHALL 至少有一個以產品目錄產品與版本指定的元件。`asset_tag` 已被任何資產（含 retired，不分大小寫）使用時 SHALL 拒絕。

#### Scenario: Adding a new firewall
- **WHEN** 新增 `FW-02`，名稱 `fw02`、category `security`、元件 PAN-OS `11.2.3`，未指定曝露度與重要性
- **THEN** 系統建立 active 資產 `FW-02`（曝露度 internal、重要性 2）與一個元件

#### Scenario: Tag used by a retired asset
- **WHEN** 新增 `NAS-02`，而 `NAS-02` 已存在且為 retired
- **THEN** 系統拒絕並說明該編號屬於已退役資產，可改為恢復該資產

#### Scenario: No component given
- **WHEN** 新增資產時沒有任何元件
- **THEN** 系統拒絕並說明至少需要一個元件，資料庫不變

### Requirement: Editing asset fields
系統 SHALL 可修改 active 資產的 name、category、曝露度、重要性、location、owner、notes，驗證規則與清冊匯入相同；`asset_tag` MUST NOT 可修改。

#### Scenario: Raising exposure
- **WHEN** 將 `NAS-01` 的曝露度由 internal 改為 internet
- **THEN** 資產欄位更新，之後的比對以 internet 曝露度計算該資產命中的風險等級

#### Scenario: Invalid criticality
- **WHEN** 將重要性改為 `5`
- **THEN** 系統拒絕並說明重要性只能是 1–3，資產不變

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
