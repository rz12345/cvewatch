# Design

## Context

- 本 change 建立在 16-isms-admin-fields 之上：資產說明（`asset_system`、`SystemDirectory`）、`systems` 工作表、`custodian` 等管理欄位，以及 `RecordContext` 驗證。apply 前 16 必須已 archive，主 spec 也已同步。
- 清冊讀取用 `calamine`（`sheet.rs` 的 `open_workbook_auto` 與 `cell_text`），寫出用 `export.rs` 的工作表寫出函式（範本、匯出共用）。匯入只讀 `assets`、`components`、`systems`，其他工作表會被忽略，所以 `isms_report` 不影響匯入。
- 產品目錄 `Catalog::resolve` 用正規化後的完全比對別名，`Catalog::suggest` 提供相似別名。硬體型號的別名由使用者寫在 `catalog.d`。
- ISMS 清冊的實際樣貌（使用者的 v2.0 與空白 v2.1）：
  - 第 1–3 列是文件資訊，第 4 列是表頭；兩欄都叫「資產編號」；資料列之後有合併儲存格的編碼說明列
  - 「資產名稱」等於子類別名稱
  - v2.1 的部分欄位是 VLOOKUP 公式。`calamine` 讀到的是快取值；沒有快取值時是空白或 `#N/A`
- GUI 寫檔走 `frontend::workspace` 呼叫 core（範本、匯出都是如此），檔案對話框透過 `gui/src/files.rs` 的 `Files` trait，可在測試中替換。

## Goals / Non-Goals

**Goals:**
- 草稿只是一般的 cvewatch 清冊檔：匯入規則、預覽與錯誤訊息完全沿用 `asset import`，不另開「ISMS 匯入」的寫入路徑
- 轉換結果可重現：同一份 ISMS 清冊與同一個資料庫狀態，產生的草稿內容相同（列序依 ISMS 清冊）
- 轉換可以容錯：個別列的問題只影響那一列

**Non-Goals:**
- 不寫資料庫，也不在轉換時執行比對
- 不嘗試理解自由文字的描述（例如「CISCO 交換器數台」），比對不到就原文寫出，交給使用者處理

## Decisions

### D1. 模組結構：讀取、轉換、輸出分開

新模組 `crates/core/src/assets/isms/`：

| 檔案 | 責任 |
|---|---|
| `workbook.rs` | 找出表頭列與欄位（`IsmsColumns`），產生 `IsmsRow`（列號＋各欄原文） |
| `row.rs` | 單列轉換：ISMS 編號、大類過濾、欄位驗證，產生 `DraftAsset` 或 `SkipReason` |
| `describe.rs` | 資產描述拆行、清理、目錄比對、重複合併 |
| `draft.rs` | 整批組裝：沿用既有資產、`systems`、報表；`build_draft(&IsmsBook, &DraftContext) -> Draft` |
| `report.rs` | 報表結構與文字（CLI 摘要、`isms_report` 工作表、GUI 摘要共用） |

`DraftContext` 是不可變快照，包含子類別表、產品目錄、資產說明目錄，以及「ISMS 編號 → 既有資產（含狀態、欄位與清冊元件）」對照表，由呼叫端從資料庫載入。`build_draft` 因此是純函式，測試不需要資料庫，也不需要 Excel 檔。

### D2. 表頭與欄位辨識

- 欄名正規化：NFKC、去除所有空白與換行、去除括號（含全形括號）及括號內的文字。例如「機密性\n(A)」會變成「機密性」，「資產\n價值\n(MAX ABC)」會變成「資產價值」。
- 工作表的選擇：優先使用名稱為「資訊資產清冊」的工作表；沒有的話，依序找第一個在前 10 列中有表頭的工作表。「選項(勿刪)」沒有「資產描述」與「資產編號」同時出現的列，所以不會被誤選。
- 兩個「資產編號」依欄位順序決定，第一個是系統代碼、第二個是流水號。只有一個時，視為缺少必要欄位。
- 儲存格值是 `#N/A` 等錯誤值時視為空白（v2.1 未填列的公式結果）。這種列的第二欄資產編號是空的，所以不是資產列。
- 替代方案「固定第 4 列為表頭、固定欄序」：v2.0 與 v2.1 目前一致，但單位自行增減欄位或插入說明列時就會讀錯，所以不採用。

