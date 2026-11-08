# gui-console Specification

## Purpose

提供 Windows 視窗介面 `cvewatch-gui.exe`，以滑鼠與對話框完成與 TUI 同等的瀏覽、篩選、命中處理、等待區、設定、VEX 匯出與清冊匯入，並與 TUI、CLI 共用同一個資料庫與設定檔。

## Requirements

### Requirement: Windowed application launch
系統 SHALL 提供與 `cvewatch.exe` 放在同一資料夾的視窗版執行檔 `cvewatch-gui.exe`；啟動時 MUST NOT 顯示主控台視窗，SHALL 以與 CLI 相同的順序（`--config` → `CVEWATCH_CONFIG` → 使用者設定目錄）決定設定檔，並開啟與 CLI、TUI 相同的加密資料庫。

#### Scenario: Launching from a shortcut
- **WHEN** 使用者以桌面捷徑開啟 `cvewatch-gui.exe`
- **THEN** 只出現 cvewatch 視窗並顯示 Dashboard，畫面上沒有任何主控台視窗

#### Scenario: Explicit configuration file
- **WHEN** 使用者執行 `cvewatch-gui.exe --config D:\cvewatch\cvewatch.toml`
- **THEN** 視窗使用該設定檔，之後由視窗啟動的 sync、清冊匯入與排程重新安裝也使用該設定檔

### Requirement: Startup failures are explained
啟動時若設定檔無效、資料庫尚未初始化、資料庫金鑰遺失或錯誤，視窗版 SHALL 顯示說明原因與對應 CLI 指令的錯誤視窗，使用者關閉後才結束；MUST NOT 無訊息結束。設定檔有未知欄位時 SHALL 以警告顯示並繼續啟動。

#### Scenario: Database not initialized
- **WHEN** 在從未執行 `cvewatch init` 的電腦上開啟 `cvewatch-gui.exe`
- **THEN** 出現錯誤視窗，說明資料庫不存在並提示執行 `cvewatch init`

#### Scenario: Recovery key needed
- **WHEN** Credential Manager 中找不到資料庫金鑰
- **THEN** 錯誤視窗說明金鑰遺失並提示 `cvewatch db import-key`

#### Scenario: Unknown configuration key
- **WHEN** 設定檔包含未知欄位 `notify.emial`
- **THEN** 視窗照常開啟，並顯示「設定檔包含未知欄位 `notify.emial`（已忽略）」的警告

### Requirement: Companion command-line executable
視窗版 SHALL 以隱藏視窗的子程序執行同資料夾的 `cvewatch.exe` 進行背景 sync、清冊匯入與排程重新安裝；同資料夾找不到 `cvewatch.exe` 時，這些操作 SHALL 停用並說明原因，其他瀏覽與處理功能 SHALL 照常使用。

#### Scenario: Command-line executable missing
- **WHEN** 只把 `cvewatch-gui.exe` 複製到另一個資料夾後開啟
- **THEN** 同步、匯入清冊與重新安裝排程的按鈕停用，滑鼠停留時說明需要同資料夾的 `cvewatch.exe`

### Requirement: GUI navigation and dashboard
視窗版 SHALL 提供 Dashboard、命中、資產、等待區、同步紀錄與設定六個畫面，可用側邊導覽或 `Ctrl+1`～`Ctrl+6` 切換。Dashboard SHALL 顯示與 TUI 相同的內容：各等級未處理命中數、KEV 命中數、等待區數量與逾期數、來源矛盾數，以及每個來源最後一次同步的時間與結果；點選等級或 KEV 數字 SHALL 開啟套用對應篩選的命中畫面。

#### Scenario: Jumping from a dashboard count
- **WHEN** 使用者在 Dashboard 點選 P1 未處理命中數
- **THEN** 切換到命中畫面，篩選為「未處理、P1」

#### Scenario: A source failed last time
- **WHEN** 最近一次 OSV 同步失敗
- **THEN** Dashboard 的 OSV 來源標示為失敗並顯示時間

### Requirement: GUI findings table
命中畫面 SHALL 以表格列出命中，預設只顯示未處理命中並依風險排序；SHALL 支援依狀態、等級、資產、信心等級與 KEV 篩選、即時文字搜尋（CVE ID、資產名稱、描述），以及點選欄位標題排序。表格 SHALL 以標記區分 probable 命中與「已處理但仍在受影響範圍」的命中。

#### Scenario: Default view
- **WHEN** 使用者切換到命中畫面
- **THEN** 表格只顯示未處理命中，依 P1 到 P4 及同級排序規則排列

