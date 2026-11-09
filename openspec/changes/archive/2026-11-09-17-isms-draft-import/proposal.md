# Proposal

## Why

使用者已在機構的 ISMS「資訊資產清冊」（v2.0／v2.1 Excel）登錄了資產，要把同一批資產建進 cvewatch 時，得把資產說明、類別、C/I/A、權責單位、保管人等再輸入一次。這份清冊不能直接匯入：它沒有版本，而且一列常列出好幾款設備（`*2`、`*6`）。cvewatch 的資產需要產品與版本才能比對 CVE。

16 補齊 ISMS 管理欄位與資產說明之後，ISMS 清冊大部分欄位在 cvewatch 都有對應的欄位。這時可以把 ISMS 清冊轉成 cvewatch 格式的清冊草稿：ISMS 已有的資料直接帶入，使用者只要補版本，再用現有的清冊匯入。

## What Changes

- 新增 `cvewatch asset isms-draft <ISMS清冊.xlsx> <草稿.xlsx>`：讀取 ISMS 清冊，產出 cvewatch 清冊格式的草稿（`assets`、`components`、`systems` 工作表，加上一張匯入時會被忽略的 `isms_report` 工作表）。目的檔已存在時拒絕
- 讀取規則：
  - 依表頭文字找出表頭列與欄位，不依固定列號。v2.0 與 v2.1 都適用
  - 兩欄「資產編號」組成 ISMS 編號（`NET-` ＋ `HW-001` → `NET-HW-001`）
  - 只轉換 HW、SW 列；PE、IF、SE 列略過並計數
  - ISMS 編號格式錯誤、大類與資產類別不符、子類別未定義、ISMS 編號重複的列：略過該列，在報表中警告，不中止整批
- 對應方式：ISMS 一列 → 一個資產。資產說明 → `systems`；類別、C/I/A、權責單位、保管人、使用單位、法規遵循性、是否含個人資料 → 對應欄位
- 資產描述 → 元件：
  - 依換行與頓號拆成多行，去除數量（`*2`）與「Core:」這類標籤後，以產品目錄別名比對
  - 比對到的行成為目錄產品元件，比對不到的行以原文寫入（匯入後為未辨識產品）
  - 版本留空，等使用者補上
  - 同一列有兩行比對到同一個產品時，合併成一個元件，並在報表中警告
- 參考資料庫：ISMS 編號已屬於 cvewatch 的某個資產時，草稿沿用該資產的編號、名稱、曝露度、地點、備註與所有清冊元件（含已知版本）。之後匯入就是「更新」，不會因 ISMS 編號重複而失敗，ISMS 清冊改版後也可以重新產生草稿
- 草稿報表：略過的列與原因、合併的產品、未辨識的描述行、待補版本的元件數、資料庫中有但 ISMS 清冊沒有的資產。CLI 印出摘要，`isms_report` 工作表保存明細
- GUI：資產畫面的清冊檔案組新增「由 ISMS 清冊產生草稿」，選擇 ISMS 清冊與草稿存檔位置後產生草稿，並顯示報表摘要與檔案路徑

## Capabilities

### New Capabilities

- `isms-inventory`：讀取機構 ISMS 資訊資產清冊，並與 cvewatch 清冊互相轉換。本 change 先做「ISMS 清冊 → cvewatch 清冊草稿」；21 的 ISMS 格式匯出之後也放在這裡

### Modified Capabilities

- `gui-console`：新增「GUI ISMS draft」

## Out of scope

- 直接把 ISMS 清冊寫入資料庫（草稿一律經使用者補版本後，再用 `asset import` 匯入）
- 匯出 ISMS 格式（21-isms-export）
- 從 ISMS 描述推測版本，或推測曝露度
- 把一列拆成多個資產（同型號多台設備只記一個元件與版本）
- 同一資產有多個相同產品的元件
- PE、IF、SE 類資產
- 讀取 ISMS 清冊的「選項(勿刪)」工作表（資產說明名稱取自各列的「資產說明」欄）
- 為比對不到的描述行自動新增目錄別名（使用者在 `catalog.d` 自行加入，或匯入後在畫面改為目錄產品）

## Impact

- 依賴：16-isms-admin-fields（資產說明、`systems` 工作表、管理欄位）須先 archive
- `crates/core/src/assets/isms/`（新模組）：ISMS 清冊讀取、列轉換、描述拆行、草稿與報表組裝；寫檔沿用 `export.rs` 的工作表寫出
- `crates/cvewatch`：`cli.rs` 新增 `asset isms-draft`，並新增指令處理
- `crates/frontend/src/workspace/`：產生草稿的操作與報表；`crates/gui`：資產工具列按鈕、開檔與存檔對話框、報表顯示
- `tests/fixtures`：由 `gen_inventory_fixtures.rs` 產生虛構的 ISMS 清冊（文件資訊列、PE/IF/SE 列、說明列、大類不符、多行描述、重複產品）
- `docs/`：使用說明新增「由 ISMS 清冊建立資產」