### D3. 列的驗證沿用 16 的 `RecordContext`

`row.rs` 把 ISMS 欄位組成 `RecordInput`（asset_tag 先用 ISMS 編號），交給 `validate_record` 驗證。驗證失敗的 `FieldError` 轉成 `SkipReason`（附列號、欄位與原因），不中止整批。所以 ISMS 編號格式、大類不符、子類別未定義、C/I/A 範圍等規則與匯入完全相同，錯誤文字也一致。

例外：
- 法規遵循性的 `-`、`－` 先轉成空白再驗證。
- 是否含個人資料只接受 Y／N（ISMS 清冊的寫法），與 cvewatch 清冊可另外接受「是／否」並不衝突。
- 驗證時的資產說明目錄是「資料庫＋這次草稿的 systems」，所以新出現的系統代碼不會被判定為尚未定義。

### D4. 描述拆行與比對（`describe.rs`）

1. 以 `\n`、`、`、`，`、`,`、`；`、`;` 拆開，去除空行。
2. 每行依序去除：冒號（`:`、`：`）及其前方的標籤、括號及括號內的註記、結尾的數量（`*N`、`xN`、`×N`，可有空白）。
3. 以 `Catalog::resolve` 比對。比對不到時，若該行以中文字開頭且後面有英數字，就去除開頭連續的中文字詞（含其後空白）再比對一次。
4. 比對結果：`Resolved { key, lines }`，同一個 key 的多行在這一步合併，記錄合併的原文行；或 `Unresolved { text }`，同時附上 `Catalog::suggest` 的建議別名，寫入報表。
5. 未辨識的行在草稿中以清理後的文字寫入 `product`。

替代方案「用 `suggest` 自動挑最相近的產品」：Jaro-Winkler 0.85 的門檻可能把不同型號配到錯的產品，這會直接影響 CVE 比對結果，所以只列為建議，不自動套用。

### D5. 草稿中的資產與元件

- 草稿 `assets` 一列帶第一個元件，其餘元件列在 `components`。這與 `inventory_rows`／匯出的格式相同，寫出時直接重用 `export.rs` 的列組裝函式（需要時把它改為接收資料結構，不直接查資料庫）。
- 沒有任何描述行的資產：`product`、`version` 都空白，報表列為「沒有元件」。匯入時會因缺少 product 中止，使用者必須補上。
- 沿用既有資產（spec 的 Existing assets are reused）時：先寫出該資產所有清冊元件（含版本）與未辨識產品，再加入描述中有、但該資產沒有的目錄產品。描述中未辨識的行，如果和該資產既有未辨識產品的原文相同（正規化後比較），就不重複加入。
  - 這是必要的：匯入會移除檔案中沒有的清冊元件。少寫一個，匯入就會刪掉使用者已有的元件。
- 名稱的清理沿用 D4 第 2 步（去除標籤、註記與數量），取第一行。沿用既有資產時用既有名稱。

### D6. 草稿報表

`DraftReport` 欄位：`converted_new`、`converted_existing`、`skipped_by_major`（PE/IF/SE 計數）、`skipped_rows`（列號＋原因）、`merged`（ISMS 編號＋合併的原文行）、`unresolved_lines`（ISMS 編號＋原文＋建議）、`missing_versions`（元件數）、`not_in_workbook`（active 資產編號）、`system_name_conflicts`。

