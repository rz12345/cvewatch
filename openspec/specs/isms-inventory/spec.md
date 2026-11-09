# isms-inventory Specification

## Purpose

讀取機構 ISMS「資訊資產清冊」Excel，轉換成 cvewatch 清冊格式，讓使用者不必把 ISMS 已登錄的資產說明、類別、C/I/A 與管理欄位重新輸入一次。

## Requirements

### Requirement: ISMS draft command
系統 SHALL 提供 `cvewatch asset isms-draft <ISMS清冊.xlsx> <草稿.xlsx>`，讀取 ISMS 資訊資產清冊，寫出 cvewatch 清冊格式的草稿：含 `assets`、`components`、`systems` 工作表與一張 `isms_report` 工作表。草稿 SHALL 可用 `cvewatch asset import` 匯入，匯入時 `isms_report` 工作表 SHALL 被忽略。產生草稿 MUST NOT 修改資料庫；目的檔已存在時 SHALL 拒絕且不覆蓋。

#### Scenario: Generating a draft
- **WHEN** 使用者執行 `cvewatch asset isms-draft D:\isms.xlsx D:\draft.xlsx`
- **THEN** `D:\draft.xlsx` 包含 `assets`、`components`、`systems`、`isms_report` 工作表，指令印出轉換摘要，資料庫不變

#### Scenario: Draft target exists
- **WHEN** `D:\draft.xlsx` 已存在
- **THEN** 指令以錯誤結束並說明目的檔已存在，原檔內容不變

#### Scenario: Draft without a database
- **WHEN** 尚未執行 `cvewatch init`，資料目錄中沒有資料庫
- **THEN** 指令仍寫出草稿並以結束碼 0 結束，提示草稿不參考既有資產

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
- **THEN** 指令以錯誤結束，說明找不到 ISMS 資訊資產清冊的表頭，且不產生草稿

#### Scenario: Missing required column
- **WHEN** 表頭沒有「可用性」欄
- **THEN** 指令以錯誤結束並指出缺少「可用性」欄

### Requirement: ISMS ID composition
系統 SHALL 以第一欄資產編號（去除結尾的 `-`）、`-` 與第二欄資產編號組成 ISMS 編號，並依 asset-inventory 的 ISMS asset ID 規則驗證。

#### Scenario: Two ID columns
- **WHEN** 某列兩欄資產編號為 `NET-` 與 `HW-001`
- **THEN** 該列的 ISMS 編號為 `NET-HW-001`，系統代碼為 `NET`

### Requirement: Rows outside HW and SW
資產類別不是 HW 或 SW 大類（PE、IF、SE）的列 SHALL 略過（資產類別為已定義子類別的代碼或名稱時依子類別判斷，其他寫法依流水號的前兩碼），報表 SHALL 依大類列出略過的列數，不逐列警告。

#### Scenario: Personnel and information rows
- **WHEN** 清冊有 3 列 PE、5 列 IF、2 列 SE 與 6 列 HW／SW
- **THEN** 草稿只有 6 個資產，報表顯示「略過 PE 3、IF 5、SE 2」

### Requirement: Invalid rows are skipped
HW／SW 列有下列情形時，系統 SHALL 略過該列，在報表中列出列號與原因，並繼續處理其他列：ISMS 編號格式錯誤、ISMS 編號大類與資產類別不符、子類別未定義、ISMS 編號與前面的列重複（不分大小寫）、新資產的 ISMS 編號與另一個既有資產的 asset_tag 相同、兩列對應到同一個既有資產、C/I/A 有填但不是 1–4、C/I/A 空白而子類別沒有範本值、法規遵循性有填但不是 1–4、是否含個人資料有填但不是 Y 或 N。空白的 C/I/A 與是否含個人資料 SHALL 依 asset-inventory 的規則帶入子類別的值；法規遵循性為 `-`、`－` 等單一破折號或空白時 SHALL 視為未評定。

