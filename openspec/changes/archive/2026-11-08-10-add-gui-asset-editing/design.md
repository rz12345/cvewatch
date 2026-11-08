# Design

## Context

- 資產寫入目前只有一條路：`assets::import::import_inventory`（單一交易：同步目錄 → 讀快照 → `plan_import` → 寫入 → `mark_match_stale`）。`AssetRepo` / `ComponentRepo` 已有 insert、update_record、retire、upsert_product（含版本歷史與軟刪除還原）、remove 等基本操作，沒有恢復（unretire）與永久刪除
- `plan.rs::resolve_asset` 以產品目錄解析產品名稱、計算 `version_norm` 與版本格式警告；`validate.rs` 的欄位規則（必填、category / exposure / criticality）綁在 `RowReader` 上，只能從工作表列呼叫
- 目錄產品的 alias 包含 `key` 與 `display_name`（`catalog/mod.rs`），所以匯出寫 `display_name` 重匯時一定能解析回同一產品
- 匯入快照只看 `source = 'excel'` 的產品元件；不在檔案中的這類元件會被軟刪除
- 外鍵都有 `ON DELETE CASCADE`：`asset → component → component_history / finding / osv_hit / match_conflict`、`finding → finding_event / finding_evidence / notification_log`、`asset → asset_unresolved`；`pending`、`redmine_issue`（以 CVE 為鍵）、`outbox` 沒有指向資產或命中的外鍵；連線已開 `foreign_keys = ON`
- 自動比對 `rematch_if_needed` 在 CLI crate；09 的 GUI 匯入與背景 sync 都經 `companion.rs` 執行同資料夾的 `cvewatch.exe`（09 design D8、D12）
- 09 的 GUI 匯出（VEX）規則：另存新檔對話框、預設文件資料夾、MUST NOT 覆蓋（`vex/file.rs` 以 `create_new(true)` 寫檔）
- 前端寫入（狀態變更、等待區）都由 `cvewatch-frontend` 的 `Workspace` 在行程內直接寫資料庫，結果以 `Report` / `Refreshed` 回傳

## Goals / Non-Goals

**Goals:**

- 編修規則全部在 core，GUI 只負責表單與呼叫；日後 TUI / CLI 可直接沿用
- 匯出 → 不改內容 → 重匯 必為「新增 0、更新 0」，讓 Excel 與畫面編修可安全往返
- 永久刪除只靠既有 cascade，不寫逐表刪除

**Non-Goals:**

- 不做樂觀鎖或多視窗同時編修的衝突偵測（單人工具；最後寫入者為準）
- 不改變匯入規則；匯入仍以檔案內容覆寫

## Decisions

### D1. 編修操作放在 `core::assets::edit`，每個操作一個交易

新增 `edit.rs`，提供：`create_asset(draft)`、`update_asset(tag, fields)`、`add_component(tag, product_key, version)`、`set_component_version(component_id, version)`、`remove_component(component_id)`、`resolve_unresolved(tag, product_raw, product_key)`、`delete_unresolved(tag, product_raw)`、`retire_asset(tag)`、`restore_asset(tag)`、`purge_asset(tag)`。每個函式自己開交易，最後 `mark_match_stale(&tx, now)` 與資料同一交易提交，回傳 `EditOutcome { warnings: Vec<VersionWarning> }`。

- 刪除前檢查「至少一個清冊元件或未辨識產品」與「同產品不重複」在同一交易內查詢，不信任呼叫端的畫面狀態
- 未辨識產品以 `asset_unresolved.id` 指定（清冊可在同一資產列出多筆相同寫法）
- 新增時 `asset_tag` 以不分大小寫比對既有資產（畫面、等待區都以不分大小寫查詢資產編號）
- 操作以 `asset_tag` / `component_id` 定位；找不到或狀態不符（例如對 retired 資產改欄位）回傳 `CoreError::Invalid` 並附可顯示的中文訊息

替代方案：把畫面編修轉成一份單資產的 `Inventory` 交給 `import_inventory` —— 可沿用全部匯入程式，但匯入以「整份清冊」為語意（未列出的元件會被移除、無法表達刪除單一元件或永久刪除），且錯誤訊息是「第 N 列」格式，否決。