- CLI：印出計數摘要與每類的前 20 筆明細，超過時提示查看 `isms_report` 工作表。
- `isms_report` 工作表：兩欄「項目、內容」，每筆一列。不加表頭以外的格式，使用者可以自行篩選。
- GUI：顯示計數摘要與路徑，明細請使用者開啟草稿查看。報表很長時，在 GUI 顯示會難以閱讀。

### D7. CLI 與 GUI 入口

- CLI：`AssetAction::IsmsDraft { workbook, output }`，處理放在 `commands/asset_isms.rs`，避免 `asset.rs` 持續變大。開啟資料庫（唯讀使用）→ 載入 `DraftContext` → 讀取 ISMS 清冊 → `build_draft` → 寫檔。目的檔存在時，在讀取任何東西之前就先拒絕。
- 結束碼：草稿成功寫出時為 0，即使有略過的列也一樣；表頭錯誤、檔案無法開啟、目的檔已存在時為非 0。
- GUI：`Files` trait 新增 `open_isms_workbook()`（只篩選 .xlsx）。`frontend::workspace` 新增 `isms_draft_default()`（文件資料夾、`cvewatch-isms-draft-<日期>.xlsx`）與 `write_isms_draft(src, dst) -> Report`。按鈕放在清冊檔案組的「匯出清冊」之後。不需要 `cvewatch.exe` 輔助程式，因為不寫資料庫，也不執行比對。

### D8. 測試策略

- 單元測試（不需要檔案）：欄名正規化、ISMS 編號組成、`describe.rs` 的拆行、清理、中文前綴重試與合併、`row.rs` 的各種略過原因、`draft.rs` 的沿用既有資產（含元件保留與未辨識產品去重）、systems 名稱衝突。
- fixture：`gen_inventory_fixtures.rs` 新增 `isms_workbook.xlsx`。它模擬 ISMS 版面：文件資訊列、兩欄資產編號、PE/IF/SE 列、大類不符列、多行描述、同產品兩行、`－`、說明列，以及含 `#N/A` 的空白公式列。所有內容都是虛構的（系統代碼 `NET`、`WEB`，人名「王小明」，型號 `XYZ-100`），測試用的別名寫在測試專用的 `catalog.d`。
- 整合測試：
  - CLI 對 fixture 產生草稿，斷言工作表與報表
  - 草稿補上版本（測試中改寫）後以 `asset import` 匯入成功，未補時中止
  - 資料庫已有同 ISMS 編號的資產時，匯入結果為「更新 1」
- GUI：以替身 `Files` 測試按鈕流程、取消與錯誤顯示。

## Risks / Trade-offs

- [描述的寫法千變萬化，比對率可能很低] → 比對不到的行原文保留並附建議別名。使用者在 `catalog.d` 加入別名後重新產生草稿即可，草稿可重複產生。
- [沿用既有資產時，ISMS 清冊的 C/I/A 與管理欄位會覆蓋 cvewatch 中修改過的值] → spec 已規定 ISMS 欄位以 ISMS 清冊為準；匯入預覽會列出欄位差異，使用者確認後才寫入。
- [合併重複產品會遺失「兩處版本不同」的資訊] → 報表會警告，並提示版本不同時要拆成多個資產（使用者已確認此作法）。
- [使用者以 `--prune` 匯入只含 ISMS 列的草稿，會把其他資產標為退役] → 報表與 GUI 摘要都提醒不要使用 `--prune`。不另外加檔案標記來阻擋，以免草稿與一般清冊的匯入行為不同。
- [v2.1 公式欄沒有快取值（檔案由其他程式產生，未經 Excel 存檔）] → 子類別、C/I/A 會讀成空白：子類別空白時該列被略過並說明；C/I/A 空白時帶入子類別範本值（與匯入相同）。
- [ISMS 清冊含真實姓名與內部編號] → 只在使用者電腦上讀取，草稿寫到使用者選的路徑；fixture 全為虛構內容，commit 前執行 `scripts/check-sensitive.ps1`。

## Migration Plan

沒有資料庫 migration。新指令與按鈕不影響既有功能；回退時直接換回舊版即可。

