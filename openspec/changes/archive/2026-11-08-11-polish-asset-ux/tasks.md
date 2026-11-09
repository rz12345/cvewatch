# Tasks

前提：10-add-gui-asset-editing 已 archive。GUI 測試以 `egui_kittest::Harness`、`cvewatch_frontend::test_support` 替身與 fake companion 執行；不呼叫真實外部 API、不開啟原生對話框。

## 1. frontend：使用者用語

- [x] 1.1 `display`：`product_name(catalog, key)`、`component_name(catalog, component)`（套件為不含版本的 purl）、`category_label`、`exposure_label`（design D1）。完成條件：單元測試斷言 `proxmox-ve` → 「Proxmox VE」、目錄沒有的 key 原樣回傳、`server` → 「server（伺服器）」、套件元件名稱不含 `@版本`
- [x] 1.2 `AssetForm::validate` 以介面文字回報錯誤（design D2）：必填 →「必填」、類別／曝露度不在清單 →「請從清單選擇」、重要性 →「重要性只能是 1–3」、元件缺版本 →「請填寫版本」、重複產品訊息用顯示名稱。完成條件：單元測試斷言空 `asset_tag` 的錯誤為「必填」且所有錯誤文字不含 `asset_tag`、`name`、`category`、產品 key
- [x] 1.3 `Workspace::save_asset_form` 等成功訊息改用產品顯示名稱。完成條件：`asset_edit` frontend 測試斷言修改 PAN-OS 版本的訊息含「PAN-OS 版本改為 11.1.6」且不含 `paloalto-panos`
- [x] 1.4 frontend 測試：`cargo test -p cvewatch-frontend` 通過（含 1.1～1.3 新增測試）

## 2. GUI：資產清單與詳情

- [x] 2.1 資產清單改為三欄表格（design D4）：名稱截斷並有提示、未處理數靠右且大於 0 時警示色、已退役整列灰字加「（已退役）」、名稱等於編號時名稱留空。完成條件：kittest 斷言 `FW-CORE` 列的未處理數為獨立節點「1」，`PVE-01`（名稱同編號）列不出現第二個「PVE-01」
- [x] 2.2 詳情：標題名稱不重複、元件與版本歷史用顯示名稱、類別與曝露度雙語、沒有命中時顯示「沒有命中」（已退役另註明不參與比對）。完成條件：kittest 斷言選取 `PVE-01` 時顯示「Proxmox VE 8.2.2」「server（伺服器）」「沒有命中」，且不顯示 `proxmox-ve 8.2.2`
- [x] 2.3 工具列分組與危險操作樣式（design D7、D8）：「刪除」改名「退役…」、「退役…」「永久刪除」警示色並與一般操作間隔；拖放提示移到空狀態與「匯入清冊」提示。完成條件：kittest 斷言使用中資產詳情有「退役…」而沒有「刪除」按鈕
- [x] 2.4 鍵盤：資產清單上 Enter 開啟編輯、Delete 開啟退役確認，文字欄位有焦點或有對話框時不作用（design D6）。完成條件：kittest 斷言選取 `FW-EDGE` 後按 Enter 開啟「編輯 FW-EDGE」、按 Delete 開啟「將 FW-EDGE 標為退役」
- [x] 2.5 GUI 測試：`cargo test -p cvewatch-gui asset` 通過（含 2.1～2.4，並更新 `asset_retire`、`asset_match` 中改名的按鈕）

## 3. GUI：編輯與確認對話框

- [x] 3.1 下拉選單只保留左側標籤並以 `labelled_by` 關聯；必填欄位標「*」；「加入元件」標籤移到選單之前；類別、曝露度選項雙語（design D3）。完成條件：kittest 斷言畫面不出現「選擇類別」，仍可以標籤「類別 *」開啟選單並選擇 `security`
- [x] 3.2 欄位錯誤顯示 1.2 的介面文字；本地驗證失敗不寫狀態列，取消或 Esc 關閉時清除對話框留下的寫入失敗訊息（design D5）。完成條件：kittest 重現 spec 的 Invalid field keeps the dialog open（顯示「必填」、不出現 `asset_tag`）與 Cancelled dialog leaves no error behind
- [x] 3.3 未辨識產品列改為「原文與版本 → 處置選單 → 刪除」；元件列用顯示名稱（版本欄標籤「Proxmox VE 版本」）。完成條件：kittest 以「Synology XYZ-300+ 7.2」所在列的處置選單改為目錄產品後儲存成功
- [x] 3.4 加入元件的焦點流程（design D6）：開啟選單時搜尋框取得焦點，選擇後焦點移到該元件的版本欄。完成條件：kittest 重現 spec 的 Keyboard flow when adding a component（開啟後直接輸入 `pan`、選擇 PAN-OS 後直接輸入 `11.2.3` 即寫入版本欄）
- [x] 3.5 退役確認標題改為「將 <編號> 標為退役」（design D7）。完成條件：kittest 重現 spec 的 Deleting from the asset list（按「退役…」、標題「將 NAS-02 標為退役」）
- [x] 3.6 GUI 測試：`cargo test -p cvewatch-gui` 全部通過（含 3.1～3.5，並更新 `asset_edit` 中改名的標籤）

## 4. 文件與整合驗證

- [x] 4.1 README「視窗版」資產編修段落與對照表改用「退役…」，並說明 Enter／Delete 快捷鍵。完成條件：README 中不再以「刪除」指稱退役按鈕
- [x] 4.2 全專案檢查：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`openspec validate 11-polish-asset-ux --strict` 全部通過。完成條件：四個指令皆成功
- [x] 4.3 以實際字型重新渲染 10 評估時的 8 張畫面（資產清單、詳情、編輯、欄位錯誤、加入元件、已退役詳情、永久刪除、退役確認），逐項對照評估的 15 項發現。完成條件：對照結果記錄於 design.md 的「驗證」章節，未改善的項目附理由

## Workflow follow-up

- archive 時同步主 spec `gui-console`（新增 GUI asset presentation，修改 GUI asset editing 與 GUI asset retirement and purge）
- archive 後執行 `openspec validate --all --strict` 確認通過
