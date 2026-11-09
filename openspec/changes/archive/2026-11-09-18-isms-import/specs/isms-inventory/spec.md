# Spec Delta

## ADDED Requirements

### Requirement: ISMS import command
系統 SHALL 提供 `cvewatch asset import-isms <ISMS清冊.xlsx> [--dry-run]`，讀取 ISMS 資訊資產清冊，依本 capability 的規則新增或更新資產。`--dry-run` SHALL 只輸出預覽，不寫入資料庫。寫入 SHALL 在單一交易內完成，任何一筆寫入失敗時資料庫 MUST NOT 變更；被略過的列不影響其他列的匯入。匯入 MUST NOT 把任何資產標為退役。寫入後 SHALL 依 vuln-matching 的 Matching runs automatically 自動比對。資料庫尚未初始化時 SHALL 以錯誤結束並提示先執行 `cvewatch init`。

#### Scenario: Importing an ISMS workbook
- **WHEN** 使用者執行 `cvewatch asset import-isms D:\isms.xlsx`，清冊有 6 列可轉換的 HW／SW 列，資料庫中都還沒有這些 ISMS 編號
- **THEN** 資料庫新增 6 個資產，指令印出匯入摘要，之後自動執行比對

#### Scenario: Previewing an ISMS import
- **WHEN** 使用者以 `--dry-run` 執行同一指令
- **THEN** 指令印出與實際匯入相同的預覽內容，資料庫不變

#### Scenario: Database not initialized
- **WHEN** 尚未執行 `cvewatch init`
- **THEN** 指令以錯誤結束並提示先執行 `cvewatch init`，不讀寫任何資料

#### Scenario: Re-importing the same ISMS workbook
- **WHEN** 同一份 ISMS 清冊連續匯入兩次
- **THEN** 第二次的結果為「新增 0、更新 0」，沒有新增的版本待補元件或未辨識產品

### Requirement: ISMS import report
ISMS 匯入的預覽與完成摘要 SHALL 列出：新增與更新的資產數；每個更新資產變更的 ISMS 欄位與新舊值；新增的版本待補元件數；依大類略過的列數；略過的列與原因；合併的產品；未辨識的描述行；資產說明的新增與改名；資料庫中 active、但不在這次匯入結果中的資產。

#### Scenario: ISMS fields changed for an existing asset
- **WHEN** 資料庫的 `FW-01` 有 ISMS 編號 `NET-HW-002`、C/I/A 2/3/4、保管人「王小明」，ISMS 清冊的 `NET-HW-002` 列 C/I/A 為 3/3/4、保管人「李小華」
- **THEN** 預覽列出 `FW-01` 的機密性 `2 → 3` 與保管人「王小明 → 李小華」

#### Scenario: Assets missing from the ISMS workbook
- **WHEN** 資料庫有 active 資產 `PVE-01`，沒有 ISMS 編號
- **THEN** 摘要列出 `PVE-01` 不在 ISMS 清冊中，`PVE-01` 維持 active

## MODIFIED Requirements

### Requirement: Reading the ISMS workbook
系統 SHALL 在工作表前 10 列中，找出同時含「資產編號」「資產類別」「資產描述」的列作為表頭，並依表頭文字（忽略空白、換行與括號註記）決定各欄位置。兩欄「資產編號」中，第一欄 SHALL 視為系統代碼、第二欄為流水號。必要欄位為兩欄資產編號、資產類別、資產說明、資產描述、機密性、完整性、可用性；缺少時 SHALL 以錯誤結束並列出缺少的欄名。第二欄資產編號為空的列 SHALL 不視為資產列。只有一欄寬、跨多列的合併儲存格 SHALL 視為整個範圍都是左上角的值；橫跨多欄的合併儲存格不展開。

#### Scenario: Header below document information rows
- **WHEN** ISMS 清冊第 1–3 列是文件編號與填表日期，第 4 列是表頭
- **THEN** 系統以第 4 列為表頭，從第 5 列開始讀取資產

#### Scenario: Footnote rows
- **WHEN** 清冊最後幾列是合併儲存格的編碼說明，第二欄資產編號為空
- **THEN** 這些列不列為資產，也不出現在報表的略過清單中

#### Scenario: Merged system code cells
- **WHEN** 第 5–7 列的第一欄資產編號合併成一格，內容為 `NET-`
- **THEN** 第 5、6、7 列的系統代碼都是 `NET`

#### Scenario: Not an ISMS workbook
- **WHEN** 使用者指定的檔案沒有含「資產編號」「資產類別」「資產描述」的列
- **THEN** 指令以錯誤結束，說明找不到 ISMS 資訊資產清冊的表頭，資料庫不變

#### Scenario: Missing required column
- **WHEN** 表頭沒有「可用性」欄
- **THEN** 指令以錯誤結束並指出缺少「可用性」欄