## Apply 時的決定（依建議做法，未另行詢問）

- **D3 補充：編號大類為 PE/IF/SE、類別為 HW/SW 的列**：資產驗證的 ISMS 編號格式只認得 HW、SW，`WEB-SE-003` 會被回報成「格式錯誤」。`row.rs` 先檢查這種情況，以與資產驗證相同的寫法回報「ISMS 編號「WEB-SE-003」的大類 SE 與資產類別 SW01 不符」。訊息帶「」與匯入錯誤一致，spec 的 Major class mismatch 情境文字同步加上「」。
- **D3 補充：是否含個人資料**的錯誤訊息為「是否含個人資料「是」只能填 Y 或 N」；法規遵循性的 `－` 經 NFKC 轉成 `-` 後視為未評定。
- **D2 補充**：文字形式的 Excel 錯誤值（`#N/A`、`#REF!` 等）也視為空白；工作表的使用範圍不從 A1 開始時，列號與欄位置換算成試算表的絕對位置。「資訊資產清冊」工作表存在但沒有表頭時，改找其他工作表。
- **D4 補充**：結尾數量的 `x` 前面必須是空白（`ABX 100` 不會被當成數量）；可帶「台、部、套、組、個」單位。相似產品取自 `Catalog::suggest`，回傳的是產品 key（不是別名原文）。
- **D5 補充：名稱的後備**：描述為空時用子類別名稱；子類別也無法辨識時用 ISMS 編號（避免多出「缺少 name」的錯誤）。
- **D5 補充：既有資產的比對**：先以 ISMS 編號（不分大小寫）比對；沒有時，比對「asset_tag 與 ISMS 編號相同（不分大小寫）、且沒有其他 ISMS 編號」的資產。否則草稿會以同一個 asset_tag 更新該資產，並因為少列既有元件而在匯入時移除它們。沿用時保留既有 ISMS 編號的寫法（大小寫），避免匯入出現只有大小寫不同的欄位變更。
- **D5 補充：未辨識的描述行**即使因為既有資產已有相同的未辨識產品而不重複加入，仍列入報表。
- **D6 補充**：`--prune` 提醒一律顯示（不只在有「不在 ISMS 清冊」的資產時），CLI、`isms_report` 與畫面一致；草稿的 `systems` 名稱與資料庫中其他資產說明衝突（名稱重複、名稱等於其他代碼）時也列入 `system_name_conflicts`，因為匯入會因此中止。
- **D7 補充：沒有資料庫時**：CLI 在 `open_working_db` 回報資料庫不存在時，仍以空的 `DraftContext` 產生草稿並提示「草稿不參考既有資產」。尚未執行 `cvewatch init` 的使用者也能先準備草稿；CLI 測試因此不需要 Credential Manager。金鑰錯誤等其他錯誤照常中止。
- **D7 補充：GUI**：檔案對話框的 trait 實際名稱是 `FileDialogs`（`gui/src/files.rs`），在其中新增 `open_isms_workbook()`。另存對話框標題為「儲存 ISMS 草稿」；任一對話框取消時狀態列顯示「已取消產生 ISMS 草稿」。有略過的列或資產說明警告時，狀態列為警告層級。
- **D5／D7：重用 `export.rs`**：抽出 `InventoryItem`（含產品 key）、`inventory_items`、`asset_row`、`component_rows` 與 `inventory_sheets`，清冊匯出與草稿共用；匯出行為與既有測試不變。
- **D8 補充：fixture**：除了 `isms_workbook.xlsx`，另有 `isms_missing_column.xlsx`（缺「可用性」）與 `isms_not_workbook.xlsx`。測試專用別名放在 `tests/fixtures/isms_catalog.d/isms-test.toml`，core 單元測試（`include_str!`）、core 整合測試、CLI 測試共用。
- **任務 5.3 的手動確認**：自動化流程無法操作視窗，所以改以相同的 core 函式（`cvewatch asset isms-draft`，暫存資料目錄＋本機 `catalog.d` 副本、不開啟資料庫）讀取使用者的 v2.0 清冊。結果為「轉換 15 個資產、略過 30 列（PE 4、IF 17、SE 3，另有 6 列是編號為 SE 但類別為 SW 的軟體列）」，與已知的清冊內容相符。草稿只寫在暫存區，確認後已刪除。