### D2. 欄位驗證抽成 `validate::validate_record`

從 `parse_asset_row` 抽出 `validate_record(RecordInput) -> Result<AssetRecord, Vec<FieldError>>`，`FieldError { field: StandardField, message }`。匯入把 `FieldError` 轉為「第 N 列：…」維持原訊息；GUI 依 `field` 把訊息放到對應欄位旁。trim、預設值（internal、2）、允許值清單訊息都只寫一次。

### D3. 版本解析共用 `plan::resolve_version`

從 `resolve_asset` 抽出「以目錄產品與原始版本字串算出 `version_norm` 與 `VersionWarning`」的函式，匯入與 `add_component` / `set_component_version` / `resolve_unresolved` 共用，確保畫面上輸入的版本與 Excel 匯入的正規化結果完全一致。

### D4. 畫面新增的元件沿用 `source = 'excel'`

`ComponentSource::Excel` 的實際語意是「清冊管理的產品元件」。新增 `manual` 來源需要 migration，而且匯入快照會忽略它 → 重匯時畫面加的元件既不會出現在差異中、也不會被移除，造成兩種來源並存且無法以 Excel 管理，否決。程式註解與 README 改稱「清冊元件」。

### D5. 匯出：`core::assets::export`，`display_name` + 原始版本

- 讀取 active 資產（依 `asset_tag` 排序）；每個資產的清冊元件依元件 id 排序（即匯入時的建立順序），未辨識產品接在其後
- 第一筆寫在 `assets` 列的 product / version，其餘寫在 `components`；產品欄寫目錄 `display_name`，未辨識產品寫原文 `product_raw`
- 版本寫 `version_raw`（不是 `version_norm`），因此重匯不會產生版本變更
- 資產若沒有任何清冊元件也沒有未辨識產品（只可能來自舊資料），`assets` 列的 product / version 留空並在匯出結果列出警告「重匯前需補上產品與版本」，不中止匯出
- 匯出欄名一律用標準欄名；`[import.columns]` 只影響讀取，匯入時標準欄名永遠可辨識（`ColumnMap::field_for` 的後備）

### D6. 不覆蓋檔案：`write_sheets` 改為先寫入記憶體再 `create_new`

`rust_xlsxwriter::Workbook::save` 會直接覆蓋，目前範本以 `path.exists()` 先檢查，有檢查與寫入之間的競態。改為 `save_to_buffer()` 後以 `OpenOptions::new().write(true).create_new(true)` 寫檔（與 `vex/file.rs` 相同作法），範本與匯出共用；檔案已存在時回傳同一則「目的檔已存在，不覆蓋」錯誤。CLI `asset template` 行為不變。

### D7. 永久刪除：只允許 retired，依 cascade 刪 `asset` 一列

`purge_asset` 在交易中確認 `status = 'retired'`，先統計元件數與命中數（給確認對話框用，另有唯讀的 `purge_preview(tag)`），再 `DELETE FROM asset WHERE id = ?`。`pending`、`redmine_issue`、CVE 資料不受影響。`outbox` 中已排入的 Email / Redmine 訊息以 `payload_json` 自帶內容，不依賴命中存在 —— 實作時以測試確認（tasks 2.x），若有任何 outbox 處理器會回查命中，改為在同一交易刪除該資產命中相關、尚未送出的 outbox 列。

替代方案：retired 一段時間後自動清除 —— 使用者沒有要求，且自動刪除不可復原的資料風險高，否決。

### D8. 恢復：`AssetRepo::restore`

`status = 'active'`、`retired_at = NULL`、更新 `last_seen`，並沿用 `mark_components_updated` 讓下一次比對重新評估其元件（與匯入時清冊重新出現 retired 資產的處理相同）。

### D9. 前端：`Workspace` 的 `workspace/assets.rs` 與表單模型 `frontend::asset_form`

