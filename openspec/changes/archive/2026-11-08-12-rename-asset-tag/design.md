# Design

## Context

- 資料庫中只有 `asset.asset_tag` 存放資產編號（`TEXT NOT NULL UNIQUE`，區分大小寫）；元件、版本歷史、未辨識產品、命中、處理事件、通知去重都以整數 id 關聯，`v_component` 等查詢在讀取時才 join 出編號。因此更名只需更新一欄，不需要 migration
- 通知（email 內容、Redmine 議題與註記）在產生時才查詢編號；`outbox.payload_json` 是已產生的內容快照
- 外部檔案以編號字串對應資產：清冊匯入（`asset_tag` 決定新增或更新）、VEX bom-ref（`cvewatch:<asset_tag>:…`）、CLI 參數（`asset show`、`import-packages`、`pending resolve --apply`）。依 proposal 的取捨，這些都不處理舊編號
- 10 建立的編修操作在 `crates/core/src/assets/edit.rs`：每個公開操作一個交易，`apply_edits` 把表單的多個步驟（`EditOp`）放在同一個交易，並在交易內記錄「比對待補」（`mark_match_stale`）
- 編輯表單（`crates/frontend/src/asset_form.rs`）目前以 `form.asset_tag` 當作要編輯的資產；GUI 對話框在 `FormMode::Edit` 時把編號欄位設為唯讀

## Goals / Non-Goals

**Goals:**

- 更名的檢查（狀態、空白、重複）全部在 core 的交易內完成，GUI 與 CLI 共用
- 只有更名時不觸發比對；和其他修改一起儲存時，維持「一次儲存、同一交易」

**Non-Goals:**

- 不新增舊編號別名資料表或更名歷史
- 不改變匯入、VEX、等待區既有的編號對應規則

## Decisions

### D1. core：`rename_asset` 與 `EditOp::Rename`

- 新增 `rename_asset(conn, tag, new_tag) -> Result<String, CoreError>`，回傳實際寫入的新編號（不寫任何時間欄位，因此不帶 `now`），供 CLI 與「只更名」的表單儲存使用
- 新增 `EditOp::Rename(&str)`，讓 `apply_edits` 能在同一交易內先更名、再套用其他步驟。後續步驟以 `Asset.id` 操作，不受編號改變影響；`EditOp::Fields` 原本就不使用 `input.asset_tag`
- 共用的 `rename_in(tx, asset, new_tag)`：
  1. `new_tag.trim()` 為空 → `CoreError::Invalid("資產編號不可空白")`
  2. 與原編號完全相同 → `Invalid("新編號與原編號相同")`（表單只在不同時才產生此步驟，此檢查主要給 CLI）
  3. 以既有的 `tag_in_use(conn, tag, exclude)` 查 `asset_tag = ?1 COLLATE NOCASE`，排除自身 id；有結果時依狀態回報「資產編號 X 已被使用」或「資產編號 X 屬於已退役的資產」。排除自身 id，因此只改大小寫可通過
  4. `AssetRepo::rename(id, new_tag)`：`UPDATE asset SET asset_tag = ?2 WHERE id = ?1`
- 狀態檢查沿用 `active_asset`（retired 回報「已退役，請先恢復再編修」）；舊編號以 `find_by_tag` 完全比對，與 `asset show` 等既有操作相同
- 日誌：交易提交後才寫 `tracing::info!(from = %old, to = %new, "資產更名")`，避免交易回復後留下沒有發生的更名紀錄。`app-config` 允許記錄 asset_tag

替代方案：把編號放進 `EditOp::Fields` 一起更新。否決，因為 `Fields` 和清冊匯入共用「以編號識別、只改其他欄位」的語意。混在一起後，匯入路徑（`update_record`）容易誤用。

### D2. 只更名時不記錄「比對待補」

比對結果與編號無關。`rename_asset` 用自己的交易，**不**呼叫 `mark_match_stale`，因此 CLI 更名後下次 sync 不會多跑一次比對，GUI 也不必在背景比對。

「是否影響比對」的規則放在 core：`EditOp::affects_matching()`（`Rename` 為 false）。`apply_edits` 只有在某個步驟影響比對時，才在同一交易記錄「比對待補」；前端一律呼叫 `apply_edits`（`Rename` 放在第一步），`Refreshed.needs_match` 取 `ops.iter().any(EditOp::affects_matching)`。

替代方案：讓前端在只更名時改呼叫 `rename_asset`。這樣規則會散在 core 與前端兩處（code review 指出），所以不採用。

### D3. 表單記住原編號

