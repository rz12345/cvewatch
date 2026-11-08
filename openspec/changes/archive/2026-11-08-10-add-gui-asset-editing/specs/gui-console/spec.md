# Spec Delta

## ADDED Requirements

### Requirement: GUI template download
資產畫面 SHALL 提供「下載範本」，以 Windows 另存新檔對話框選擇路徑（預設文件資料夾、檔名 `cvewatch-inventory-template.xlsx`），寫出與 `cvewatch asset template` 相同的空白範本；MUST NOT 覆蓋既有檔案，完成後 SHALL 顯示檔案路徑。

#### Scenario: Saving the template
- **WHEN** 使用者按「下載範本」並在對話框確認預設檔名
- **THEN** 文件資料夾中產生含 `assets`、`components` 工作表與範例列的範本，視窗顯示其路徑

#### Scenario: Template target already exists
- **WHEN** 使用者在對話框選擇已存在的檔案並確認取代
- **THEN** 系統不覆蓋該檔，顯示檔案已存在的錯誤，原檔內容不變

### Requirement: GUI inventory export
資產畫面 SHALL 提供「匯出清冊」，以 Windows 另存新檔對話框選擇路徑（預設文件資料夾、檔名含當天日期），匯出內容 SHALL 與 `cvewatch asset export` 相同；MUST NOT 覆蓋既有檔案，完成後 SHALL 顯示路徑、匯出的資產數與「此檔未加密」提示。

#### Scenario: Exporting the inventory
- **WHEN** 有 12 個 active 資產，使用者按「匯出清冊」並確認預設檔名
- **THEN** 文件資料夾中產生清冊檔，視窗顯示路徑、「已匯出 12 個資產」與「此檔未加密」提示

### Requirement: GUI asset editing
資產畫面 SHALL 提供「新增資產」與所選 active 資產的「編輯」，以對話框編輯資產欄位（category、曝露度、重要性以下拉選單選擇）與元件（產品以可搜尋的目錄產品清單選擇）；未辨識產品 SHALL 可改為目錄產品或刪除。驗證錯誤 SHALL 顯示在對應欄位旁且不關閉對話框；儲存失敗時 SHALL 保留對話框與輸入內容且資料庫 MUST NOT 變更；按「取消」時資料庫 MUST NOT 變更。

#### Scenario: Adding an asset from the window
- **WHEN** 使用者按「新增資產」，填入 `FW-02`、`fw02`、security，加入元件 PAN-OS `11.2.3` 後按「儲存」
- **THEN** 對話框關閉，資產清單出現 `FW-02` 並選取它

#### Scenario: Invalid field keeps the dialog open
- **WHEN** 新增資產時 `asset_tag` 留空就按「儲存」
- **THEN** 對話框不關閉，`asset_tag` 欄位旁顯示必填錯誤，資料庫不變

#### Scenario: Editing a component version
- **WHEN** 使用者編輯 `FW-01`，把 PAN-OS 版本改為 `11.1.6` 後儲存
- **THEN** 資產詳情顯示新版本，該元件的版本歷史多一筆 `11.1.4-h7`

### Requirement: GUI asset retirement and purge
資產畫面 SHALL 提供所選 active 資產的「刪除」（確認後標為退役）與「顯示已退役」切換；已退役資產 SHALL 提供「恢復」與「永久刪除」。永久刪除 SHALL 先說明將一併移除的元件數與命中數，並要求輸入完全相同的 `asset_tag` 才能確認。

#### Scenario: Deleting from the asset list
- **WHEN** 使用者選取 `NAS-02` 按「刪除」並在確認對話框按「標為退役」
- **THEN** `NAS-02` 從預設清單消失，開啟「顯示已退役」時以退役樣式列出

#### Scenario: Purge confirmation requires the tag
- **WHEN** 使用者對 retired 的 `NAS-02` 按「永久刪除」，在確認欄輸入 `NAS-0`
- **THEN** 「永久刪除」按鈕維持停用；輸入 `NAS-02` 後才可按下

### Requirement: GUI matching after edits
GUI 的資產編修、退役、恢復與永久刪除寫入後 SHALL 在背景執行比對（不凍結視窗），完成後重新載入資料並在狀態列顯示結果；找不到同資料夾的 `cvewatch.exe` 或比對失敗時，編修 SHALL 保留並以警告提示執行 `cvewatch match`。

#### Scenario: Matching runs in the background
- **WHEN** 使用者儲存 `FW-01` 的版本變更
- **THEN** 狀態列顯示「比對中」，視窗仍可操作；比對完成後命中清單重新載入

#### Scenario: Companion missing
- **WHEN** 同資料夾沒有 `cvewatch.exe`，使用者儲存編修
- **THEN** 編修已寫入，狀態列以警告顯示尚未比對，請執行 `cvewatch match`

### Requirement: GUI import overwrite reminder
清冊匯入預覽 SHALL 顯示提醒：匯入會以檔案內容覆寫資產欄位與清冊元件（檔案中沒有的清冊元件會被移除），在視窗中修改過資產時建議先「匯出清冊」再編輯該檔。

#### Scenario: Previewing after on-screen edits
- **WHEN** 使用者在視窗中修改過 `FW-01` 後拖入舊的清冊檔
- **THEN** 預覽顯示覆寫提醒與 `FW-01` 的版本變更明細，按「匯入」前資料庫不變
