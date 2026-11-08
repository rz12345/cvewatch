# Spec Delta

## ADDED Requirements

### Requirement: GUI asset renaming
資產畫面的「編輯」對話框 SHALL 可修改「資產編號」（依 asset-inventory 的 Renaming an asset）；與原編號不同時欄位下方 SHALL 提示舊清冊與已匯出的 VEX 不會對應到新編號。新編號已被其他資產使用時，錯誤 SHALL 顯示在「資產編號」欄位旁且對話框不關閉。更名與其他修改 SHALL 在同一次儲存中全部寫入或全部不寫入，儲存後資產清單 SHALL 選取新編號。

#### Scenario: Renaming in the edit dialog
- **WHEN** 使用者編輯 `NAS-01`，把資產編號改為 `NAS-11`、名稱改為 `nas11` 後儲存
- **THEN** 輸入新編號時欄位下方顯示更名提示；儲存後對話框關閉，資產清單選取 `NAS-11`，名稱為 `nas11`，原有命中仍列在 `NAS-11` 之下

#### Scenario: Renaming to a tag in use
- **WHEN** 使用者編輯 `NAS-01`，把資產編號改為已存在的 `NAS-02`，並修改名稱後儲存
- **THEN** 對話框不關閉，「資產編號」欄位旁說明該編號已被使用（`NAS-02`），`NAS-01` 的編號與名稱都不變