- `AssetForm`（資產欄位字串 + 元件草稿清單）為 UI 無關的表單狀態，`AssetForm::from_entry(&AssetEntry)` 建立編輯用初值；`Workspace::save_asset_form(form)` 依新增／編輯計算出需要的 core 操作
- 編輯既有資產時，欄位變更、元件移除、元件新增、版本變更、未辨識產品處置各是一個 `EditOp`，由 `core::assets::edit::apply_edits` 在**同一個交易**依序執行；任何一步失敗就整批回復（不會留下部分更新的資產）。「至少一個元件」在全部步驟之後檢查一次，因此可以先移除再加回同一產品。表單在送出前先以 `validate_record` 與重複產品檢查做本地驗證，讓常見錯誤在任何寫入前就被擋下
- 寫入成功回傳 `Refreshed`（重新載入資料）並附 `needs_match = true`，由前端決定如何執行比對
- 目錄產品清單（給下拉選單）由 `Settings.catalog` 提供 `display_name`、`vendor_display`、`key`，搜尋比對 display_name 與 alias

（code review 後修訂：原設計為每步一個交易、失敗時回報已完成的步驟。review 指出部分寫入需要前端重建表單基準，且「移除後加回同一產品」的順序無法表達，改為單一交易。D1 的單一操作函式保留，供退役、恢復、永久刪除與日後的 CLI / TUI 使用。）

### D10. GUI 比對以 companion `cvewatch match` 在背景執行

寫入在行程內完成（快、可回傳結構化錯誤），比對沿用 09 D8 的作法：背景執行緒呼叫 `Companion::run(["match"])`，完成後 `Workspace::reload` 並在狀態列顯示摘要；結束碼非 0 時顯示警告「請執行 `cvewatch match`」。比對進行中再次編修時，不排第二個比對，而是在目前比對結束後若「比對待補」仍成立再跑一次（單一旗標 `rematch_again`）。

替代方案：在 UI 執行緒直接呼叫 `core::matching::run::run_match` —— 全量比對在大資料庫上需數秒，會凍結視窗；改開新執行緒則要第二條資料庫連線與重複 `rematch_if_needed` 的邏輯，否決。

### D11. GUI 對話框

- 「新增資產」與「編輯」共用 `dialogs/asset_edit.rs`：上半部資產欄位（`asset_tag` 只在新增時可編輯），下半部元件表格（產品、版本、來源；套件元件唯讀並顯示「由套件清單管理」），「加入元件」列使用可搜尋的 `ComboBox`
- 驗證錯誤以紅字顯示在欄位下方；儲存中按鈕停用
- 「刪除」確認對話框：說明退役語意（保留紀錄、不再比對、可恢復）
- 「永久刪除」對話框 `dialogs/purge.rs`：顯示 `purge_preview` 的元件數、命中數，輸入框內容與 `asset_tag` 完全相同（區分大小寫、不 trim）才啟用確認按鈕
- 「下載範本」「匯出清冊」在 `FileDialogs` trait 新增 `save_xlsx(title, dir, file_name)`，測試以替身取代；預設檔名 `cvewatch-inventory-template.xlsx` 與 `cvewatch-inventory-YYYYMMDD.xlsx`（顯示時區當天）
- 範本與匯出在行程內呼叫 core（寫檔很快，不需子程序），匯出顯示「此檔未加密」提示（與 VEX 匯出相同）
- 資產清單上方加入「顯示已退役」核取方塊；retired 資產以灰字與「（已退役）」標示

## Risks / Trade-offs

- [使用者在畫面編修後又匯入舊清冊，覆寫畫面上的修改] → 匯入預覽加註提醒（gui-console 規格），README 建議「先匯出、再編輯、再匯入」的流程；預覽本來就列出版本變更與移除，按下匯入前可取消
- [編輯對話框多步寫入中途失敗，資產處於部分更新狀態] → 送出前本地驗證擋下大多數錯誤；失敗時回報已完成的步驟並重新載入，使用者看到的永遠是資料庫實際狀態
- [永久刪除不可復原] → 只允許 retired、需輸入資產編號；README 提醒先 `cvewatch db backup`
- [背景 sync 與畫面編修同時寫入] → SQLite `busy_timeout` 等待；編修交易很短，`mark_match_stale` 確保 sync 結尾或下一次比對會納入編修
- [其他已開啟的 TUI / GUI 不會即時看到編修] → 不在本 change 範圍；重新載入後即可看到