- `FormMode::Edit` 改為 `FormMode::Edit { original_tag: String }`，`AssetForm::from_entry` 時填入
- 新增 `AssetForm::renamed_to(&self) -> Option<&str>`：編輯模式下，`asset_tag.trim()` 不為空、且與 `original_tag` 不完全相同時，回傳新編號
- `Workspace::save_asset_form`：以 `original_tag` 找 `data.asset`。`edit_steps` 在 `renamed_to` 有值時，第一步加上 `("更名 OLD → NEW", EditOp::Rename(new))`
- `fields_changed` 目前比較整個 `AssetRecord`，其中含 `asset_tag`，只改編號時也會誤判成「修改資產欄位」。改為比較前先把驗證結果的 `asset_tag` 換成原編號
- 成功訊息使用新編號，例如「已儲存 NAS-11（更名 NAS-01 → NAS-11、修改資產欄位）」。失敗訊息使用原編號，例如「儲存 NAS-01 失敗（未寫入任何變更）」
- 重複編號先在前端檢查（code review 修正）：`asset_form::rename_conflict` 以載入的使用中與已退役資產（`Snapshot` 一律載入兩者）比對，排除原資產、不分大小寫；衝突時錯誤標在「資產編號」欄位旁（「已被使用（FW-CORE）」「已被使用（已退役的 NAS-02）」），和其他本地驗證錯誤一樣不寫狀態列。`Workspace::validate_asset_form` 合併欄位驗證與這項檢查，GUI 與 `save_asset_form` 都用它。core 仍在交易內再檢查一次

### D4. GUI 編輯對話框

- 編號欄位在新增與編輯模式都可編輯，標籤仍為「資產編號 *」
- `renamed_to` 有值時，欄位下方以警示色顯示 `rename_note(OLD)`：「更名後，舊清冊與已匯出的 VEX 中的「OLD」不會對應到此資產；儲存後建議重新匯出清冊」。提示內含原編號，因此以公開函式（而非常數）提供給 kittest 比對；文字以欄位寬度換行
- 對話框標題維持「編輯 OLD」，讓使用者知道正在編輯哪個資產
- `save_asset` 成功後把 `assets_selected` 設為表單的新編號。這是現有行為，原本就用 `form.asset_tag`
- 未儲存修改的判斷（`dirty`）不變；更改編號也算修改，Esc 不會關閉對話框

### D5. CLI `cvewatch asset rename`

- `AssetAction::Rename { asset_tag: String, new_tag: String }`，說明文字：「更改資產編號（元件、命中與處理紀錄保留；舊編號不再對應）」
- 開啟資料庫後呼叫 `rename_asset`，成功時輸出：
  ```
  已將 NAS-01 更名為 NAS-11
  注意：舊清冊與已匯出的 VEX 中的 NAS-01 不會對應到此資產；請以 `cvewatch asset export` 重新匯出清冊。
  ```
- 錯誤沿用 `CoreError::Invalid` → 結束碼 1（與 `asset show` 找不到資產相同）
- 不加確認提示，也不加 `--yes`：更名可以再更名回來，不是破壞性操作

### D6. 測試策略

- core：更名的各種情況（保留命中、處理事件、版本歷史，重複、大小寫、空白、retired、相同編號），以及「只更名不記錄比對待補」。外部對應另外測：舊清冊 `--prune` 重匯、舊 bom-ref 的 VEX、重新使用舊編號，都在記憶體資料庫上執行既有的匯入與 VEX 函式
- frontend：`renamed_to`、`edit_steps` 的步驟（只更名、更名＋欄位、只改欄位不誤判更名）、`save_asset_form` 只更名時 `needs_match = false`、重複編號時資料不變
- GUI kittest：更名提示出現、儲存後選取新編號、重複編號時對話框保留
- CLI：執行檔無法在測試中開啟加密資料庫（不得存取真實 Credential Manager），因此只以 `assert_cmd` 驗證 `--help` 說明與參數錯誤；更名邏輯由 core 測試涵蓋（與 10 的 `asset export` 測試方式相同）

## Risks / Trade-offs