### Requirement: Rows outside HW and SW
資產類別不是 HW 或 SW 大類（PE、IF、SE）的列 SHALL 略過（資產類別為已定義子類別的代碼或名稱時依子類別判斷，其他寫法依流水號的前兩碼），報表 SHALL 依大類列出略過的列數，不逐列警告。

#### Scenario: Personnel and information rows
- **WHEN** 清冊有 3 列 PE、5 列 IF、2 列 SE 與 6 列 HW／SW，資料庫中沒有這些 ISMS 編號
- **THEN** 匯入新增 6 個資產，報表顯示「略過 PE 3、IF 5、SE 2」

### Requirement: Invalid rows are skipped
HW／SW 列有下列情形時，系統 SHALL 略過該列，在報表中列出列號與原因，並繼續處理其他列：ISMS 編號格式錯誤、大類與資產類別不符、子類別未定義、ISMS 編號與前面的列重複（不分大小寫）、新資產的 ISMS 編號與另一個既有資產的 asset_tag 相同、兩列對應到同一個既有資產、新資產的資產描述空白、C/I/A 有填但不是 1–4、C/I/A 空白而子類別沒有範本值、法規遵循性有填但不是 1–4、是否含個人資料有填但不是 Y 或 N、對應的既有資產有產品已不在目錄中的元件（匯入會移除它）、對應的既有資產有已可由目錄辨識為既有元件產品的未辨識產品（匯入時重複）。空白的 C/I/A 與是否含個人資料 SHALL 依 asset-inventory 的規則帶入子類別的值；法規遵循性為 `-`、`－` 等單一破折號或空白時 SHALL 視為未評定。

#### Scenario: Major class mismatch
- **WHEN** 第 44 列兩欄資產編號為 `WEB-` 與 `SE-003`，資產類別為 `SW01`
- **THEN** 這一列不匯入，報表列出「第 44 列：ISMS 編號「WEB-SE-003」的大類 SE 與資產類別 SW01 不符」，其他列照常匯入

#### Scenario: Asset tag used by another asset
- **WHEN** 資料庫的資產 `NET-HW-001` 的 ISMS 編號為 `ABC-HW-009`，ISMS 清冊有 `NET-HW-001` 列
- **THEN** 這一列不匯入，報表說明 ISMS 編號 `NET-HW-001` 與既有資產的 asset_tag 相同，須先更改該資產的編號

#### Scenario: Blank description for a new asset
- **WHEN** 第 12 列的 ISMS 編號在資料庫中沒有對應的資產，資產描述空白
- **THEN** 這一列不匯入，報表說明該列沒有資產描述、無法建立元件

#### Scenario: Existing component no longer in the catalog
- **WHEN** 資料庫的 `FW-02` 有 ISMS 編號 `NET-HW-003`，其中一個元件的產品已從 `catalog.d` 移除
- **THEN** 這一列不匯入，報表說明該元件已不在產品目錄中，`FW-02` 的元件不變

#### Scenario: Compliance dash
- **WHEN** 某列的法規遵循性為 `－`
- **THEN** 該列照常匯入，資產的法規遵循性為未評定

### Requirement: ISMS field mapping
每個匯入的列 SHALL 對應一個資產：新資產的 `asset_tag` 與 `isms_id` 為 ISMS 編號；`category` 為資產類別，`system` 為系統代碼；C/I/A、權責單位、保管人、使用單位、法規遵循性與是否含個人資料 SHALL 帶入對應欄位。新資產的 `name` SHALL 為資產描述第一行去除數量與標籤後的文字（為空時用子類別名稱），`notes` SHALL 為「ISMS 描述：」加上資產描述原文，曝露度 SHALL 為預設值 internal。資產價值與防護等級欄 SHALL 被忽略。

#### Scenario: Mapping a storage row
- **WHEN** 某列為 `NET-`、`HW-003`、`HW05`、資產說明「網路服務」、資產描述「Synology XYZ-200 *1」、保管人「王小明」、C/I/A 2/3/4、是否含個人資料 `N`，資料庫中沒有 `NET-HW-003`
- **THEN** 匯入後有資產 `NET-HW-003`：類別 `HW05`、資產說明 `NET`、名稱「Synology XYZ-200」、保管人「王小明」、C/I/A 2/3/4、不含個人資料、曝露度 internal、notes 為「ISMS 描述：Synology XYZ-200 *1」

### Requirement: Systems from the ISMS workbook
匯入的列中出現的每個系統代碼 SHALL 依 asset-inventory 的 Inventory systems sheet 規則新增或更新資產說明，名稱取自該代碼第一個非空的「資產說明」。同一代碼出現不同的資產說明時，SHALL 使用第一個並在報表警告。某代碼的資產說明都是空白時，SHALL 使用資料庫中該代碼的名稱；資料庫也沒有時，SHALL 以代碼作為名稱。名稱與其他資產說明衝突時，匯入 SHALL 中止並說明原因，資料庫不變。

