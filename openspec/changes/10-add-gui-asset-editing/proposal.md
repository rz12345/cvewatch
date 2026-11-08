# Proposal

## Why

目前資產只能靠 Excel/CSV 清冊匯入維護：視窗版要改一個版本號，得先找到原始清冊、改完再匯入；手上沒有清冊檔時，也無法從資料庫取回目前的資產清單。09 的視窗版已提供匯入，本 change 補齊視窗版的資產管理：下載範本、匯出目前的清冊、在畫面上直接新增、修改、刪除資產與元件，讓資料庫與 Excel 可以雙向往返。

## What Changes

- 新增 CLI `cvewatch asset export <path.xlsx>`：把使用中資產匯出為與匯入格式相同的 Excel（`assets`、`components` 工作表、標準欄名），匯出後直接重匯的結果為「新增 0、更新 0」；目的檔已存在時拒絕
- core 新增資產編修操作，GUI 呼叫、CLI 與 TUI 日後可共用：
  - 新增資產（欄位驗證規則與匯入相同，至少一個元件）
  - 修改資產欄位（`asset_tag` 不可修改）
  - 新增元件、修改元件版本（記錄版本歷史）、移除元件
  - 把未辨識產品改為目錄產品，或刪除未辨識產品
  - 刪除 = 標為退役（與 `--prune` 相同，保留命中與處理紀錄）；已退役資產可恢復，或「永久刪除」（連同元件、命中、處理紀錄一起移除，無法復原）
  - 每次寫入在同一交易記錄「比對待補」，寫入後自動比對
- 視窗版資產畫面新增：
  - 「下載範本」：以另存新檔對話框儲存空白範本（同 `cvewatch asset template`）
  - 「匯出清冊」：以另存新檔對話框匯出使用中資產
  - 「新增資產」、「編輯」、「刪除（退役）」，以及元件與未辨識產品的編修
  - 「顯示已退役」切換，已退役資產提供「恢復」與「永久刪除」（需輸入資產編號確認）
  - 編修完成後於背景自動比對並重新載入
- 匯入預覽加註：匯入會以檔案內容覆寫資產欄位與元件，在視窗中修改過時建議先匯出再編輯
- 資產資料的來源由「只有清冊」改為「清冊匯入與畫面編修」兩者；資料庫為目前狀態，清冊為交換格式

## Capabilities

### New Capabilities

（無）

### Modified Capabilities

- `asset-inventory`：新增清冊匯出、資產與元件編修、退役資產恢復與永久刪除的規則
- `gui-console`：新增視窗版的下載範本、匯出清冊、資產編修畫面；匯入預覽加註覆寫提醒（`gui-console` 由 09 建立，本 change 須在 09 archive 後才能 apply）

## Out of scope

- TUI 與 CLI 的資產編修指令（core 操作可共用，但本 change 只做 GUI；CLI 只新增 `asset export`）
- 匯出 CSV、匯出已退役資產、匯出時套用 `[import.columns]` 自訂欄名
- 修改 `asset_tag`（改名請新增新資產後刪除舊資產）
- 在畫面上新增或修改產品目錄（`catalog.d`）；編修時只能選既有目錄產品
- 編修主機套件元件（sbom / dpkg / rpm 來源），仍只能以 `asset import-packages` 整批取代
- 多筆資產批次編修、復原（undo）、資產異動稽核紀錄（元件版本歷史以外）
- GUI 編修時通知其他已開啟的 TUI / GUI 視窗重新載入

## Impact

- `crates/core/src/assets`：新增 `export.rs`（匯出）、`edit.rs`（編修操作）；`validate.rs` 抽出單筆資產欄位驗證供匯入與編修共用；`repo.rs` 增加恢復與永久刪除
- `crates/cvewatch`：`AssetAction::Export` 子命令
- `crates/frontend`：`Workspace` 新增資產編修方法（回傳 `Report`），表單狀態與驗證
- `crates/gui`：資產畫面按鈕、資產／元件編輯對話框、永久刪除確認對話框、背景比對（companion `cvewatch match`）
- 不新增資料庫 migration：畫面新增的元件沿用 `source = 'excel'`（代表「清冊管理的元件」），永久刪除依既有 `ON DELETE CASCADE`
- README「視窗版」與「資產清冊」章節更新