#### Scenario: Searching as you type
- **WHEN** 使用者在搜尋框輸入 `2026-0`
- **THEN** 表格即時只顯示 CVE ID、資產名稱或描述含該字串的命中

### Requirement: GUI finding detail
在命中上按兩下或按 Enter，SHALL 開啟與 TUI 詳情內容相同的詳情面板：CVE 描述、各來源 CVSS、EPSS、KEV、各來源判定與版本區間、來源矛盾、原廠修正版本、處理時間軸與參考連結。

#### Scenario: Viewing why a finding exists
- **WHEN** 使用者在 PAN-OS 命中上按兩下
- **THEN** 詳情面板顯示判定來源（例如原廠公告）、受影響區間與元件目前版本

### Requirement: GUI triage
使用者 SHALL 能以 Ctrl／Shift 點選多筆命中，並以按鈕、右鍵選單或與 TUI 相同的按鍵（`r`、`i`、`u`）設為已處理、忽略或未處理；套用前 SHALL 要求輸入備註，忽略時 SHALL 先選擇原因，原因為「不受影響」時再選擇理由。結果 SHALL 與 CLI 及 TUI 的狀態變更紀錄相同，且只套用到目前顯示中的所選命中。

#### Scenario: Batch ignore
- **WHEN** 使用者選取三筆命中後按「忽略」，選擇原因「不受影響」與理由「需特定設定」，輸入備註並確認
- **THEN** 三筆命中變為 ignored 並帶相同原因與理由，每筆的時間軸各新增一筆含該備註的事件

#### Scenario: Cancelling the note dialog
- **WHEN** 使用者在備註對話框按「取消」或 Esc
- **THEN** 狀態不變更

#### Scenario: Selected findings hidden by search
- **WHEN** 使用者選取三筆命中後輸入搜尋字串，使其中兩筆不再顯示，再按「已處理」並確認
- **THEN** 只有仍顯示的一筆變更狀態，被隱藏的兩筆維持原狀且仍保持選取

### Requirement: GUI assets and pending
資產畫面 SHALL 列出資產、元件、各資產的命中與未辨識產品；等待區畫面 SHALL 顯示原因、進入天數、逾期與 KEV 置頂，並 SHALL 可將一筆 CVE 套用到資產（資產有多個相關元件時選擇元件）或標為不相關，套用前要求輸入備註。

#### Scenario: Applying a pending CVE to an asset
- **WHEN** 使用者在等待區選擇一筆 CVE，按「套用到資產」並選擇 `PVE-01`
- **THEN** 該 CVE 移出等待區，並在命中畫面出現一筆 PVE-01 的 probable 命中

### Requirement: GUI references and clipboard
詳情與等待區 SHALL 以連結按鈕開啟 CVE.org、NVD 與原廠公告（以系統預設瀏覽器開啟），並 SHALL 提供複製 CVE ID；只允許 https 網址。

#### Scenario: Open vendor advisory
- **WHEN** 使用者在詳情點選原廠公告連結
- **THEN** 預設瀏覽器開啟該公告網址

### Requirement: GUI new data and background sync
視窗開啟期間若有 sync 或比對寫入新資料，視窗 SHALL 顯示「有新資料」提示與重新載入按鈕，且 MUST NOT 自動重排目前表格；使用者 SHALL 能以按鈕啟動背景 sync，sync 期間視窗保持可操作並在結束後顯示結果。

#### Scenario: Scheduled sync finishes while browsing
- **WHEN** 使用者正在瀏覽命中表格，排程 sync 完成
- **THEN** 視窗顯示有新資料的提示，表格維持不變直到使用者按「重新載入」

#### Scenario: Sync already running
- **WHEN** 使用者按「同步」時排程 sync 正在執行
- **THEN** 視窗顯示「已有 sync 正在執行」，不啟動第二個 sync

### Requirement: Traditional Chinese rendering
視窗版 SHALL 正確顯示繁體中文（資產名稱、描述、備註、介面文字），並 SHALL 支援以 Windows 中文輸入法在搜尋框、備註與設定欄位輸入中文。找不到可用的中文字型時 SHALL 顯示警告。

#### Scenario: Chinese asset name and note
- **WHEN** 資產名稱為「資訊所機房核心防火牆主機」，使用者以注音輸入法在備註輸入「已更新韌體」
- **THEN** 表格與詳情完整顯示中文名稱，備註內容正確寫入時間軸

