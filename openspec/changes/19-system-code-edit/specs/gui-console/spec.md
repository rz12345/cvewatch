## MODIFIED Requirements

### Requirement: GUI asset system management
資產畫面的工具列 SHALL 提供「新增資產說明」，開啟對話框列出所有資產說明（代碼、名稱、歸屬的資產數，含 retired），並 SHALL 可新增資產說明、編輯與刪除。每個資產說明的「編輯」SHALL 可在同一列修改代碼與名稱後一次儲存。驗證錯誤 SHALL 顯示在對應欄位旁且不關閉對話框；仍有資產歸屬的資產說明，其「刪除」SHALL 停用並以提示說明歸屬的資產數。規則 SHALL 與 asset-inventory 的 Asset system 與 Changing a system code 相同。

#### Scenario: Adding a system from the window
- **WHEN** 使用者按「新增資產說明」，新增代碼 `net`、名稱「網路服務」後儲存
- **THEN** 清單出現「NET 網路服務」，資產數為 0

#### Scenario: Duplicate name in the dialog
- **WHEN** 已有「網路服務」，使用者新增另一個名稱也是「網路服務」的資產說明
- **THEN** 對話框不關閉，名稱欄位旁說明該名稱已被 `NET` 使用，資料庫不變

#### Scenario: System still in use
- **WHEN** 資產說明 `NET` 有 3 個資產歸屬
- **THEN** `NET` 的「刪除」停用，滑鼠提示說明仍有 3 個資產歸屬

#### Scenario: Editing a code without assets
- **WHEN** 沒有資產歸屬的 `LAB`，使用者按「編輯」把代碼改為 `tst` 後儲存
- **THEN** 不顯示確認，清單出現 `TST`、不再有 `LAB`

#### Scenario: Code already used in the dialog
- **WHEN** 使用者編輯 `NET`，把代碼改為已存在的 `SYS` 後儲存
- **THEN** 對話框不關閉，代碼欄位旁說明 `SYS` 已被使用，資料庫不變

## ADDED Requirements

### Requirement: GUI system code change confirmation
編輯資產說明時代碼改變、且有資產（含 retired）歸屬，按儲存後 SHALL 先顯示確認，列出受影響的資產數、每筆 ISMS 編號與資產編號的變更，並提醒舊清冊、已匯出的 VEX 與尚未更新的 ISMS 清冊不會對應、建議重新匯出清冊。按「確定修改」才寫入；按「返回」或 Esc SHALL 回到編輯列並保留輸入，資料庫 MUST NOT 變更。新資產編號衝突時 SHALL 不顯示確認，改在代碼欄位旁列出衝突的編號。寫入後，選取中的資產若被改了編號，資產清單 SHALL 選取其新編號。

#### Scenario: Confirming a code change
- **WHEN** `NET` 有 3 個資產歸屬，使用者把代碼改為 `NWS` 後按儲存
- **THEN** 顯示確認，列出 3 個受影響的資產與 `NET-HW-001 → NWS-HW-001` 等變更；按「確定修改」後清單出現 `NWS`，資產詳情顯示新的 ISMS 編號

#### Scenario: Going back from the confirmation
- **WHEN** 確認畫面出現後，使用者按「返回」
- **THEN** 回到 `NET` 的編輯列，代碼欄仍是 `NWS`，資料庫不變

#### Scenario: Selected asset follows its new tag
- **WHEN** 資產清單選取 `NET-HW-001`，使用者確認把 `NET` 改為 `NWS`
- **THEN** 資產清單選取 `NWS-HW-001`，詳情顯示該資產

#### Scenario: Conflicting asset tag in the dialog
- **WHEN** 使用者把 `NET` 改為 `NWS`，而 `NWS-HW-001` 已被其他資產使用
- **THEN** 不顯示確認，代碼欄位旁列出衝突的編號 `NWS-HW-001`，資料庫不變
