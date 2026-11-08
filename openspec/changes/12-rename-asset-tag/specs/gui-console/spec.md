# Spec Delta

## MODIFIED Requirements

### Requirement: GUI asset editing
資產畫面 SHALL 提供「新增資產」與所選 active 資產的「編輯」，以對話框編輯資產欄位（category、曝露度、重要性以下拉選單選擇）與元件（產品以可搜尋的目錄產品清單選擇）；未辨識產品 SHALL 可改為目錄產品或刪除，每筆未辨識產品 SHALL 依「原文與版本、處置、刪除」的順序呈現。每個欄位 SHALL 只有一個標籤，必填欄位 SHALL 以「*」標示。驗證錯誤 SHALL 以介面用語（例如「必填」）顯示在對應欄位旁、MUST NOT 使用清冊的英文欄位名，且不關閉對話框；儲存失敗時 SHALL 保留對話框與輸入內容且資料庫 MUST NOT 變更；按「取消」時資料庫 MUST NOT 變更。對話框關閉時，該對話框造成的驗證錯誤 MUST NOT 留在狀態列。開啟「加入元件」時輸入焦點 SHALL 位於搜尋框，加入元件後 SHALL 移到該元件的版本欄；資產清單上按 Enter SHALL 開啟所選 active 資產的編輯。編輯時「資產編號」SHALL 可修改（依 asset-inventory 的 Renaming an asset），與原編號不同時欄位下方 SHALL 提示舊清冊與已匯出的 VEX 不會對應到新編號；更名與其他修改 SHALL 在同一次儲存中全部寫入或全部不寫入，儲存後資產清單 SHALL 選取新編號。

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

#### Scenario: Renaming in the edit dialog
- **WHEN** 使用者編輯 `NAS-01`，把資產編號改為 `NAS-11`、名稱改為 `nas11` 後儲存
- **THEN** 輸入新編號時欄位下方顯示更名提示；儲存後對話框關閉，資產清單選取 `NAS-11`，名稱為 `nas11`，原有命中仍列在 `NAS-11` 之下

#### Scenario: Renaming to a tag in use
- **WHEN** 使用者編輯 `NAS-01`，把資產編號改為已存在的 `NAS-02`，並修改名稱後儲存
- **THEN** 對話框不關閉，「資產編號」欄位旁說明該編號已被使用（`NAS-02`），`NAS-01` 的編號與名稱都不變
