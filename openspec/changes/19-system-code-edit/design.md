# Design

## Context

- 資產說明存在 `asset_system (code TEXT PRIMARY KEY, name TEXT NOT NULL)`，代碼與名稱各有 `COLLATE NOCASE` 的唯一索引；`asset.system_code REFERENCES asset_system (code)`，沒有 `ON UPDATE CASCADE`（0008 當時假設代碼不可改）。工作資料庫開啟時 `foreign_keys = ON`
- `asset.isms_id` 保留使用者寫法（已去除前後空白），有 `COLLATE NOCASE` 的部分唯一索引；格式為「三碼系統代碼-大類-三碼流水號」，系統代碼必須等於 `system_code`（16 Asset system membership）
- 從 ISMS 清冊匯入的資產（18），`asset_tag` 預設等於 ISMS 編號；ISMS 匯入以 ISMS 編號比對既有資產
- 資產更名（12）：`edit.rs` 的 `rename_in` 檢查空白、相同、`tag_in_use`（不分大小寫、含 retired、排除自身）後呼叫 `AssetRepo::rename`；元件、命中、處理紀錄以資產 id 關聯而保留；只更名不記錄「比對待補」（12 design D2）。公開的 `rename_asset` 只接受 active 資產
- 資產說明管理：core `systems.rs`（469 行）提供 `validate_new`／`validate_rename`／`create_system`／`rename_system`／`delete_system`；frontend `workspace/systems.rs` 包成 `Workspace::create_system` 等，回傳 `Refreshed`；GUI `dialogs/asset_systems.rs` 的列有「改名 X」與「刪除 X」。目前沒有 CLI 子命令管理資產說明
- `SystemDirectory::conflict(code, name)` 以 `code` 排除自身；代碼改變時若用新代碼呼叫，會把自己原本的名稱算成重複

## Goals / Non-Goals

**Goals:**

- 修改代碼的驗證、預覽與寫入都在 core，GUI 與 CLI 共用；寫入時在交易內重新計算計畫，不信任畫面快照
- 只改名稱時行為與現在相同（不顯示確認、不連動資產）
- 不需要 migration

**Non-Goals:**

- 不重建 `asset` 表加上 `ON UPDATE CASCADE`
- 不保留代碼或編號的變更歷史（只寫 tracing 日誌，比照資產更名）

## Decisions

### D1. core 模組：`assets/systems_edit.rs`

`systems.rs` 已近 470 行，修改代碼的邏輯另放 `systems_edit.rs`（由 `systems` 重新匯出），`rename_system` 與 `validate_rename` 改為呼叫新的函式後移除：

```rust
pub struct SystemEdit { pub code: SystemCode /* 原代碼 */, pub new_code: String, pub name: String }

pub struct RecodedAsset {
    pub id: i64,
    pub asset_tag: String,
    pub status: AssetStatus,
    pub isms_id: Option<(String, String)>,   // (舊, 新)
    pub new_tag: Option<String>,             // 資產編號等於舊 ISMS 編號時
}

pub struct SystemEditPlan {
    pub before: AssetSystem,
    pub after: AssetSystem,
    pub assets: Vec<RecodedAsset>,           // 代碼未變時為空
}

pub fn validate_edit(edit: &SystemEdit, dir: &SystemDirectory) -> Result<AssetSystem, Vec<SystemError>>;
pub fn plan_system_edit(conn: &Connection, edit: &SystemEdit) -> Result<SystemEditPlan, SystemEditError>;
pub fn edit_system(conn: &mut Connection, edit: &SystemEdit) -> Result<SystemEditPlan, SystemEditError>;
```

- `SystemEditError`：`Fields(Vec<SystemError>)`（欄位錯誤，畫面標在欄位旁；資產編號衝突也歸在 `SystemField::Code`）或 `Failed(CoreError)`；`From` 轉成 `CoreError::Invalid` 供 CLI 使用
- `plan_system_edit` 只讀；`edit_system` 開交易、在交易內呼叫 `plan_system_edit` 後套用並 commit，回傳實際套用的計畫
- 原代碼不存在時回報「資產說明 X 不存在」（`SystemField::Code`）

### D2. 驗證：排除自身

`validate_edit` 依序檢查：