### Requirement: GUI settings
設定畫面 SHALL 提供與 TUI 相同的 Email、Redmine、排程與門檻三個分頁與相同的欄位，並遵守相同規則：一般欄位寫回 `cvewatch.toml` 且保留註解；密碼與 API key 以遮罩輸入、只存入作業系統憑證庫、畫面只顯示「已設定」或「未設定」；Redmine 專案、tracker、優先權與成員可從 Redmine 載入後挑選，無法連線時可手動輸入數字 ID；即時驗證欄位，任何欄位無效時拒絕儲存；測試寄信與測試 Redmine 使用表單中尚未儲存的值且 MUST NOT 建立議題；MUST NOT 提供無加密 SMTP、http Redmine 或明文傳輸例外開關。

#### Scenario: Setting the Redmine API key
- **WHEN** 使用者在 API key 欄位輸入金鑰並按「儲存」
- **THEN** 金鑰存入憑證庫，欄位顯示「已設定」，`cvewatch.toml` 中找不到該金鑰

#### Scenario: Invalid port
- **WHEN** 使用者將連接埠輸入為 `70000`
- **THEN** 連接埠欄位立即標示錯誤，「儲存」按鈕停用，設定檔不變

#### Scenario: Testing unsaved SMTP settings
- **WHEN** 使用者修改連接埠為 465、加密改為 TLS，未儲存即按「測試寄信」
- **THEN** 系統以 465/TLS 寄出測試信並顯示結果，設定檔不變

### Requirement: GUI insecure transport warning
設定檔已啟用 `allow_insecure_transport = true` 時，視窗 SHALL 在所有畫面頂端常駐顯示「明文傳輸已啟用」紅色警示列。

#### Scenario: Exception enabled in the file
- **WHEN** 使用者在設定檔手動設定 `allow_insecure_transport = true` 後開啟視窗
- **THEN** 每個畫面頂端都顯示紅色警示列

### Requirement: GUI unsaved changes protection
設定有未儲存變更或備註對話框有未送出的內容時，離開設定畫面或關閉視窗（含按視窗標題列的關閉鈕與 Alt+F4），系統 SHALL 詢問儲存、放棄或取消；選擇取消時視窗 MUST NOT 關閉。

#### Scenario: Closing the window with unsaved settings
- **WHEN** 使用者修改收件者後按視窗右上角的關閉鈕
- **THEN** 出現儲存／放棄／取消的確認對話框，選擇取消後視窗維持開啟且修改仍在

### Requirement: GUI schedule change prompt
儲存後若同步間隔或摘要時間有變更，系統 SHALL 提示重新安裝排程，並 SHALL 提供「重新安裝排程」按鈕直接執行。

#### Scenario: Digest time changed
- **WHEN** 使用者將摘要時間由 08:30 改為 07:45 並儲存，再按「重新安裝排程」
- **THEN** `cvewatch\digest` 的觸發時間更新為 07:45，視窗顯示結果

### Requirement: GUI VEX export
命中畫面 SHALL 提供「匯出 VEX」，以目前篩選條件匯出 CycloneDX VEX；SHALL 以 Windows 另存新檔對話框選擇路徑（預設文件資料夾），MUST NOT 覆蓋既有檔案，完成後 SHALL 顯示路徑與「此檔未加密」提示。

#### Scenario: Exporting filtered findings
- **WHEN** 使用者篩選資產 `FW-01` 後按「匯出 VEX」並在對話框確認預設檔名
- **THEN** 文件資料夾中產生只含 FW-01 命中的 VEX 檔，視窗顯示路徑與「此檔未加密」提示

#### Scenario: Target file already exists
- **WHEN** 使用者在對話框選擇已存在的檔案並確認取代
- **THEN** 系統不覆蓋該檔，顯示檔案已存在的錯誤，原檔內容不變

### Requirement: GUI inventory import
資產畫面 SHALL 提供「匯入清冊」，可用開啟檔案對話框選擇 .xlsx 或 .csv，或把檔案拖進視窗；匯入 SHALL 先顯示預覽（新增、更新、版本變更、未變、未辨識產品、清冊中消失的數量與明細），使用者確認後才寫入，並 SHALL 可勾選「將清冊中消失的資產標為退役」。CSV 清冊 SHALL 可另選元件檔。匯入規則、自訂欄名對應與匯入後的自動比對 SHALL 與 `cvewatch asset import` 相同；完成後視窗 SHALL 重新載入資料。

#### Scenario: Dragging a spreadsheet into the window
- **WHEN** 使用者把 `assets.xlsx` 拖進視窗
- **THEN** 視窗顯示預覽（例如「新增 2、更新 1（版本變更 1）」與明細），資料庫尚未變更；按「匯入」後才寫入，並自動比對與重新載入

