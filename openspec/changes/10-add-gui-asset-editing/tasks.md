# Tasks

前提：09-add-gui-console 已 archive（本 change 的 `gui-console` delta 以 09 建立的主 spec 為基礎）。core 測試使用 `crates/core/tests/common` 的記憶體資料庫與 `tests/fixtures`；GUI 測試以 `egui_kittest::Harness`、`cvewatch_frontend::test_support` 替身、fake companion 與 `FileDialogs` 替身執行，不呼叫真實外部 API、不啟動真實子程序、不開啟原生對話框。

## 1. core：欄位驗證與版本解析共用

- [x] 1.1 `validate.rs` 抽出 `validate_record(RecordInput) -> Result<AssetRecord, Vec<FieldError>>`（design D2），`parse_asset_row` 改為呼叫它並把 `FieldError` 轉回「第 N 列：…」訊息。完成條件：既有匯入測試（含 spec 的 Missing required field、Invalid exposure value、Invalid category value）全部不改即通過
- [x] 1.2 `plan.rs` 抽出 `resolve_version(catalog_product, version_raw) -> (Option<String>, Option<VersionWarning>)`（design D3），`resolve_asset` 改為呼叫它。完成條件：既有 plan 與版本正規化測試不改即通過
- [x] 1.3 單元測試：`validate_record` 對空 `asset_tag`、`public` 曝露度、重要性 `5`、未填曝露度／重要性（預設 internal、2）各回傳對應 `FieldError.field` 與訊息；`resolve_version` 對 ESXi `8.0 latest` 回傳與匯入相同的警告。完成條件：`cargo test -p cvewatch-core assets::validate assets::plan` 通過

## 2. core：資產與元件編修操作

- [x] 2.1 `assets/edit.rs`：`create_asset`、`update_asset`（design D1），每個操作單一交易並在同一交易 `mark_match_stale`；`asset_tag` 已存在（含 retired）時拒絕並區分兩種訊息。完成條件：單元測試重現 spec 的 Adding a new firewall、Tag used by a retired asset、No component given、Raising exposure、Invalid criticality
- [x] 2.2 `add_component`、`set_component_version`、`remove_component`：重複產品、移除最後一個清冊元件、對套件元件或 retired 資產操作皆拒絕；版本修改沿用 `upsert_product` 的歷史記錄。完成條件：單元測試重現 spec 的 Upgrading firmware on screen、Adding a duplicate product、Removing the last component、Version text the scheme cannot read，並斷言對 dpkg 元件 `remove_component` 被拒絕
- [x] 2.3 `resolve_unresolved`、`delete_unresolved`：改為目錄產品時沿用原版本字串並移除該筆未辨識產品。完成條件：單元測試重現 spec 的 Mapping to a catalog product；刪除最後一筆未辨識產品且沒有清冊元件時被拒絕
- [x] 2.4 `AssetRepo::restore` 與 `retire_asset`、`restore_asset`（design D8）。完成條件：單元測試重現 spec 的 Deleting an asset on screen（命中與 `finding_event` 筆數不變）與 Restoring a retired asset（之後執行 `run_match` 重新列出命中）
- [x] 2.5 `purge_preview`（元件數、命中數）與 `purge_asset`（design D7）。完成條件：單元測試重現 spec 的 Purging a retired asset（`component`、`component_history`、`finding`、`finding_event`、`asset_unresolved` 中該資產的列皆為 0，`cve`、`pending`、`redmine_issue` 列數不變）、Active asset cannot be purged、Reusing a purged tag（再匯入時計為新增）
- [x] 2.6 確認 `outbox` 中已排入、指向被永久刪除資產命中的 Email / Redmine 訊息在處理時不會失敗（design D7）；若會回查命中，改為在 `purge_asset` 同一交易刪除這些尚未送出的 outbox 列。完成條件：測試排入一筆該資產命中的急件通知後永久刪除資產，再執行一次 outbox 處理（以 wiremock 替身 SMTP / Redmine），處理不報錯；結論記錄於 design.md Implementation Notes
- [x] 2.7 編修觸發比對的整合測試：每個編修操作後 `rematch_state(...).needed()` 為 true；重現 spec 的 Version fixed and finding resolved（以 fixture CVE 只影響 `11.1.4-h7`，改版後 `run_match` 使命中轉為已處理）。完成條件：`cargo test -p cvewatch-core --test asset_edit` 通過

