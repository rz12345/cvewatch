## ADDED Requirements

### Requirement: GUI asset system management
資產畫面的工具列 SHALL 提供「資產說明…」，開啟對話框列出所有資產說明（代碼、名稱、歸屬的資產數，含 retired），並 SHALL 可新增資產說明、修改名稱與刪除。代碼欄 SHALL 只在新增時可輸入。驗證錯誤 SHALL 顯示在對應欄位旁且不關閉對話框；仍有資產歸屬的資產說明，其「刪除」SHALL 停用並以提示說明歸屬的資產數。規則 SHALL 與 asset-inventory 的 Asset system 相同。

#### Scenario: Adding a system from the window
- **WHEN** 使用者開啟「資產說明…」，新增代碼 `net`、名稱「網路服務」後儲存
- **THEN** 清單出現「NET 網路服務」，資產數為 0

#### Scenario: Duplicate name in the dialog
- **WHEN** 已有「網路服務」，使用者新增另一個名稱也是「網路服務」的資產說明
- **THEN** 對話框不關閉，名稱欄位旁說明該名稱已被 `NET` 使用，資料庫不變

#### Scenario: System still in use
- **WHEN** 資產說明 `NET` 有 3 個資產歸屬
- **THEN** `NET` 的「刪除」停用，滑鼠提示說明仍有 3 個資產歸屬

### Requirement: GUI asset list grouped by system
資產清單 SHALL 依資產說明分組：每組以「代碼 名稱」與該組資產數為標題，組內維持編號、名稱、未處理三欄；組依代碼排序，未歸屬任何資產說明的資產 SHALL 列在最後一組「未歸屬資產說明」。各組 SHALL 可收合，收合的組標題 SHALL 顯示組內未處理命中的總數。沒有可顯示資產的組 MUST NOT 出現在清單中。選取、Enter 編輯與 Delete 退役 SHALL 與分組前相同。

#### Scenario: Assets under their systems
- **WHEN** `FW-01`、`SW-CORE` 歸屬 `NET`，`NAS-01` 沒有資產說明
- **THEN** 清單先列「NET 網路服務（2）」與其下的 `FW-01`、`SW-CORE`，最後列「未歸屬資產說明（1）」與 `NAS-01`

#### Scenario: Collapsed group shows open findings
- **WHEN** `NET` 組內共有 3 筆未處理命中，使用者收合該組
- **THEN** 組標題醒目顯示未處理 3，組內資產不顯示

#### Scenario: Group with only retired assets
- **WHEN** 資產說明 `SYS` 只有 retired 資產，「顯示已退役」未勾選
- **THEN** 清單中不出現 `SYS` 組；勾選後 `SYS` 組與其退役資產才出現

## MODIFIED Requirements

### Requirement: GUI asset class and rating input
資產編輯對話框的類別下拉選單 SHALL 列出所有已定義子類別，以「代碼 名稱」呈現並依代碼排序。機密性、完整性、可用性 SHALL 各為一個下拉選單，含「依類別」與 1–4；選「依類別」時 SHALL 顯示目前類別的範本值，儲存時寫入該值。對話框 SHALL 即時顯示資產價值與防護等級。ISMS 編號 SHALL 為選填文字欄位，格式、大類、系統代碼或重複錯誤 SHALL 顯示在該欄位旁。資產說明 SHALL 為下拉選單（「未歸屬」與所有資產說明，以「代碼 名稱」呈現）。權責單位、保管人、使用單位 SHALL 為選填文字欄位；法規遵循性 SHALL 為含「未評定」與 1–4 的下拉選單；是否含個人資料 SHALL 為含「依類別」「是」「否」的下拉選單，選「依類別」時 SHALL 顯示目前類別的預設值。

#### Scenario: Template values follow the chosen class
- **WHEN** 新增資產時類別選「HW05 儲存設備」，C/I/A 都維持「依類別」
- **THEN** 三個選單分別顯示「依類別（2）」「依類別（3）」「依類別（4）」，資產價值顯示 4；儲存後 C/I/A 為 2/3/4

#### Scenario: Application system needs ratings
- **WHEN** 類別選「SW01 應用系統」，C/I/A 維持「依類別」就按「儲存」
- **THEN** 對話框不關閉，三個欄位旁顯示「必填」，資料庫不變

#### Scenario: ISMS ID with the wrong major class
- **WHEN** 類別為「HW03 資安設備」，ISMS 編號填 `SYS-SW-001` 後按「儲存」
- **THEN** 對話框不關閉，ISMS 編號欄位旁顯示大類不符的錯誤，資料庫不變

#### Scenario: ISMS ID from another system
- **WHEN** 資產說明選「NET 網路服務」，ISMS 編號填 `SYS-HW-001` 後按「儲存」
- **THEN** 對話框不關閉，ISMS 編號欄位旁顯示系統代碼與資產說明不符的錯誤，資料庫不變

#### Scenario: Protection level follows compliance
- **WHEN** C/I/A 為 2/3/3，使用者把法規遵循性由「未評定」改為 4
- **THEN** 對話框的資產價值維持 3，防護等級由「未評定」變為 4

#### Scenario: Personal data follows the class
- **WHEN** 新增資產時類別選「HW05 儲存設備」，是否含個人資料維持「依類別」
- **THEN** 選單顯示「依類別（否）」，儲存後資產標示為不含個人資料

### Requirement: GUI asset presentation
資產畫面與資產對話框 SHALL 以使用者用語呈現資料：目錄產品 SHALL 以產品目錄的顯示名稱呈現（產品 key MUST NOT 作為主要文字，只可出現在提示中）；類別 SHALL 以「代碼 名稱」呈現，曝露度 SHALL 以「英文值（中文名稱）」呈現；資產詳情 SHALL 顯示 C/I/A、資產價值、ISMS 編號、資產說明（「代碼 名稱」）、權責單位、保管人、使用單位、法規遵循性、防護等級與是否含個人資料，未填的欄位 SHALL 顯示「-」，未評定的法規遵循性與防護等級 SHALL 顯示「未評定」；套件元件的版本 SHALL 只顯示一次。資產清單 SHALL 以編號、名稱、未處理命中數三個欄位呈現，未處理數大於 0 時 SHALL 醒目顯示，名稱與編號相同時 MUST NOT 重複顯示。資產沒有命中時 SHALL 顯示「沒有命中」而非空白。工具列 SHALL 將「新增資產」「資產說明…」、清冊檔案操作（匯入清冊、下載範本、匯出清冊）與「顯示已退役」分組；「退役…」與「永久刪除」SHALL 以警示樣式顯示並與一般操作分開。

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

#### Scenario: Administrative fields in the detail pane
- **WHEN** 使用者選取歸屬「SYS 核心系統」、保管人「王小明」、權責單位與使用單位未填、法規遵循性未評定、不含個人資料的 `FW-01`
- **THEN** 詳情顯示資產說明「SYS 核心系統」、保管人「王小明」、權責單位「-」、使用單位「-」、法規遵循性與防護等級「未評定」、是否含個人資料「否」
