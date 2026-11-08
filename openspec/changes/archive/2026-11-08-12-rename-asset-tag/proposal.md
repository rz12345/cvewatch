# Proposal

## Why

資產編號（`asset_tag`）建立後無法修改（10 的規格明定 MUST NOT）。設備重新編號、財產編號更正或打錯字時，目前只能新增一個新資產再退役或刪除舊資產，舊資產的命中、處理紀錄與版本歷史都無法帶到新編號。資料庫內部都以整數 id 關聯資產，改名只需更新一欄，因此改為允許更名。

## What Changes

- core 新增「更名」操作：把 active 資產的 `asset_tag` 改為新編號，元件、版本歷史、未辨識產品、命中與處理紀錄都保留在同一個資產上
  - 新編號的驗證與匯入相同（去除前後空白、不可空白）；已被其他資產（含 retired，不分大小寫）使用時拒絕；只改大小寫（`nas-01` → `NAS-01`）允許
  - 單純更名不改變比對結果，不觸發重新比對
  - 不保留舊編號紀錄：更名後舊編號不再對應任何資產，可再被新增或匯入使用
- 新增 CLI `cvewatch asset rename <舊編號> <新編號>`；完成後提示舊清冊與已匯出的 VEX 不會對應到新編號，建議重新匯出清冊
- 視窗版「編輯」對話框的「資產編號」欄位改為可編輯；與原編號不同時，欄位下方提示更名的影響。更名與其他欄位、元件的修改在同一個交易儲存，儲存後資產清單選取新編號
- 更名之後，外部仍寫舊編號的資料**不會**自動對應（使用者確認的取捨）：
  - 匯入舊清冊時，舊編號視為新資產；搭配 `--prune` 時，更名後的資產因不在清冊中而被標為退役
  - VEX 匯入時，bom-ref 為 `cvewatch:<舊編號>:…` 的判定列為未對應
  - 已送出或已排入送出佇列的通知、已建立的 Redmine 議題內容保留舊編號；之後產生的內容使用新編號
- **BREAKING**（規格層面）：`asset-inventory` 原本規定 `asset_tag` MUST NOT 可修改，改為可依「更名」規則修改

## Capabilities

### New Capabilities

（無）

### Modified Capabilities

- `asset-inventory`：「Editing asset fields」不再禁止修改 `asset_tag`；新增「Renaming an asset」規則（驗證、保留紀錄、不留舊編號、對清冊與 VEX 的影響）
- `gui-console`：新增「GUI asset renaming」：編輯對話框可修改資產編號，提示更名的影響，衝突標在欄位旁（封存時由修改「GUI asset editing」改為獨立需求，因為原需求加入後超過 500 字）

## Out of scope

- 保留舊編號別名，或在匯入清冊、VEX 時把舊編號對應到更名後的資產
- 更名 retired 資產（請先恢復；要釋出編號可永久刪除）
- TUI 的更名操作
- 批次更名（例如依對照表一次更名多個資產）
- 修改已送出或佇列中的通知、已建立的 Redmine 議題內容
- 更名時通知其他已開啟的 TUI／GUI 視窗重新載入

## Impact

- `crates/core/src/assets/edit.rs`：新增 `rename_asset` 與 `EditOp::Rename`；`repo.rs` 增加更新 `asset_tag` 的方法
- `crates/cvewatch`：`AssetAction::Rename` 子命令與輸出
- `crates/frontend`：`FormMode::Edit` 需記住原編號；`Workspace::save_asset_form` 依原編號找資產，編號不同時加入更名步驟
- `crates/gui`：`dialogs/asset_edit.rs` 編輯時開放編號欄位並顯示更名提示，對話框標題維持原編號
- 不需要資料庫 migration
- README：資產編修說明移除「資產編號建立後不可修改」，加入更名說明與 CLI 用法