1. 原代碼存在
2. 新代碼格式（與 `validate_new` 相同訊息）；`SystemCode::parse` 後等於原代碼時視為只改名稱（只改大小寫因為以大寫儲存，自然等於原代碼）
3. 新代碼不等於其他資產說明的代碼（「資產說明代碼 SYS 已被使用」）
4. 名稱：沿用 `check_name`，但衝突檢查改為以**原代碼**排除自身：`SystemDirectory::conflict` 新增 `exclude: &SystemCode` 參數（或新增 `conflict_excluding`），名稱與其他資產說明的名稱、代碼比對，以及新代碼與其他資產說明的名稱比對都排除原資產說明

`validate_new` 改呼叫同一個內部函式（`exclude = None`），兩者訊息一致。

### D3. 連動計畫

代碼改變時，`plan_system_edit` 讀出 `system_code = 原代碼` 的所有資產（含 retired，依 `asset_tag` 排序）：

- ISMS 編號：有值時新值為「新代碼 + 舊值第 4 個字元之後」（保留使用者其餘寫法，例如 `NET-hw-001` → `NWS-hw-001`）
- 資產編號：`asset_tag` 與舊 ISMS 編號不分大小寫相同時，新編號為「新代碼 + 舊編號第 4 個字元之後」
- 衝突檢查：每個新編號以 `tag_in_use(conn, new_tag, Some(id))` 檢查（改為 `pub(crate)`）。被改名的資產舊編號都以舊代碼開頭、新編號都以新代碼開頭，因此衝突對象不可能是同一批中也要改名的資產，不需要處理交換或暫時編號。有衝突時收集全部衝突（「資產編號 NWS-HW-001 已被使用（屬於已退役的資產）」）後一次回報
- ISMS 編號的唯一索引：新 ISMS 編號的系統代碼是新代碼，而新代碼目前不存在，依 membership 規則不會有資產使用以新代碼開頭的 ISMS 編號；交易內仍由唯一索引把關，萬一衝突就整筆失敗

### D4. 套用：延後外鍵檢查

在 `edit_system` 的交易中：

1. `PRAGMA defer_foreign_keys = ON`（只在此交易有效，commit 或 rollback 後自動恢復）
2. `UPDATE asset_system SET code = ?新, name = ?名稱 WHERE code = ?原`（名稱索引不衝突，因為是同一列）
3. `UPDATE asset SET system_code = ?新 WHERE system_code = ?原`
4. 逐筆更新 ISMS 編號（`AssetRepo` 新增 `set_isms_id`）與資產編號（`AssetRepo::rename`）
5. commit 時才檢查外鍵

替代方案：新增一列新代碼 → 改資產 → 刪舊列。名稱的唯一索引會讓新列與舊列衝突，需要暫時名稱，較繁瑣；重建 `asset` 表加 `ON UPDATE CASCADE` 需要 migration 且影響元件等外鍵，不採用。

- 不記錄「比對待補」：資產說明與資產編號都不參與比對（12 design D2 同理）
- 寫入後以 `tracing::info!` 記錄舊、新代碼與受影響資產數；每個更名的資產另記一筆，格式比照 `log_rename`

### D5. frontend：`Workspace::edit_system` 與預覽

- `rename_system` 改為 `edit_system(&mut self, edit: &SystemEdit, open) -> Result<SystemEdited, SystemSaveError>`；`SystemSaveError::Fields` 沿用
- 新增 `preview_system_edit(&self, edit) -> Result<SystemEditPlan, SystemSaveError>`：先以快照執行 `validate_edit`（欄位錯誤不必查 DB），再呼叫 core `plan_system_edit`（衝突需查 DB）
- 狀態列訊息：只改名「已將資產說明改為 NET 校園網路服務」（現有）；改代碼「已將資產說明代碼 NET 改為 NWS（3 個資產；ISMS 編號 2 筆、資產編號 1 筆變更）」
- 回傳 `SystemEdited { refreshed, renamed: Vec<(String, String)> }`，讓 GUI 把 `assets_selected` 從舊編號換成新編號
- 提醒文字放 frontend 常數 `RECODE_NOTE`：「舊清冊、已匯出的 VEX 與尚未更新的 ISMS 清冊中的舊代碼與舊編號不會對應；儲存後建議重新匯出清冊，並更新 ISMS 清冊」，GUI 與 CLI 共用

