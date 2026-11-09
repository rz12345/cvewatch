## MODIFIED Requirements

### Requirement: GUI asset class and rating input
資產編輯對話框的類別下拉選單 SHALL 列出所有已定義子類別，以「代碼 名稱」呈現並依代碼排序。機密性、完整性、可用性 SHALL 各為一個下拉選單，含「依類別」與 1–4；選「依類別」時 SHALL 顯示目前類別的範本值，儲存時寫入該值。對話框 SHALL 即時顯示資產價值。ISMS 編號 SHALL 為選填文字欄位，格式、大類或重複錯誤 SHALL 顯示在該欄位旁。

#### Scenario: Template values follow the chosen class
- **WHEN** 新增資產時類別選「HW05 儲存設備」，C/I/A 都維持「依類別」
- **THEN** 三個選單分別顯示「依類別（2）」「依類別（3）」「依類別（4）」，資產價值顯示 4；儲存後 C/I/A 為 2/3/4

#### Scenario: Application system needs ratings
- **WHEN** 類別選「SW01 應用系統」，C/I/A 維持「依類別」就按「儲存」
- **THEN** 對話框不關閉，三個欄位旁顯示「必填」，資料庫不變

#### Scenario: ISMS ID with the wrong major class
- **WHEN** 類別為「HW03 資安設備」，ISMS 編號填 `SYS-SW-001` 後按「儲存」
- **THEN** 對話框不關閉，ISMS 編號欄位旁顯示大類不符的錯誤，資料庫不變

### Requirement: GUI asset presentation
資產畫面與資產對話框 SHALL 以使用者用語呈現資料：目錄產品 SHALL 以產品目錄的顯示名稱呈現（產品 key MUST NOT 作為主要文字，只可出現在提示中）；類別 SHALL 以「代碼 名稱」呈現，曝露度 SHALL 以「英文值（中文名稱）」呈現；資產詳情 SHALL 顯示 C/I/A、資產價值與 ISMS 編號；套件元件的版本 SHALL 只顯示一次。資產清單 SHALL 以編號、名稱、未處理命中數三個欄位呈現，未處理數大於 0 時 SHALL 醒目顯示，名稱與編號相同時 MUST NOT 重複顯示。資產沒有命中時 SHALL 顯示「沒有命中」而非空白。工具列 SHALL 將「新增資產」、清冊檔案操作（匯入清冊、下載範本、匯出清冊）與「顯示已退役」分組；「退役…」與「永久刪除」SHALL 以警示樣式顯示並與一般操作分開。

#### Scenario: Product shown by display name
- **WHEN** 使用者選取元件為 `proxmox-ve 8.2.2` 的 `PVE-01`
- **THEN** 詳情與編輯對話框顯示「Proxmox VE 8.2.2」，畫面主要文字不出現 `proxmox-ve`

#### Scenario: Long asset name in the list
- **WHEN** 資產 `FW-CORE` 名稱為「資訊所機房核心防火牆主機」、有 1 筆未處理命中
- **THEN** 清單以編號、名稱、未處理數三欄顯示該列，未處理數 `1` 位於自己的欄位且醒目顯示

#### Scenario: Asset without findings
- **WHEN** 使用者選取沒有任何命中的 `PVE-01`
- **THEN** 詳情的命中區塊顯示「沒有命中」

#### Scenario: Bilingual category value
- **WHEN** 資產類別為 `HW01`
- **THEN** 詳情與類別下拉選單顯示「HW01 伺服器」，儲存後資料庫與匯出的值仍為 `HW01`

#### Scenario: Ratings in the detail pane
- **WHEN** 使用者選取 C/I/A 為 2/3/4、ISMS 編號為 `SYS-HW-003` 的 `FW-01`
- **THEN** 詳情顯示機密性 2、完整性 3、可用性 4、資產價值 4 與 `SYS-HW-003`