#### Scenario: Two names for one system code
- **WHEN** 第 5 列與第 9 列的系統代碼都是 `NET`，資產說明分別為「網路服務」與「網路服務組」
- **THEN** 資產說明 `NET` 的名稱為「網路服務」，報表警告第 9 列的資產說明與第 5 列不同

#### Scenario: System name used by another code
- **WHEN** 資料庫的資產說明 `SYS` 名稱為「核心系統」，ISMS 清冊 `NET` 列的資產說明也是「核心系統」
- **THEN** 匯入中止，說明名稱「核心系統」已由 `SYS` 使用，資料庫不變

### Requirement: Description to components
系統 SHALL 依換行、頓號、逗號與分號把資產描述拆成多行。每行 SHALL 去除數量（`*2`、` x2`、`×2`；` x64` 這類平台寫法保留）、冒號前的標籤（例如「Core:」；URL 與含數字的名稱中的冒號不是標籤）與括號內的註記，再依 product-catalog 的 Alias resolution 比對。比對不到時，SHALL 去除開頭的中文字詞後再比對一次。比對到的行 SHALL 成為該產品的版本待補元件；仍比對不到的行 SHALL 以去除數量與標籤後的原文成為版本待補的未辨識產品。

#### Scenario: Multi-line description
- **WHEN** `catalog.d` 為 `hpe-aoscx` 宣告了別名「Aruba XYZ-100」，某列資產描述為「Core: Aruba XYZ-100 *2、\nZyxel ABCD *1」
- **THEN** 該資產有一個版本待補的 `hpe-aoscx` 元件與一筆原文為「Zyxel ABCD」、版本待補的未辨識產品，報表列出「Zyxel ABCD」為未辨識的描述行

#### Scenario: Leading Chinese words
- **WHEN** 某列資產描述為「備份軟體 Veeam Backup & Replication」，目錄有別名「Veeam Backup & Replication」
- **THEN** 該資產有一個版本待補的 Veeam Backup & Replication 元件

### Requirement: Duplicate products are merged
同一列有兩行以上比對到同一個目錄產品時，系統 SHALL 只建立一個該產品的元件，報表 SHALL 警告該列合併了哪些描述行，並提醒版本不同時須在畫面另外新增資產。

#### Scenario: Same switch used twice
- **WHEN** 某列描述為「Core: Aruba XYZ-100 *2、Edge: Aruba XYZ-100 *9」，兩行都比對到 `hpe-aoscx`
- **THEN** 該資產只有一個 `hpe-aoscx` 元件，報表警告「Core: Aruba XYZ-100 *2」與「Edge: Aruba XYZ-100 *9」已合併

### Requirement: Existing assets are reused
ISMS 編號已屬於 active 資產時（不分大小寫；沒有時，asset_tag 與 ISMS 編號相同且沒有其他 ISMS 編號的資產也視為同一資產），匯入 SHALL 更新該資產：ISMS 欄位（類別、資產說明、C/I/A 與管理欄位）取自 ISMS 清冊；asset_tag、name、曝露度、location、notes、既有元件（含版本）與未辨識產品 MUST NOT 改變。描述中比對到、該資產還沒有的產品 SHALL 加入為版本待補元件；還沒有的未辨識描述行 SHALL 加入為版本待補的未辨識產品。ISMS 編號屬於 retired 資產時，SHALL 略過該列並說明須先恢復。

#### Scenario: ISMS ID already in cvewatch
- **WHEN** 資料庫的 `FW-01` 有 ISMS 編號 `NET-HW-002`、PAN-OS `11.1.6` 元件、曝露度 internet，ISMS 清冊的 `NET-HW-002` 列 C/I/A 為 3/3/4
- **THEN** 匯入後 `FW-01` 的 C/I/A 為 3/3/4，曝露度仍為 internet，PAN-OS 版本仍為 `11.1.6`，沒有新增資產

#### Scenario: New product on an existing asset
- **WHEN** `FW-01` 只有 PAN-OS 元件，ISMS 描述另有一行比對到 Dell iDRAC
- **THEN** 匯入後 `FW-01` 多一個版本待補的 iDRAC 元件，PAN-OS 元件不變

#### Scenario: ISMS ID of a retired asset
- **WHEN** ISMS 編號 `NET-HW-005` 屬於 retired 的 `NAS-02`
- **THEN** 這一列不匯入，報表說明 `NET-HW-005` 屬於已退役資產 `NAS-02`，須先恢復

## REMOVED Requirements

### Requirement: ISMS draft command
**Reason**: 元件可以是版本待補之後，ISMS 清冊可以直接匯入，不再需要先產生草稿再補版本。
**Migration**: 改用 `cvewatch asset import-isms <ISMS清冊.xlsx>`（可加 `--dry-run` 預覽）；匯入後在畫面補上版本。

### Requirement: ISMS draft report
**Reason**: 草稿與 `isms_report` 工作表一併移除。
**Migration**: 匯入預覽與摘要改依 ISMS import report 列出相同的資訊。