### D6. GUI：「編輯」列與確認畫面

- 列上的「改名 X」改為「編輯 X」。編輯中的列：代碼欄（標籤「X 的代碼」，寬 80）與名稱欄（「X 的名稱」，沿用 `rename_label`），下方各自顯示欄位錯誤；按鈕「儲存」與「取消編輯」
- `SystemsDialog.renaming` 改為 `editing: Option<EditRow { code, new_code, name }>`；新增 `confirm: Option<SystemEditPlan>`
- 按「儲存」：呼叫 `preview_system_edit`
  - 欄位錯誤 → 標在欄位旁
  - 計畫的 `assets` 為空（只改名或沒有資產歸屬）→ 直接 `edit_system`
  - 否則設 `confirm`，對話框內容換成確認畫面（不另開 Modal，避免巢狀 Modal 的焦點問題）：標題「將資產說明代碼 NET 改為 NWS？」、「受影響的資產：3 個（含已退役 1 個）」、ISMS 編號與資產編號的變更（`ScrollArea`，最多高 240）、`RECODE_NOTE`（警示色）、「確定修改」「返回」
- 「確定修改」→ `edit_system`（core 在交易內重算；若此時出現衝突，回到編輯列並標示錯誤）；成功後清除 `editing` 與 `confirm`，若 `assets_selected` 在 `renamed` 中則換成新編號
- 「返回」或 Esc → 清除 `confirm`，保留 `editing` 的輸入。編輯列存在時 Esc 不關閉對話框（現有行為）

### D7. CLI：`cvewatch asset system recode`

```rust
AssetAction::System { #[command(subcommand)] action: SystemAction }
enum SystemAction {
    /// 修改資產說明代碼：歸屬資產（含已退役）的 ISMS 編號與等於 ISMS 編號的資產編號一併更新
    Recode { code: String, new_code: String, #[arg(long)] dry_run: bool },
}
```

- 舊代碼以 `SystemCode::parse` 正規化（不分大小寫）；格式不符或不存在時以錯誤結束（「找不到資產說明 LAB」）
- 新代碼與舊代碼相同時以錯誤結束（「新代碼與原代碼相同：NET」），因為 CLI 不改名稱，沒有其他效果
- 名稱沿用原名稱（`SystemEdit.name = 原名稱`）
- 輸出：
  ```
  資產說明 NET 網路服務 → NWS 網路服務
  受影響的資產：3 個（含已退役 1 個）
  ISMS 編號：
    NET-HW-001 → NWS-HW-001
    NET-SW-002 → NWS-SW-002
  資產編號：
    NET-HW-001 → NWS-HW-001
  已將資產說明代碼 NET 改為 NWS
  <RECODE_NOTE>
  ```
  `--dry-run` 時最後兩行改為「預覽：未寫入資料庫」。沒有資產歸屬時只輸出第一行與結果
- 失敗時錯誤結束碼與 `asset rename` 相同（`CoreError::Invalid` 的既有對應）

## Risks / Trade-offs

- [舊資料不再對應] 舊清冊（含 `systems` 工作表的舊代碼）重新匯入時，會嘗試新增舊代碼的資產說明並與新代碼的名稱衝突而中止；ISMS 清冊尚未更新時，ISMS 匯入會回報系統代碼未定義 → 以 `RECODE_NOTE` 在確認畫面與 CLI 提醒，proposal 已列為取捨
- [資產編號被改] 使用者若刻意讓資產編號等於 ISMS 編號以外的用途，修改代碼會一併改編號 → 確認畫面逐筆列出資產編號變更，`--dry-run` 可先預覽
- [延後外鍵檢查] 若步驟遺漏某些資產，錯誤要到 commit 才出現 → 第 3 步以單一 `UPDATE … WHERE system_code = 原代碼` 涵蓋全部，測試以 `PRAGMA foreign_key_check` 確認沒有孤兒列
- [畫面快照與交易內重算不一致] 另一個視窗在確認期間修改了資產 → core 在交易內重算並回傳實際套用的計畫，狀態列以實際結果為準