## Migration Plan

無 schema 變更。發布新版 `cvewatch.exe` 與 `cvewatch-gui.exe` 即可；兩者需同版本（GUI 會呼叫 `cvewatch match`，舊版 CLI 也有此指令，相容）。回復舊版不影響資料。archive 時需同步修改主 spec `asset-inventory` 的 Purpose：資產來源由「清冊為唯一來源」改為「清冊匯入與畫面編修，清冊為交換格式」。

## Implementation Notes

apply 時依建議做法自行決定的事項（`/opsx-auto`，未向使用者提問）：

1. **`resolve_version` 多兩個參數**（tasks 1.2）：`VersionWarning` 需要資產編號與列位置，簽名改為 `resolve_version(product, version_raw, asset_tag, row: Option<RowRef>)`；`VersionWarning.row` 改為 `Option<RowRef>`（畫面編修沒有列號），並新增 `describe()` 供 CLI 與 GUI 共用同一句警告。CLI 匯入輸出不變
2. **匯入錯誤的順序**（tasks 1.1）：`validate_record` 先回報資產欄位（必填 → category → exposure → criticality），product / version 缺漏接在其後；訊息文字不變，既有匯入測試未修改即通過
3. **核心編修測試放在 `crates/core/tests/asset_edit.rs`**（tasks 2.1～2.7）：需要比對、CVE 與通知替身，整合測試比 `#[cfg(test)]` 單元測試合適；`cargo test -p cvewatch-core --test asset_edit` 涵蓋 2.1～2.7 的全部 scenario
4. **每個編修都記錄「比對待補」**（D1）：不區分欄位種類（改名稱也記錄），寧可多比對一次；被拒絕的編修不寫入任何資料（含比對待補）
5. **目錄產品 id**：編修時以 key 查 `catalog_product`；目錄有而資料庫尚未同步（例如剛新增 catalog.d）時先 `sync_catalog`
6. **永久刪除的元件數含已軟刪除的元件**：預覽顯示的是實際會被刪除的列數（含先前移除、仍保留命中紀錄的元件）
7. **outbox 結論（tasks 2.6）**：Email 與 Redmine 的佇列項目以 `payload_json` 自帶內容，送出時不回查命中；`notification_log` 依 cascade 刪除後，`outbox::succeed` 的 UPDATE 只是影響 0 列。因此 `purge_asset` 不刪除 outbox 列。測試以既有的本機 rustls HTTPS 替身（Redmine 只接受 https，wiremock 只提供明文 HTTP）排入一張急件議題 → 永久刪除資產 → flush 成功送出
8. **檔案寫入共用 `cvewatch_core::files::write_new`**（D6）：VEX 與 xlsx（範本、清冊匯出）共用 `create_new` 寫檔與失敗清理，各自保留原本的「已存在」訊息
9. **CLI `asset export` 先檢查目的檔再開資料庫**：避免要求金鑰後才失敗；實際寫入仍以 `create_new` 保證不覆蓋
10. **CLI 往返測試在記憶體資料庫上執行**（tasks 3.4）：測試不得存取 Credential Manager，執行檔無法開啟加密資料庫；`crates/cvewatch/tests/asset_export.rs` 以 `asset export` / `asset import --dry-run` 呼叫的同一組 core 函式驗證往返（`inventory_basic.xlsx`、`inventory_v2.xlsx`），`assert_cmd` 只驗證說明文字與「目的檔已存在」
11. **退役資產另列 `Snapshot.retired_assets`**（tasks 4.3「AssetFilter 增加含已退役」）：`Snapshot.assets` 維持只有使用中資產（TUI、等待區套用目標、命中篩選不受影響）；資產畫面以 `Snapshot::asset_list(&AssetListFilter { include_retired })` 取得清單，`any_asset` 查兩者
12. **`Refreshed.needs_match`**：所有 `Refreshed` 新增此欄位（一般操作為 false），資產編修寫入成功時為 true
13. **表單儲存為單一交易**（code review 修正，見 D9）：取代原本的逐步交易與失敗後重建表單
14. **資產詳情顯示版本歷史**：spec 的 Editing a component version 要求詳情可見舊版本；`AssetEntry.history` 以一次查詢（`ComponentRepo::history_by_component`）載入
15. **儲存為行程內同步寫入**（tasks 6.3「儲存中停用按鈕」）：寫入在同一幀內完成，按鈕不會在寫入期間被再次按下，因此沒有另設「儲存中」狀態；耗時的比對在背景執行
16. **比對結果接在寫入結果之後**：狀態列先顯示「<寫入結果>；比對中…」，完成後把「比對中…」換成 `cvewatch match` 輸出的前兩行（失敗時為警告），不被「已重新載入」取代
17. **kittest Harness 的 `max_steps` 改為 8**：表格新增列（`egui::Grid`）第一次排版需要多繪製幾幀
18. **匯出的產品名稱取自目前的產品目錄**（code review 修正）：`export_inventory` 接收 `Catalog`，寫該產品的 `display_name`（確認能解析回同一產品，否則寫 key），不讀資料庫中可能尚未同步的 `catalog_product.display_name`
19. **編輯對話框的 Esc**（code review 修正）：Esc 先關閉下拉選單；表單有未儲存的修改時 Esc 不關閉對話框（以「取消」放棄），關閉視窗時也會詢問
20. **比對進行中再次編修的狀態列**（code review 修正）：同樣以「比對中…」結尾，比對結果接在最後一次寫入結果之後
21. **共用判斷**：`Component::is_inventory_product()`（core 編修與表單的唯讀判斷共用）、`cvewatch_core::files::ensure_absent`（VEX、範本、清冊匯出共用）

