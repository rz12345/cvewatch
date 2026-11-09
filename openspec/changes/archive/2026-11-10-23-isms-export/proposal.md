# Proposal

## Why

18 讓 ISMS 資訊資產清冊可以直接匯入 cvewatch，但方向只有一個：之後在 cvewatch 補上的版本、元件、廠牌型號與管理欄位，每年 ISMS 覆核時仍要回頭手動抄進 ISMS 清冊。cvewatch 已經有 ISMS 清冊需要的全部欄位（ISMS 編號、資產說明、子類別、C/I/A、管理欄位），補上反方向的匯出，ISMS 清冊與 cvewatch 才能互相對照、不必重打。

匯出檔必須能再以「從 ISMS 清冊匯入」讀回，且推測出的產品與原本相同，否則使用者把匯出檔交給 ISMS 窗口修改後再匯入時，資產會多出錯誤的未辨識產品。

## What Changes

- 新增 CLI `cvewatch asset export-isms <path.xlsx>`（與 `asset import-isms` 成對）：把 active 資產匯出為**獨立的新檔**（不填入使用者提供的範本），工作表「資訊資產清冊」，第 1–3 列為文件資訊、第 4 列為 ISMS v2.1 的 16 欄表頭、第 5 列起一列一個資產
  - 只匯出有 ISMS 編號的 active 資產，依 ISMS 編號排序；retired 資產不匯出
  - 沒有 ISMS 編號的 active 資產略過，完成後列出略過的資產編號
  - 欄位：資產編號兩欄（系統代碼 `NET-`、流水號 `HW-001`）、資產類別（子類別代碼）、資產說明（資產說明名稱）、資產名稱（子類別名稱）、資產描述、權責單位、保管人、使用單位、C/I/A、資產價值 MAX(C,I,A)、法規遵循性、防護等級、是否含個人資料
  - 資產描述：廠牌與型號（20 新增的欄位，有值時第一行）＋每個清冊元件與未辨識產品一行「產品（版本）」；版本待補只寫產品；產品用目錄顯示名稱；**不含**主機名稱與 IP
  - 廠牌型號行以「廠牌型號：【…】」寫出，ISMS 匯入的描述清理規則會把它整行去除，不會被推測成產品；版本放在括號內，讀回時同樣被去除
  - 匯出時以 ISMS 匯入的同一套描述規則檢查每一行；讀回結果與原本不同的資產列在警告中（不中止匯出）
  - 目的檔已存在時拒絕且不覆蓋；完成後顯示「此檔未加密」提示（含保管人等個人資訊）
- GUI 資產工具列清冊檔案組新增「匯出 ISMS 清冊」，緊接在「從 ISMS 清冊匯入」之後：另存新檔對話框（預設文件資料夾、檔名 `cvewatch-isms-YYYYMMDD.xlsx`），內容與 CLI 相同，完成後顯示路徑、匯出數、略過的資產與「此檔未加密」提示

## Capabilities

### New Capabilities

（無）

### Modified Capabilities

- `isms-inventory`：新增 ISMS export command、ISMS export layout、ISMS export field mapping、ISMS export description、ISMS export round trip
- `gui-console`：新增 GUI ISMS export

## Out of scope

- 把資料填入使用者提供的 ISMS 範本檔、保留範本的格式、公式、文件編號或「選項(勿刪)」工作表
- 匯出 PE、IF、SE 類資產；匯出 retired 資產
- 自動為沒有 ISMS 編號的資產配發 ISMS 編號
- 資產描述中寫入主機名稱、IP、序號、財產編號、管理介面、保固／合約／授權到期或 EoL（20、22 的欄位）
- 依 21 的元件角色分組或排序描述行（描述依元件建立順序）
- 一列多個資產、合併儲存格、把同一系統代碼合併成一格
- 匯出主機套件元件（與一般清冊匯出相同，只匯出清冊元件與未辨識產品）
- 以加密或口令保護匯出檔
- ISMS 匯入的規則（描述拆行、清理、比對）不因匯出而修改
- TUI 的匯出按鍵

## Impact

- `crates/core/src/assets/isms/`：新增 `export.rs`（列資料與報表，純函式）、`export_write.rs`（xlsx 版面）、`description.rs`（資產描述組成與讀回檢查）；`workbook.rs` 的 16 欄表頭由測試常數改為公開常數 `ISMS_V21_HEADERS`；`describe.rs` 的分隔字元與括號規則改為 `pub(crate)` 供描述組成共用
- `crates/core/src/assets/export.rs`：重用 `inventory_items_by_asset`、`product_text`、`personal_data_text`
- `crates/cvewatch`：`AssetAction::ExportIsms` 與 `commands/asset_isms.rs` 的輸出
- `crates/frontend/src/workspace/assets.rs`：`isms_export_default`、`export_isms`
- `crates/gui/src/screens/asset_list.rs`、`screens/assets.rs`：「匯出 ISMS 清冊」按鈕與另存流程
- `docs/user-guide/assets.md`、`gui.md`：ISMS 匯出的說明
- 依賴 20-asset-identity-network 的廠牌、型號欄位；不需要 migration，不新增 crate 相依（沿用 `rust_xlsxwriter`）
