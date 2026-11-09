# Spec Delta

## ADDED Requirements

### Requirement: ISMS export command
系統 SHALL 提供 `cvewatch asset export-isms <path.xlsx>`，把有 ISMS 編號的 active 資產匯出為新的 ISMS 資訊資產清冊檔；匯出 MUST NOT 讀取或修改任何既有的 ISMS 範本，MUST NOT 寫入資料庫。retired 資產 MUST NOT 匯出。沒有 ISMS 編號的 active 資產 SHALL 略過，完成後 SHALL 顯示匯出的資產數並列出每個略過的資產編號。目的檔已存在時 SHALL 拒絕且不覆蓋。完成後 SHALL 提示此檔未加密、含保管人等資產資料。資料庫尚未初始化時 SHALL 以錯誤結束並提示先執行 `cvewatch init`。

#### Scenario: Exporting ISMS assets
- **WHEN** 有 3 個 active 資產都有 ISMS 編號，使用者執行 `cvewatch asset export-isms D:\isms-out.xlsx`
- **THEN** 產生 `D:\isms-out.xlsx`，輸出「已匯出 3 個資產」與此檔未加密的提示，資料庫不變

#### Scenario: Asset without an ISMS ID
- **WHEN** active 資產 `SRV-01` 沒有 ISMS 編號，執行匯出
- **THEN** 檔案中沒有 `SRV-01`，完成摘要列出 `SRV-01` 因沒有 ISMS 編號而略過

#### Scenario: Retired asset is not exported
- **WHEN** ISMS 編號 `NET-HW-005` 屬於 retired 資產，執行匯出
- **THEN** 檔案中沒有 `NET-HW-005`，也不列在略過清單中

#### Scenario: ISMS export target exists
- **WHEN** `D:\isms-out.xlsx` 已存在
- **THEN** 指令以錯誤結束並說明目的檔已存在，原檔內容不變

### Requirement: ISMS export layout
匯出檔 SHALL 只有一個工作表「資訊資產清冊」：第 1–3 列為文件資訊（標題、匯出日期、說明；MUST NOT 含文件編號），第 4 列為 ISMS v2.1 的 16 欄表頭，依序為資產編號、資產編號、資產類別、資產說明、資產名稱、資產描述、權責單位、保管人、使用單位、機密性、完整性、可用性、資產價值、法規遵循性、資通系統防護等級、是否含個人資料。第 5 列起 SHALL 一列一個資產，依 ISMS 編號排序（不分大小寫）。匯出檔 MUST NOT 使用合併儲存格。

#### Scenario: Header row of the exported file
- **WHEN** 使用者開啟匯出檔
- **THEN** 工作表「資訊資產清冊」第 4 列是 16 欄表頭，前兩欄都是「資產編號」，第 5 列是第一個資產

#### Scenario: Exported file is read as an ISMS workbook
- **WHEN** 以 ISMS 匯入讀取匯出檔
- **THEN** 系統以第 4 列為表頭、從第 5 列開始讀取資產，文件資訊列不列為資產

#### Scenario: Rows sorted by ISMS ID
- **WHEN** 匯出的資產 ISMS 編號為 `SYS-HW-002`、`net-hw-001`
- **THEN** 第 5 列是 `net-hw-001`（保留原寫法），第 6 列是 `SYS-HW-002`

### Requirement: ISMS export field mapping
每列 SHALL 依下列對應：第一欄資產編號為 ISMS 編號的系統代碼加 `-`，第二欄為其餘部分；資產類別為子類別代碼；資產說明為資產說明名稱；資產名稱為子類別名稱；權責單位、保管人、使用單位為原值，未填為空白；機密性、完整性、可用性與資產價值（三者最大值）為數字；法規遵循性與資通系統防護等級依 asset-inventory 的 Protection level，未評定時為 `-`；是否含個人資料為 `Y` 或 `N`。子類別已不在設定中時，資產名稱 SHALL 空白，完成摘要 SHALL 警告該資產讀回時會被略過。