#### Scenario: Major class mismatch
- **WHEN** 第 44 列兩欄資產編號為 `WEB-` 與 `SE-003`，資產類別為 `SW01`
- **THEN** 草稿沒有這一列，報表列出「第 44 列：ISMS 編號「WEB-SE-003」的大類 SE 與資產類別 SW01 不符」，其他列照常轉換

#### Scenario: Asset tag used by another asset
- **WHEN** 資料庫的資產 `NET-HW-001` 的 ISMS 編號為 `ABC-HW-009`，ISMS 清冊有 `NET-HW-001` 列
- **THEN** 草稿沒有這一列，報表說明 ISMS 編號 `NET-HW-001` 與既有資產的 asset_tag 相同，須先更改該資產的編號

#### Scenario: Compliance dash
- **WHEN** 某列的法規遵循性為 `－`
- **THEN** 該列照常轉換，資產的法規遵循性為未評定

### Requirement: ISMS field mapping
每個轉換的列 SHALL 成為草稿中的一個資產：`asset_tag` 與 `isms_id` 為 ISMS 編號，`category` 為資產類別，`system` 為系統代碼；C/I/A、權責單位、保管人、使用單位、法規遵循性與是否含個人資料 SHALL 帶入對應欄位。`name` SHALL 為資產描述第一行去除數量與標籤後的文字（為空時用子類別名稱），`notes` SHALL 為「ISMS 描述：」加上資產描述原文。曝露度 SHALL 留空。資產價值與防護等級欄 SHALL 被忽略。

#### Scenario: Mapping a storage row
- **WHEN** 某列為 `NET-`、`HW-003`、`HW05`、資產說明「網路服務」、資產描述「Synology XYZ-200 *1」、保管人「王小明」、C/I/A 2/3/4、是否含個人資料 `N`
- **THEN** 草稿有資產 `NET-HW-003`：類別 `HW05`、資產說明 `NET`、名稱「Synology XYZ-200」、保管人「王小明」、C/I/A 2/3/4、`personal_data` 為 `N`、曝露度空白、notes 為「ISMS 描述：Synology XYZ-200 *1」

### Requirement: Systems from the ISMS workbook
草稿的 `systems` 工作表 SHALL 列出轉換的列中出現的每個系統代碼，名稱取自該代碼第一個非空的「資產說明」。同一代碼出現不同的資產說明時，SHALL 使用第一個並在報表警告。某代碼的資產說明都是空白時，SHALL 使用資料庫中該代碼的名稱；資料庫也沒有時，SHALL 以代碼作為名稱。

#### Scenario: Two names for one system code
- **WHEN** 第 5 列與第 9 列的系統代碼都是 `NET`，資產說明分別為「網路服務」與「網路服務組」
- **THEN** `systems` 工作表的 `NET` 名稱為「網路服務」，報表警告第 9 列的資產說明與第 5 列不同

### Requirement: Description to components
系統 SHALL 依換行、頓號、逗號與分號把資產描述拆成多行。每行 SHALL 去除數量（`*2`、` x2`、`×2`；` x64` 這類平台寫法保留）、冒號前的標籤（例如「Core:」；URL 與含數字的名稱中的冒號不是標籤）與括號內的註記，再依 product-catalog 的 Alias resolution 比對。比對不到時，SHALL 去除開頭的中文字詞後再比對一次。比對到的行 SHALL 成為該產品的元件；仍比對不到的行 SHALL 以去除數量與標籤後的原文寫入 `product`，匯入後成為未辨識產品。新元件的版本 SHALL 留空，`isms_report` SHALL 列出待補版本的元件數。

#### Scenario: Multi-line description
- **WHEN** `catalog.d` 為 `hpe-aoscx` 宣告了別名「Aruba XYZ-100」，某列資產描述為「Core: Aruba XYZ-100 *2、\nZyxel ABCD *1」
- **THEN** 該資產有一個 `hpe-aoscx` 元件與一筆產品原文為「Zyxel ABCD」的元件列，兩者版本都空白，報表列出「Zyxel ABCD」為未辨識的描述行