## 3. core 與 CLI：清冊匯出

- [x] 3.1 `template.rs` 的 `write_sheets` 改為 `save_to_buffer` 後以 `create_new(true)` 寫檔（design D6），移除 `path.exists()` 預先檢查。完成條件：既有 `asset template` 測試通過；新測試斷言目的檔已存在時錯誤訊息相同且原檔位元組不變
- [x] 3.2 `assets/export.rs`：依 design D5 讀取 active 資產與清冊元件、未辨識產品，產出 `SheetData` 並寫出；回傳匯出的資產數與警告。完成條件：單元測試重現 spec 的 Unresolved product is kept、Retired assets and packages are left out，並斷言沒有任何元件的舊資產產生「重匯前需補上產品與版本」警告
- [x] 3.3 CLI `cvewatch asset export <path.xlsx>`（`AssetAction::Export`），輸出路徑、資產數、警告與「此檔未加密」提示；目的檔已存在時以錯誤結束。完成條件：`assert_cmd` 測試重現 spec 的 Export target exists；`cvewatch asset export --help` 顯示中文說明
- [x] 3.4 往返整合測試：以 `tests/fixtures` 的清冊匯入 → `asset export` → `asset import --dry-run` 同一檔案，斷言「新增 0、更新 0」、無版本變更、無清冊中消失（spec 的 Round trip without changes）；並在 README「資產清冊」章節加入 `asset export` 與「先匯出、再編輯、再匯入」流程說明。完成條件：`cargo test -p cvewatch --test asset_export` 通過，README 中的指令可照抄執行

## 4. frontend：表單模型與 Workspace 方法

- [x] 4.1 `frontend::asset_form`：`AssetForm`（資產欄位字串、元件草稿、未辨識產品處置）、`AssetForm::new()` / `from_entry(&AssetEntry)`、本地驗證（`validate_record` + 重複產品 + 至少一個元件），錯誤以 `StandardField` 或元件列索引定位。完成條件：單元測試斷言空 `asset_tag` 與重複產品各回傳對應位置的錯誤，且不需資料庫
- [x] 4.2 目錄產品選單資料：由 `Settings.catalog` 產生 `(key, display_name, vendor_display)` 清單與以 display_name／alias 的不分大小寫搜尋。完成條件：單元測試斷言搜尋 `pan` 包含 PAN-OS、搜尋 `ESX` 包含 VMware ESXi
- [x] 4.3 `workspace/assets.rs`：`save_asset_form`（新增，或把差異轉成 `EditOp` 以 `apply_edits` 在同一交易執行，design D9；code review 後由逐步交易改為單一交易）、`retire_asset`、`restore_asset`、`purge_preview`、`purge_asset`、`export_inventory(path)`、`write_template(path)`；寫入成功回傳 `Refreshed` 並標示 `needs_match`；`AssetFilter` 增加「含已退役」。完成條件：以記憶體資料庫的單元測試斷言編輯 `FW-01` 版本後 `needs_match` 為 true 且重新載入的資料顯示新版本；任何一步失敗時 `Report` 為錯誤且資料庫不變（含第一步）
- [x] 4.4 frontend 測試：`cargo test -p cvewatch-frontend asset` 涵蓋 4.1～4.3，含「顯示已退役」時清單包含 retired 資產、預設不包含。完成條件：測試通過

## 5. GUI：下載範本與匯出清冊

- [x] 5.1 `FileDialogs` 新增 `save_xlsx(title, dir, file_name)`，`NativeDialogs` 以 `rfd` 實作（篩選 .xlsx）；測試替身可預設回傳路徑或取消。完成條件：既有 GUI 測試以更新後的替身通過
- [x] 5.2 資產畫面工具列「下載範本」「匯出清冊」（design D11 的預設檔名與文件資料夾），呼叫 `Workspace::write_template` / `export_inventory`，結果顯示在狀態列（匯出含資產數與「此檔未加密」）。完成條件：手動在視窗中按兩個按鈕各一次，產生的檔案可用 Excel 開啟
- [x] 5.3 kittest：重現 spec 的 Saving the template、Template target already exists、Exporting the inventory；取消對話框時狀態列顯示「已取消」且不產生檔案。完成條件：`cargo test -p cvewatch-gui assets_export` 通過