- [更名後重匯舊清冊，產生重複資產或退役] → GUI 提示與 CLI 輸出都建議重新匯出清冊；README 的「Excel 往返」流程加註更名後要先匯出。匯入預覽本來就會列出「新增 NAS-01」「清冊中消失 NAS-11」，使用者在確認前可以看到
- [舊 VEX 判定對應不到] → VEX 匯入報告已列出未對應的 bom-ref，使用者可以手動處理
- [Redmine 議題內文保留舊編號，與之後的註記不一致] → 接受；註記使用新編號，舊編號仍可在 cvewatch 中查到命中（`finding` 的時間軸不變）
- [其他已開啟的 TUI／GUI 視窗仍顯示舊編號，以舊編號操作會找不到資產] → 既有行為（10 已列為不處理），重新載入即可
- [SQLite `UNIQUE` 區分大小寫，但產品規則不分大小寫] → 交易內先以 `COLLATE NOCASE` 檢查；`UNIQUE` 只是最後的保護

## Migration Plan

不需要資料庫 migration，也沒有設定變更。回復時移除新的指令與操作即可，已更名的資產維持新編號。

## Apply 時的決定

依 `/opsx-auto` 流程，以下調整未另行詢問，採建議做法：

1. **`rename_asset` 不帶 `now`**（tasks 1.2）：更名不寫入任何時間欄位，也不記錄「比對待補」。保留 `now` 參數只會換來一個未使用的參數，所以拿掉
2. **重複編號的訊息**：被 active 資產占用時為「資產編號 X 已被使用」，被 retired 資產占用時為「資產編號 X 已被使用（屬於已退役的資產）」。不像新增資產那樣建議「改為恢復該資產」，因為更名的情境不是要恢復那個資產
3. **`apply_edits` 攔截 `EditOp::Rename`**：更名後以更新過的 `Asset` 執行後續步驟，錯誤訊息也使用新編號。`apply_op` 中的 `Rename` 分支只是為了讓 match 完整，行為與攔截時相同
4. **更名提示用函式 `rename_note(原編號)`**（tasks 4.1）：提示內含原編號，無法寫成常數
5. **測試放在新檔**：core、frontend、GUI 各新增 `tests/asset_rename.rs`，不擴充既有的 `asset_edit.rs`，以免檔案過大（core 的 `asset_edit.rs` 已有約 900 行）。core 測試沿用 VEX 測試的 `FW-01` 資料，因為它已經有 4 筆命中與處理事件；GUI 測試使用種子資料 `PA-460`
6. **CLI 測試多驗證一項**：尚未 `init` 時，`asset rename` 以結束碼 1 提示 `cvewatch init`，與其他需要資料庫的指令一致
7. **`Workspace::save_asset_form` 拆成 `save_new_asset`、`save_asset_edits`、`after_save`、`save_failed`**：加入更名分支後，原函式會超過 50 行

## Code review 修正

`/code-review` 一輪共 8 項發現，全部修正：

1. **只改大小寫的更名與清冊匯入**：資料庫的編號區分大小寫，匯入規劃也以完全相同的字串找既有資產。`FW-01` 更名為 `fw-01` 後，匯入舊清冊會新增一個只差大小寫的 `FW-01`，破壞「不分大小寫不重複」的規則；畫面以 `eq_ignore_ascii_case` 查詢時也可能找到錯的資產。修正方式是讓 `plan_import` 拒絕與既有資產（含 retired）只差 ASCII 大小寫的列（與 SQLite `COLLATE NOCASE` 一致），訊息建議改用相同寫法或 `cvewatch asset rename`；spec 新增「Import rejects tags differing only in letter case」。這也補上了既有的漏洞：先在 GUI 新增 `fw-01`、再匯入 `FW-01`，原本同樣會產生重複
2. **更名日誌在交易提交前寫入**：改為提交後才寫（`log_rename`），`apply_edits` 與 `rename_asset` 共用
3. **「只更名不需比對」寫在前端**：改為 core 的 `EditOp::affects_matching`，見 D2
4. **`apply_op` 中 `Rename` 分支是死碼**：`apply_op` 改為接受 `&mut Asset`，`Rename` 只在這裡處理，`apply_edits` 不再另外攔截
5. **重複編號查詢重複實作**：`tag_in_use` 加上 `exclude: Option<i64>`，`create_asset` 與更名共用
6. **CLI 自行去除空白後才輸出**：`rename_asset` 回傳實際寫入的編號，CLI 直接輸出它
7. **`text_field` 的 `enabled` 參數已無用途**：移除，改用 `ui.add`
8. **更名衝突沒有標在欄位上**：改為在前端先檢查，見 D3

## 封存時的調整

- 同步主 spec 後，`openspec validate --specs --strict` 指出 `gui-console` 的「GUI asset editing」需求描述超過 500 字。原因是更名規則併入了該需求。處理方式：把更名規則與兩個更名 scenario 拆成新需求「GUI asset renaming」，原需求維持原文；delta spec 也同步改為只 ADDED。行為沒有改變