## 手動驗證

### 以自動測試取代視窗手動檢查

tasks 5.2、6.1、6.2、6.3、7.1、7.2、8.1、8.2 的完成條件是在視窗中手動操作。自動流程無法操作使用者的視窗與資料庫（開啟加密資料庫需要 Credential Manager 中的金鑰），改以 kittest 在相同畫面上重現後勾選：

| 任務 | 對應測試 |
|---|---|
| 5.2 | `assets_export_saving_the_template`、`assets_export_exporting_the_inventory`（產生的檔案以清冊讀取器讀回） |
| 6.1 | `asset_edit_invalid_field_keeps_the_dialog_open`（留空儲存，欄位下方顯示「缺少 asset_tag」） |
| 6.2 | `asset_edit_package_rows_are_read_only`（套件列顯示「由套件清單管理」，沒有版本欄位與移除按鈕） |
| 6.3 | `asset_edit_adding_an_asset_from_the_window`（新增後清單出現並被選取） |
| 7.1 | `asset_retire_deleting_from_the_asset_list` |
| 7.2 | `asset_retire_purge_confirmation_requires_the_tag`（`NAS-0`、`nas-02` 時按鈕停用） |
| 8.1 | `asset_match_runs_in_the_background`（延後執行的 spawner：先「比對中…」再顯示結果） |
| 8.2 | `asset_match_previewing_after_on_screen_edits` |

### 封存後待補測（需使用者以實際資料庫操作）

| 項目 | 方式 |
|---|---|
| 9.2 整合流程 | 以 `cargo build --release` 的執行檔：下載範本 → 填好匯入 → 新增一個資產、修改一個版本、刪除並恢復一個資產 → 匯出清冊 → 將匯出檔拖回視窗，確認預覽為「新增 0、更新 0」；永久刪除一個已退役資產後命中畫面不再列出其命中 |
| 原生另存新檔對話框 | 「下載範本」「匯出清冊」的 Windows 對話框預設「文件」資料夾、篩選 .xlsx；選既有檔案確認取代時顯示「目的檔已存在」且原檔不變 |
| 注音輸入 | 編輯對話框的名稱、位置、備註欄以注音輸入中文 |

## Open Questions

（無）