#### Scenario: Mapping an exported row
- **WHEN** `FW-EDGE` 的 ISMS 編號為 `NET-HW-001`、類別 `HW02`、資產說明「NET 網路服務」、保管人「王小明」、C/I/A 2/3/4、法規遵循性未評定、不含個人資料
- **THEN** 該列依序為 `NET-`、`HW-001`、`HW02`、「網路服務」、「通訊設備」、資產描述、空白、「王小明」、空白、2、3、4、4、`-`、`-`、`N`

#### Scenario: Compliance raises the exported protection level
- **WHEN** 資產 C/I/A 為 2/3/3、法規遵循性為 4
- **THEN** 該列的資產價值為 3、法規遵循性為 4、資通系統防護等級為 4

### Requirement: ISMS export description
資產描述 SHALL 以換行分隔多行。資產有廠牌或型號時，第一行 SHALL 為「廠牌型號：【廠牌 型號】」。其後 SHALL 依建立順序，每個清冊元件與未辨識產品各一行：目錄產品寫產品目錄的顯示名稱（無法解析回同一產品時寫產品 key），未辨識產品寫原文；有版本時接「（版本）」，版本待補時只寫產品。描述 MUST NOT 包含主機名稱、IP 等其他欄位，也 MUST NOT 包含主機套件元件。廠牌、型號與版本中會被 ISMS 匯入視為分隔、標籤或括號的字元 SHALL 以空白取代。

#### Scenario: Description with model and components
- **WHEN** `FW-EDGE` 的廠牌為 `Aruba`、型號為 `XYZ-100`，元件為 AOS-CX `1.2.3`
- **THEN** 資產描述為「廠牌型號：【Aruba XYZ-100】」與「AOS-CX（1.2.3）」兩行

#### Scenario: Pending and unresolved items
- **WHEN** `SRV-01` 沒有廠牌與型號，元件為版本待補的 DiskStation Manager，另有未辨識產品 `Zyxel ABCD` 版本 `2.0`
- **THEN** 資產描述為「DiskStation Manager」與「Zyxel ABCD（2.0）」兩行

#### Scenario: Host name and IP are not exported
- **WHEN** `SRV-01` 的主機名稱為 `srv01`、IP 為 `192.0.2.10`
- **THEN** 匯出檔的任何儲存格都不含 `srv01` 與 `192.0.2.10`

#### Scenario: Separator in a version
- **WHEN** AOS-CX 元件的版本為 `1.0, build 5`
- **THEN** 該行為「AOS-CX（1.0 build 5）」，以 ISMS 匯入讀取時仍為一行並辨識為 AOS-CX

### Requirement: ISMS export round trip
以 ISMS 匯入讀取匯出檔時，每個匯出資產推測出的目錄產品 SHALL 與原資產的清冊元件相同，未辨識產品 SHALL 與原資產的未辨識產品原文相同（正規化後比較）；廠牌型號行 MUST NOT 成為元件或未辨識產品，MUST NOT 產生合併警告。版本 MUST NOT 被讀回。匯出時 SHALL 以 ISMS 匯入相同的描述規則檢查每個資產；讀回結果不同、或描述沒有任何元件行時，完成摘要 SHALL 警告該資產與原因，匯出不中止。

#### Scenario: Re-importing into the same database
- **WHEN** 使用者匯出後，立刻以 `cvewatch asset import-isms <匯出檔> --dry-run` 讀回
- **THEN** 預覽為「新增 0、更新 0」，這次新加入的版本待補元件為 0，不在 ISMS 清冊的資產只有匯出時因沒有 ISMS 編號而略過的資產

#### Scenario: Importing into an empty database
- **WHEN** 把匯出檔以 ISMS 匯入讀進沒有任何資產的資料庫
- **THEN** `NET-HW-001` 的元件產品與原資產相同、都是版本待補，沒有來自「廠牌型號」行的未辨識產品

#### Scenario: Line that cannot be read back
- **WHEN** 資產的未辨識產品原文為 `Zyxel ABCD x2`（ISMS 匯入會去除結尾數量）
- **THEN** 匯出照常完成，完成摘要警告該資產的「Zyxel ABCD x2」讀回時會成為「Zyxel ABCD」

#### Scenario: Asset with only a model
- **WHEN** 資產有廠牌與型號，但沒有清冊元件與未辨識產品
- **THEN** 該資產照常匯出，完成摘要警告其資產描述沒有元件行，以 ISMS 匯入建立新資產時會被略過