#### Scenario: Leading Chinese words
- **WHEN** 某列資產描述為「備份軟體 Veeam Backup & Replication」，目錄有別名「Veeam Backup & Replication」
- **THEN** 該資產有一個 Veeam Backup & Replication 元件

### Requirement: Duplicate products are merged
同一列有兩行以上比對到同一個目錄產品時，草稿 SHALL 只保留一個該產品的元件，報表 SHALL 警告該列合併了哪些描述行，並提醒版本不同時須把該資產拆成多個資產。

#### Scenario: Same switch used twice
- **WHEN** 某列描述為「Core: Aruba XYZ-100 *2、Edge: Aruba XYZ-100 *9」，兩行都比對到 `hpe-aoscx`
- **THEN** 草稿中該資產只有一個 `hpe-aoscx` 元件，報表警告「Core: Aruba XYZ-100 *2」與「Edge: Aruba XYZ-100 *9」已合併

### Requirement: Existing assets are reused
ISMS 編號已屬於資料庫中的 active 資產時（不分大小寫；沒有時，asset_tag 與 ISMS 編號相同且沒有其他 ISMS 編號的資產也視為同一資產），草稿 SHALL 沿用該資產的 `asset_tag`、name、曝露度、location 與 notes，並 SHALL 寫出它所有的清冊元件（含版本）與未辨識產品；描述中比對到、但該資產還沒有的產品 SHALL 以空白版本加入。ISMS 欄位（類別、資產說明、C/I/A 與管理欄位）SHALL 取自 ISMS 清冊。ISMS 編號屬於 retired 資產時，SHALL 略過該列並在報表說明須先恢復該資產。

#### Scenario: ISMS ID already in cvewatch
- **WHEN** 資料庫的 `FW-01` 有 ISMS 編號 `NET-HW-002`、PAN-OS `11.1.6` 元件、曝露度 internet，ISMS 清冊的 `NET-HW-002` 列 C/I/A 為 3/3/4
- **THEN** 草稿中該列的資產為 `FW-01`：曝露度 internet、PAN-OS 版本 `11.1.6`、C/I/A 3/3/4；匯入後 `FW-01` 被更新，不會新增資產

#### Scenario: ISMS ID of a retired asset
- **WHEN** ISMS 編號 `NET-HW-005` 屬於 retired 的 `NAS-02`
- **THEN** 草稿沒有這一列，報表說明 `NET-HW-005` 屬於已退役資產 `NAS-02`，須先恢復

### Requirement: ISMS draft report
草稿報表 SHALL 列出：轉換的資產數（新資產與沿用既有資產分開計數）、依大類略過的列數、略過的列與原因、合併的產品、未辨識的描述行、待補版本的元件數，以及資料庫中 active、但不在這次轉換結果中的資產。報表 SHALL 一律附帶提醒：匯入草稿時不要使用 `--prune`。CLI SHALL 印出摘要，`isms_report` 工作表 SHALL 保存完整明細。

#### Scenario: Assets missing from the ISMS workbook
- **WHEN** 資料庫有 active 資產 `PVE-01`，沒有 ISMS 編號
- **THEN** 報表列出 `PVE-01` 不在 ISMS 清冊中，並提醒匯入草稿時不要使用 `--prune`

#### Scenario: Importing a draft before filling versions
- **WHEN** 使用者未補版本就以 `cvewatch asset import D:\draft.xlsx` 匯入草稿
- **THEN** 匯入依既有規則中止，列出缺少 version 的列號，資料庫不變

#### Scenario: Importing a completed draft
- **WHEN** 使用者在草稿補上所有版本後匯入
- **THEN** 匯入成功，新資產帶有 ISMS 編號、資產說明、C/I/A 與管理欄位，`isms_report` 工作表不影響匯入