#### Scenario: Cancelling after preview
- **WHEN** 使用者在預覽後按「取消」
- **THEN** 資料庫不變

#### Scenario: Unsupported file dropped
- **WHEN** 使用者把 `report.pdf` 拖進視窗
- **THEN** 視窗說明只支援 .xlsx 與 .csv 清冊，不執行任何匯入

### Requirement: GUI template download
資產畫面 SHALL 提供「下載範本」，以 Windows 另存新檔對話框選擇路徑（預設文件資料夾、檔名 `cvewatch-inventory-template.xlsx`），寫出與 `cvewatch asset template` 相同的空白範本；MUST NOT 覆蓋既有檔案，完成後 SHALL 顯示檔案路徑。

#### Scenario: Saving the template
- **WHEN** 使用者按「下載範本」並在對話框確認預設檔名
- **THEN** 文件資料夾中產生含 `assets`、`components` 工作表與範例列的範本，視窗顯示其路徑

#### Scenario: Template target already exists
- **WHEN** 使用者在對話框選擇已存在的檔案並確認取代
- **THEN** 系統不覆蓋該檔，顯示檔案已存在的錯誤，原檔內容不變

### Requirement: GUI inventory export
資產畫面 SHALL 提供「匯出清冊」，以 Windows 另存新檔對話框選擇路徑（預設文件資料夾、檔名含當天日期），匯出內容 SHALL 與 `cvewatch asset export` 相同；MUST NOT 覆蓋既有檔案，完成後 SHALL 顯示路徑、匯出的資產數與「此檔未加密」提示。

#### Scenario: Exporting the inventory
- **WHEN** 有 12 個 active 資產，使用者按「匯出清冊」並確認預設檔名
- **THEN** 文件資料夾中產生清冊檔，視窗顯示路徑、「已匯出 12 個資產」與「此檔未加密」提示

### Requirement: GUI asset editing
資產畫面 SHALL 提供「新增資產」與所選 active 資產的「編輯」，以對話框編輯資產欄位（category、曝露度、重要性以下拉選單選擇）與元件（產品以可搜尋的目錄產品清單選擇）；未辨識產品 SHALL 可改為目錄產品或刪除，每筆未辨識產品 SHALL 依「原文與版本、處置、刪除」的順序呈現。每個欄位 SHALL 只有一個標籤，必填欄位 SHALL 以「*」標示。驗證錯誤 SHALL 以介面用語（例如「必填」）顯示在對應欄位旁、MUST NOT 使用清冊的英文欄位名，且不關閉對話框；儲存失敗時 SHALL 保留對話框與輸入內容且資料庫 MUST NOT 變更；按「取消」時資料庫 MUST NOT 變更。對話框關閉時，該對話框造成的驗證錯誤 MUST NOT 留在狀態列。開啟「加入元件」時輸入焦點 SHALL 位於搜尋框，加入元件後 SHALL 移到該元件的版本欄；資產清單上按 Enter SHALL 開啟所選 active 資產的編輯。

#### Scenario: Adding an asset from the window
- **WHEN** 使用者按「新增資產」，填入 `FW-02`、`fw02`、security，加入元件 PAN-OS `11.2.3` 後按「儲存」
- **THEN** 對話框關閉，資產清單出現 `FW-02` 並選取它

#### Scenario: Invalid field keeps the dialog open
- **WHEN** 新增資產時 `asset_tag` 留空就按「儲存」
- **THEN** 對話框不關閉，「資產編號」欄位旁顯示「必填」，畫面不出現「asset_tag」，資料庫不變

#### Scenario: Editing a component version
- **WHEN** 使用者編輯 `FW-01`，把 PAN-OS 版本改為 `11.1.6` 後儲存
- **THEN** 資產詳情顯示新版本，該元件的版本歷史多一筆 `11.1.4-h7`

#### Scenario: Cancelled dialog leaves no error behind
- **WHEN** 新增資產時留空按「儲存」看到錯誤，再按「取消」
- **THEN** 對話框關閉，狀態列不再顯示欄位錯誤

#### Scenario: Keyboard flow when adding a component
- **WHEN** 使用者開啟「加入元件」並輸入 `pan`，選擇 PAN-OS
- **THEN** 搜尋框在開啟時即取得輸入焦點，選擇後焦點移到 PAN-OS 的版本欄，可直接輸入版本

### Requirement: GUI asset renaming
資產畫面的「編輯」對話框 SHALL 可修改「資產編號」（依 asset-inventory 的 Renaming an asset）；與原編號不同時欄位下方 SHALL 提示舊清冊與已匯出的 VEX 不會對應到新編號。新編號已被其他資產使用時，錯誤 SHALL 顯示在「資產編號」欄位旁且對話框不關閉。更名與其他修改 SHALL 在同一次儲存中全部寫入或全部不寫入，儲存後資產清單 SHALL 選取新編號。