## Code review 後的修正（一輪，依建議做法）

- **asset_tag 與既有資產相同**：新資產以 ISMS 編號為 asset_tag。若另一個既有資產（ISMS 編號不同、未被沿用）已使用這個 asset_tag，匯入會更新那個資產、覆寫欄位並移除草稿沒列出的元件。所以略過該列，報表說明「ISMS 編號 X 與既有資產 Y（ISMS 編號 Z）的 asset_tag 相同；請先以 cvewatch asset rename 更改該資產的編號」。兩列對應到同一個資產時，略過後一列，避免匯入因重複而中止。
- **垂直合併儲存格**（D2 補充）：手工維護的清冊常把同一系統代碼、資產說明合併成一格，calamine 只在左上角有值。讀取 .xlsx 時，只有一欄寬、跨兩列以上的合併範圍才填入左上角的值。橫跨多欄的合併（文件資訊列、編碼說明列）不處理，避免說明文字被填進資產編號欄。
- **逐一載入工作表**（D2 補充）：依「資訊資產清冊優先、其餘依序」逐一載入，找到表頭就停止。用不到的工作表讀取失敗時只記日誌並略過；第一個檢查的工作表讀不到時才是錯誤。檔案讀取移到 `workbook_read.rs`，`workbook.rs` 只保留純函式。
- **大類判斷**（D3 補充）：資產類別寫的是子類別名稱（例如「通訊設備」）時，依子類別決定大類；`PE01` 這類代碼寫法取前兩碼；其他寫法（例如 PE 類的「人員」）依流水號前兩碼。名稱後備與大類不符的檢查也改用解析後的子類別。
- **冒號標籤**（D4 補充）：只有冒號前最多 2 個詞、16 個字、不含數字，且冒號後不是 `/` 時，才當成標籤去除。URL、時間與 `Windows Server 2019: Standard` 這類名稱保留原文。
- **數量**（D4 補充）：移除「整行只有 `x` 加數字」的規則，避免 `X100` 被刪掉；` x32`、` x64`、` x86` 沒有單位時視為平台寫法而保留。
- **法規遵循性的破折號**：除了 `-`、`－`，也接受 `‐ ‑ ‒ – — ― − ー` 這類單一破折號（Excel 自動校正常產生），視為未評定。
- **既有元件的載入**：`DraftContext::load` 改用 `inventory_items_by_asset` 一次讀出所有資產的清冊元件與未辨識產品，固定兩個查詢，不再每個資產各查兩次。清冊匯出的單一資產讀取共用同一段 SQL。
- **重複程式**：frontend 的「文件資料夾＋檔名含日期」改為共用的 `dated_xlsx_default`；CLI 的未加密提醒沿用 `asset.rs` 的常數。
- **測試的型號**：單元測試改用虛構型號（`Zyxel ABX 100`），不使用真實型號。

## 架構圖判斷

架構圖：不需更新。本 change 只新增 core 內部模組 `assets/isms/`（讀取使用者選擇的本機 xlsx，沿用既有的 `calamine`／`rust_xlsxwriter`）、既有 `cvewatch` 執行檔的子指令 `asset isms-draft`，以及 GUI 資產畫面的按鈕與檔案對話框（沿用既有的 `rfd`）。workspace `members`、各 crate 間的 `path =` 相依與執行檔都沒有改變；沒有新增或改換外部系統、HTTP 端點、通知管道、Credential Manager 或工作排程器的互動，GUI 也不以子程序啟動 `cvewatch.exe`。
