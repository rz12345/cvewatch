## ADDED Requirements

### Requirement: GUI component groups
資產詳情與資產編輯對話框的元件區塊 SHALL 依 asset-inventory 的 Component grouping by role 分組，每組以角色中文名稱作為標題，元件列在標題之下；詳情中各元件的版本歷史 SHALL 緊接在該元件之下。編輯對話框中，以「加入元件」新加入的元件 SHALL 出現在其角色的組內，加入後焦點仍 SHALL 移到該元件的版本欄；元件的欄位錯誤 SHALL 顯示在該元件列之下；標為移除的元件 SHALL 留在原組並可復原；套件元件 SHALL 在「套件」組中維持唯讀。

#### Scenario: Grouped components in the detail pane
- **WHEN** 使用者選取有 Ubuntu、MySQL、iDRAC9 三個清冊元件與一個套件元件的 `SRV-01`
- **THEN** 詳情的元件區塊依序顯示「BMC」「作業系統」「資料庫管理系統」「套件」四個標題，iDRAC9、Ubuntu、MySQL 與該套件分別列在對應標題之下

#### Scenario: Added component joins its group
- **WHEN** 使用者在 `SRV-01` 的編輯對話框以「加入元件」加入 nginx
- **THEN** 對話框在「資料庫管理系統」組之後出現「網站服務系統」組，nginx 的版本欄在該組內並取得焦點，其他元件已輸入的版本不變

#### Scenario: Package group in the edit dialog
- **WHEN** 使用者開啟有套件元件的 `SRV-01` 的編輯對話框
- **THEN** 套件元件列在「套件」組，沒有版本欄與移除按鈕，並說明由套件清單管理