#### Scenario: Renaming in the edit dialog
- **WHEN** 使用者編輯 `NAS-01`，把資產編號改為 `NAS-11`、名稱改為 `nas11` 後儲存
- **THEN** 輸入新編號時欄位下方顯示更名提示；儲存後對話框關閉，資產清單選取 `NAS-11`，名稱為 `nas11`，原有命中仍列在 `NAS-11` 之下

#### Scenario: Renaming to a tag in use
- **WHEN** 使用者編輯 `NAS-01`，把資產編號改為已存在的 `NAS-02`，並修改名稱後儲存
- **THEN** 對話框不關閉，「資產編號」欄位旁說明該編號已被使用（`NAS-02`），`NAS-01` 的編號與名稱都不變

### Requirement: GUI asset retirement and purge
資產畫面 SHALL 提供所選 active 資產的「退役…」（確認後標為退役；確認對話框標題 SHALL 寫明「將 <資產編號> 標為退役」並說明保留紀錄、不再比對、可恢復）與「顯示已退役」切換；資產清單上按 Delete SHALL 開啟所選 active 資產的退役確認。已退役資產 SHALL 提供「恢復」與「永久刪除」。永久刪除 SHALL 先說明將一併移除的元件數與命中數，並要求輸入完全相同的 `asset_tag` 才能確認。

#### Scenario: Deleting from the asset list
- **WHEN** 使用者選取 `NAS-02` 按「退役…」並在標題為「將 NAS-02 標為退役」的確認對話框按「標為退役」
- **THEN** `NAS-02` 從預設清單消失，開啟「顯示已退役」時以退役樣式列出

#### Scenario: Purge confirmation requires the tag
- **WHEN** 使用者對 retired 的 `NAS-02` 按「永久刪除」，在確認欄輸入 `NAS-0`
- **THEN** 「永久刪除」按鈕維持停用；輸入 `NAS-02` 後才可按下

### Requirement: GUI matching after edits
GUI 的資產編修、退役、恢復與永久刪除寫入後 SHALL 在背景執行比對（不凍結視窗），完成後重新載入資料並在狀態列顯示結果；找不到同資料夾的 `cvewatch.exe` 或比對失敗時，編修 SHALL 保留並以警告提示執行 `cvewatch match`。

#### Scenario: Matching runs in the background
- **WHEN** 使用者儲存 `FW-01` 的版本變更
- **THEN** 狀態列顯示「比對中」，視窗仍可操作；比對完成後命中清單重新載入

#### Scenario: Companion missing
- **WHEN** 同資料夾沒有 `cvewatch.exe`，使用者儲存編修
- **THEN** 編修已寫入，狀態列以警告顯示尚未比對，請執行 `cvewatch match`

### Requirement: GUI import overwrite reminder
清冊匯入預覽 SHALL 顯示提醒：匯入會以檔案內容覆寫資產欄位與清冊元件（檔案中沒有的清冊元件會被移除），在視窗中修改過資產時建議先「匯出清冊」再編輯該檔。

#### Scenario: Previewing after on-screen edits
- **WHEN** 使用者在視窗中修改過 `FW-01` 後拖入舊的清冊檔
- **THEN** 預覽顯示覆寫提醒與 `FW-01` 的版本變更明細，按「匯入」前資料庫不變

### Requirement: GUI asset presentation
資產畫面與資產對話框 SHALL 以使用者用語呈現資料：目錄產品 SHALL 以產品目錄的顯示名稱呈現（產品 key MUST NOT 作為主要文字，只可出現在提示中）；類別與曝露度 SHALL 以「英文值（中文名稱）」呈現；套件元件的版本 SHALL 只顯示一次。資產清單 SHALL 以編號、名稱、未處理命中數三個欄位呈現，未處理數大於 0 時 SHALL 醒目顯示，名稱與編號相同時 MUST NOT 重複顯示。資產沒有命中時 SHALL 顯示「沒有命中」而非空白。工具列 SHALL 將「新增資產」、清冊檔案操作（匯入清冊、下載範本、匯出清冊）與「顯示已退役」分組；「退役…」與「永久刪除」SHALL 以警示樣式顯示並與一般操作分開。

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
- **WHEN** 資產類別為 `server`
- **THEN** 詳情與類別下拉選單顯示「server（伺服器）」，儲存後資料庫與匯出的值仍為 `server`