## 6. GUI：資產編輯對話框

- [x] 6.1 `dialogs/asset_edit.rs`：資產欄位區（新增時才可編輯 `asset_tag`；category、曝露度、重要性下拉選單）、欄位錯誤紅字（design D11）。完成條件：手動開啟「新增資產」對話框，留空儲存時欄位下方出現錯誤
- [x] 6.2 元件表格：版本可編輯、移除按鈕、可搜尋的「加入元件」`ComboBox`；套件元件唯讀並標示「由套件清單管理」；未辨識產品列提供「改為目錄產品」與「刪除」。完成條件：手動對含套件元件的資產開啟編輯，套件列沒有可編輯欄位
- [x] 6.3 資產畫面「新增資產」「編輯」按鈕與儲存流程：儲存中停用按鈕、成功後關閉對話框並選取該資產、失敗時保留對話框與輸入內容。完成條件：手動新增一個資產後清單出現並被選取
- [x] 6.4 kittest：重現 spec 的 Adding an asset from the window、Invalid field keeps the dialog open、Editing a component version，以及按「取消」後資料庫不變、未辨識產品改為目錄產品後詳情不再列出該筆。完成條件：`cargo test -p cvewatch-gui asset_edit` 通過

## 7. GUI：退役、恢復與永久刪除

- [x] 7.1 「刪除」確認對話框（說明退役語意）與「顯示已退役」核取方塊；retired 資產以灰字與「（已退役）」標示，詳情只提供「恢復」「永久刪除」而不提供「編輯」。完成條件：手動刪除一個資產後預設清單不再列出，勾選後以退役樣式列出
- [x] 7.2 `dialogs/purge.rs`：顯示 `purge_preview` 的元件數與命中數、建議先執行 `cvewatch db backup`，輸入完全相同的 `asset_tag` 才啟用「永久刪除」。完成條件：手動輸入不完整的資產編號時按鈕停用
- [x] 7.3 kittest：重現 spec 的 Deleting from the asset list、Purge confirmation requires the tag，以及恢復後資產回到預設清單、永久刪除後勾選「顯示已退役」也不再列出。完成條件：`cargo test -p cvewatch-gui asset_retire` 通過

## 8. GUI：編修後背景比對與匯入提醒

- [x] 8.1 背景比對（design D10）：`needs_match` 時以 fake／真實 companion 在背景執行 `match`，進行中顯示「比對中」，完成後 `reload` 並顯示摘要；進行中再次編修時設 `rematch_again`；companion 為 None 或結束碼非 0 時顯示警告與 `cvewatch match` 提示。完成條件：手動編修一個版本後狀態列先顯示「比對中」再顯示比對結果
- [x] 8.2 匯入預覽對話框加入覆寫提醒文字（gui-console 的 GUI import overwrite reminder）。完成條件：手動拖入清冊時預覽頂端顯示提醒
- [x] 8.3 kittest：重現 spec 的 Matching runs in the background（fake companion 延遲回應期間視窗仍可切換畫面）、Companion missing、Previewing after on-screen edits；並斷言比對進行中再次編修只會在結束後再跑一次 `match`。完成條件：`cargo test -p cvewatch-gui asset_match` 通過；README「視窗版」章節加入資產編修、範本與匯出說明及永久刪除前先備份的提醒

## 9. 整合驗證

- [x] 9.1 全專案檢查：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 全部通過；`openspec validate 10-add-gui-asset-editing --strict` 通過。完成條件：四個指令皆成功
- [ ] 9.2 手動整合測試：以 `cargo build --release` 的實際執行檔，在視窗中下載範本 → 填好匯入 → 新增一個資產、修改一個版本、刪除並恢復一個資產 → 匯出清冊 → 將匯出檔拖回視窗，確認預覽為「新增 0、更新 0」；永久刪除一個已退役資產後命中畫面不再列出其命中。完成條件：結果記錄於 design.md 的「手動驗證」章節

## Workflow follow-up

- archive 時將主 spec `openspec/specs/asset-inventory/spec.md` 的 Purpose 改為「以清冊匯入與畫面編修維護資產，資料庫為目前狀態、清冊為交換格式」（design Migration Plan）
- archive 後執行 `openspec validate --all --strict` 確認通過
