# Spec Delta

## ADDED Requirements

### Requirement: GUI asset presentation
資產畫面與資產對話框 SHALL 以使用者用語呈現資料：目錄產品 SHALL 以產品目錄的顯示名稱呈現（產品 key MUST NOT 作為主要文字，只可出現在提示中）；類別與曝露度 SHALL 以「英文值（中文名稱）」呈現；套件元件的版本 SHALL 只顯示一次。資產清單 SHALL 以編號、名稱、未處理命中數三個欄位呈現，未處理數大於 0 時 SHALL 醒目顯示，名稱與編號相同時 MUST NOT 重複顯示。資產沒有命中時 SHALL 顯示「沒有命中」而非空白。工具列 SHALL 將「新增資產」、清冊檔案操作（匯入清冊、下載範本、匯出清冊）與「顯示已退役」分組；「退役…」與「永久刪除」SHALL 以警示樣式顯示並與一般操作分開。

#### Scenario: Product shown by display name
- **WHEN** 使用者選取元件為 `proxmox-ve 8.2.2` 的 `PVE-01`
- **THEN** 詳情與編輯對話框顯示「Proxmox VE 8.2.2」，畫面主要文字不出現 `proxmox-ve`

#### Scenario: Long asset name in the list
- **WHEN** 資產 `FW-CORE` 名稱為「資訊所機房核心防火牆主機」、有 1 筆未處理命中
- **THEN** 清單以編號、名稱、未處理數三欄顯示該列，未處理數 `1` 位於自己的欄位且醒目顯示

#### Scenario: Asset without findings
- **WHEN** 使用者選取沒有任何命中的 `PVE-01`
- **THEN** 詳情的命中區塊顯示「沒有命中」

#### Scenario: Bilingual category value
- **WHEN** 資產類別為 `server`
- **THEN** 詳情與類別下拉選單顯示「server（伺服器）」，儲存後資料庫與匯出的值仍為 `server`

## MODIFIED Requirements

### Requirement: GUI asset editing
資產畫面 SHALL 提供「新增資產」與所選 active 資產的「編輯」，以對話框編輯資產欄位（category、曝露度、重要性以下拉選單選擇）與元件（產品以可搜尋的目錄產品清單選擇）；未辨識產品 SHALL 可改為目錄產品或刪除，每筆未辨識產品 SHALL 依「原文與版本、處置、刪除」的順序呈現。每個欄位 SHALL 只有一個標籤，必填欄位 SHALL 以「*」標示。驗證錯誤 SHALL 以介面用語（例如「必填」）顯示在對應欄位旁、MUST NOT 使用清冊的英文欄位名，且不關閉對話框；儲存失敗時 SHALL 保留對話框與輸入內容且資料庫 MUST NOT 變更；按「取消」時資料庫 MUST NOT 變更。對話框關閉時，該對話框造成的驗證錯誤 MUST NOT 留在狀態列。開啟「加入元件」時輸入焦點 SHALL 位於搜尋框，加入元件後 SHALL 移到該元件的版本欄；資產清單上按 Enter SHALL 開啟所選 active 資產的編輯。

#### Scenario: Adding an asset from the window
- **WHEN** 使用者按「新增資產」，填入 `FW-02`、`fw02`、security，加入元件 PAN-OS `11.2.3` 後按「儲存」
- **THEN** 對話框關閉，資產清單出現 `FW-02` 並選取它

#### Scenario: Invalid field keeps the dialog open
- **WHEN** 新增資產時 `asset_tag` 留空就按「儲存」
- **THEN** 對話框不關閉，「資產編號」欄位旁顯示「必填」，畫面不出現「asset_tag」，資料庫不變

#### Scenario: Editing a component version
- **WHEN** 使用者編輯 `FW-01`，把 PAN-OS 版本改為 `11.1.6` 後儲存
- **THEN** 資產詳情顯示新版本，該元件的版本歷史多一筆 `11.1.4-h7`

#### Scenario: Cancelled dialog leaves no error behind
- **WHEN** 新增資產時留空按「儲存」看到錯誤，再按「取消」
- **THEN** 對話框關閉，狀態列不再顯示欄位錯誤

#### Scenario: Keyboard flow when adding a component
- **WHEN** 使用者開啟「加入元件」並輸入 `pan`，選擇 PAN-OS
- **THEN** 搜尋框在開啟時即取得輸入焦點，選擇後焦點移到 PAN-OS 的版本欄，可直接輸入版本

### Requirement: GUI asset retirement and purge
資產畫面 SHALL 提供所選 active 資產的「退役…」（確認後標為退役；確認對話框標題 SHALL 寫明「將 <資產編號> 標為退役」並說明保留紀錄、不再比對、可恢復）與「顯示已退役」切換；資產清單上按 Delete SHALL 開啟所選 active 資產的退役確認。已退役資產 SHALL 提供「恢復」與「永久刪除」。永久刪除 SHALL 先說明將一併移除的元件數與命中數，並要求輸入完全相同的 `asset_tag` 才能確認。

#### Scenario: Deleting from the asset list
- **WHEN** 使用者選取 `NAS-02` 按「退役…」並在標題為「將 NAS-02 標為退役」的確認對話框按「標為退役」
- **THEN** `NAS-02` 從預設清單消失，開啟「顯示已退役」時以退役樣式列出

#### Scenario: Purge confirmation requires the tag
- **WHEN** 使用者對 retired 的 `NAS-02` 按「永久刪除」，在確認欄輸入 `NAS-0`
- **THEN** 「永久刪除」按鈕維持停用；輸入 `NAS-02` 後才可按下
