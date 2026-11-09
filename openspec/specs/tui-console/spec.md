# tui-console Specification

## Purpose

提供終端機互動介面，讓使用者瀏覽風險概況、篩選命中、查看判定證據、變更處理狀態並管理等待區，行為與 CLI 一致。

## Requirements

### Requirement: Dashboard overview
TUI 開啟時 SHALL 顯示 Dashboard，內容包含各等級未處理命中數、KEV 命中數、等待區數量與逾期數、來源矛盾數、版本待補的元件數與資產數，以及每個來源最後一次同步的時間與結果。

#### Scenario: Opening the console
- **WHEN** 使用者執行 `cvewatch tui`
- **THEN** 畫面顯示 Dashboard 及上述各項數字

#### Scenario: Pending versions on the dashboard
- **WHEN** 有 2 個資產共 3 個元件為版本待補
- **THEN** Dashboard 顯示「版本待補 3 個元件（2 個資產），未納入比對」

#### Scenario: A source failed last time
- **WHEN** 最近一次 OSV 同步失敗
- **THEN** Dashboard 的 OSV 來源標示為失敗並顯示時間

#### Scenario: Earlier source failed but the last one succeeded
- **WHEN** 最近一次 NVD 同步失敗，之後結束的 KEV 同步成功
- **THEN** 標題列的最後同步仍標示失敗並顯示失敗的來源數

### Requirement: Findings list with filter and sort
Findings 畫面 SHALL 以風險排序列出命中，並 SHALL 支援依狀態、等級、資產、信心等級與 KEV 篩選，以及即時文字搜尋。

#### Scenario: Default view
- **WHEN** 使用者切換到 Findings
- **THEN** 預設只顯示未處理命中，依 P1 到 P4 及同級排序規則排列

#### Scenario: Searching by CVE ID
- **WHEN** 使用者按 `/` 輸入 `2026-0`
- **THEN** 清單即時只顯示 CVE ID、資產名稱或描述含該字串的命中

### Requirement: Finding detail shows evidence
Finding 詳情 SHALL 顯示 CVE 描述、各來源 CVSS、EPSS、KEV 資訊、每個來源的判定與依據的版本區間、來源矛盾、原廠修正版本、處理時間軸與參考連結。

#### Scenario: Viewing why a finding exists
- **WHEN** 使用者在 PAN-OS 命中上按 Enter
- **THEN** 詳情顯示判定來源（例如原廠公告）、受影響區間與元件目前版本

### Requirement: Triage from the console
使用者 SHALL 能在 Findings 清單或詳情中以單一按鍵將一筆或多筆命中設為已處理、忽略或未處理，並 SHALL 在套用前輸入備註；結果 SHALL 與 CLI 的狀態變更紀錄相同。

#### Scenario: Batch ignore
- **WHEN** 使用者以 Space 選取三筆命中後按 `i`，選擇原因「不受影響」與理由「需特定設定」，輸入備註並確認
- **THEN** 三筆命中變為 ignored 並帶相同原因與理由，每筆的時間軸各新增一筆含該備註的事件

#### Scenario: Cancelling the note dialog
- **WHEN** 使用者在備註框按 Esc
- **THEN** 狀態不變更

#### Scenario: Batch includes findings already in that status
- **WHEN** 使用者選取兩筆未處理命中後按 `u` 並確認
- **THEN** 兩筆都不寫入事件，狀態列說明原本就是未處理、未變更

#### Scenario: Selected findings hidden by search
- **WHEN** 使用者選取三筆命中後以 `/` 搜尋，使其中兩筆不再顯示，再按 `r` 並確認
- **THEN** 只有仍顯示的一筆變更狀態，被隱藏的兩筆維持原狀且仍保持選取

### Requirement: Assets and pending screens
TUI SHALL 提供資產畫面（資產、元件、各資產命中、未辨識產品）與等待區畫面（原因、進入天數、逾期、KEV 置頂），並 SHALL 可在等待區手動套用到資產或標為不相關。資產畫面 SHALL 標示有版本待補元件的資產，並以「版本待補」呈現待補元件的版本。

#### Scenario: Applying a pending CVE to an asset
- **WHEN** 使用者在等待區選擇一筆 CVE，選擇套用到 `PVE-01`
- **THEN** 該 CVE 移出等待區，並在 Findings 出現一筆 PVE-01 的 probable 命中

#### Scenario: Pending component in the assets screen
- **WHEN** 使用者在資產畫面選取 DSM 元件為版本待補的 `NET-HW-003`
- **THEN** 該資產列有版本待補標示，元件顯示「Synology DSM 版本待補」

### Requirement: Opening references
使用者 SHALL 能以按鍵用系統預設瀏覽器開啟 CVE.org、NVD 或原廠公告連結；有多個連結時 SHALL 先顯示選單。

#### Scenario: Open vendor advisory
- **WHEN** 使用者在詳情按 `o` 並選擇原廠公告
- **THEN** 預設瀏覽器開啟該公告網址

### Requirement: New data notification
TUI 開啟期間若有 sync 寫入新資料，SHALL 在狀態列提示，且 MUST NOT 自動重排目前清單；使用者按 `R` 後才重新載入。

#### Scenario: Scheduled sync finishes while browsing
- **WHEN** 使用者正在瀏覽 Findings，排程 sync 完成
- **THEN** 狀態列顯示有新資料的提示，清單維持不變直到使用者按 `R`

#### Scenario: Automatic match finishes after reload
- **WHEN** 使用者在 sync 的來源都結束後按 `R`，之後 sync 結尾的自動比對才完成
- **THEN** 狀態列再次顯示有新資料的提示

### Requirement: Background sync from the console
使用者 SHALL 能按 `S` 在背景啟動一次 sync，TUI SHALL 保持可操作並在 sync 結束後顯示結果。

#### Scenario: Sync already running
- **WHEN** 使用者按 `S` 時排程 sync 正在執行
- **THEN** 狀態列顯示「已有 sync 正在執行」

### Requirement: Wide character layout
表格欄位 SHALL 依顯示寬度截斷含中文的內容，不得造成欄位錯位。

#### Scenario: Long Chinese asset name
- **WHEN** 資產名稱為「資訊所機房核心防火牆主機」且欄寬不足
- **THEN** 名稱以顯示寬度截斷並加上省略符號，其他欄位對齊不變

### Requirement: Settings screen
TUI SHALL 提供「設定」畫面，分為 Email、Redmine、排程與門檻三個分頁，可檢視與編輯通知與排程相關設定。

#### Scenario: Opening settings
- **WHEN** 使用者在 TUI 按 `6`
- **THEN** 顯示設定畫面的 Email 分頁，欄位填入目前設定檔的值

### Requirement: Editable SMTP settings
設定畫面 SHALL 可編輯 Email 啟用、SMTP 主機、連接埠、加密方式、帳號、密碼、寄件者與一到多個收件者。

#### Scenario: Changing SMTP host
- **WHEN** 使用者將 SMTP 主機改為 `mail.example.edu.tw` 並按 Ctrl+S
- **THEN** `cvewatch.toml` 的 `notify.email.smtp_host` 更新為該值，其他內容與註解保留不變

### Requirement: Editable Redmine settings
設定畫面 SHALL 可編輯 Redmine 啟用、網址、API key、專案、tracker、P1/P2 優先權對應、指派對象與狀態變更註記開關。

#### Scenario: Toggling triage notes
- **WHEN** 使用者關閉「狀態變更時加註記」並儲存
- **THEN** `notify.redmine.add_note_on_triage` 變為 false

### Requirement: Pick Redmine IDs from lists
填妥 Redmine 網址與 API key 後，設定畫面 SHALL 能從 Redmine 載入專案、該專案啟用的 tracker、優先權與專案成員供使用者以清單挑選，並以「名稱 (#ID)」顯示；設定檔 SHALL 儲存數字 ID。無法連線時 SHALL 允許手動輸入 ID。

#### Scenario: Selecting a project and tracker
- **WHEN** 使用者按 `l` 載入後，選擇專案「基礎設施資安」與 tracker「漏洞」
- **THEN** 畫面顯示「基礎設施資安 (#12)」「漏洞 (#3)」，儲存後設定檔記錄對應 ID

#### Scenario: Redmine unreachable
- **WHEN** 載入時 Redmine 無法連線
- **THEN** 畫面顯示錯誤原因，專案與 tracker 欄位改為可手動輸入數字

### Requirement: Secrets entered in the console
密碼與 API key SHALL 以遮罩輸入，儲存時 SHALL 只寫入作業系統憑證庫；畫面 SHALL 只顯示「已設定」或「未設定」，MUST NOT 顯示或寫入設定檔任何秘密內容。

#### Scenario: Setting the Redmine API key
- **WHEN** 使用者在 API key 欄位輸入金鑰並儲存
- **THEN** 金鑰存入憑證庫，欄位顯示「已設定」，`cvewatch.toml` 中找不到該金鑰

#### Scenario: Clearing the SMTP password
- **WHEN** 使用者在密碼欄位按 Del 並儲存
- **THEN** 憑證庫中的 SMTP 密碼被刪除，欄位顯示「未設定」

### Requirement: No insecure options in the console
設定畫面 MUST NOT 提供無加密 SMTP 選項、http Redmine 網址或明文傳輸例外開關；例外開關已在設定檔啟用時，TUI SHALL 在所有畫面頂端常駐顯示警示。

#### Scenario: Exception enabled in the file
- **WHEN** 使用者在設定檔手動設定 `allow_insecure_transport = true` 後開啟 TUI
- **THEN** 每個畫面頂端都顯示「明文傳輸已啟用」紅色警示列

### Requirement: Validation before saving
設定畫面 SHALL 即時驗證欄位（連接埠範圍、Email 格式、網址必須為 https、門檻 0–10、時間 HH:MM）；任何欄位無效時 SHALL 標示錯誤並拒絕儲存。

#### Scenario: Invalid port
- **WHEN** 使用者將連接埠輸入為 `70000` 後按 Ctrl+S
- **THEN** 連接埠欄位標示錯誤，設定檔不變

### Requirement: Connection tests from settings
設定畫面 SHALL 提供測試寄信與測試 Redmine，使用目前表單中（含尚未儲存）的值，並顯示成功或伺服器回傳的錯誤；測試 MUST NOT 建立 Redmine 議題。

#### Scenario: Testing unsaved SMTP settings
- **WHEN** 使用者修改連接埠為 465、加密改為 TLS，未儲存即按 `t`
- **THEN** 系統以 465/TLS 寄出測試信並顯示結果，設定檔不變

### Requirement: Unsaved changes protection
有未儲存的變更時離開設定畫面或結束 TUI，系統 SHALL 詢問儲存、放棄或取消。

#### Scenario: Leaving with unsaved changes
- **WHEN** 使用者修改收件者後按 Esc
- **THEN** 出現儲存 / 放棄 / 取消的確認對話框

### Requirement: Schedule change prompt
儲存後若同步間隔或摘要時間有變更，系統 SHALL 提示重新安裝排程，並 SHALL 可在設定畫面直接執行重新安裝。

#### Scenario: Digest time changed
- **WHEN** 使用者將摘要時間由 08:30 改為 07:45 並儲存
- **THEN** 畫面提示排程已變更，按 `i` 後 `cvewatch\digest` 的觸發時間更新為 07:45

### Requirement: Export VEX from findings view
Findings 畫面 SHALL 提供以目前篩選條件匯出 CycloneDX VEX 的操作，並 SHALL 讓使用者確認或修改輸出路徑。

#### Scenario: Exporting filtered findings
- **WHEN** 使用者在 Findings 篩選 `FW-01` 後按 `E` 並確認預設路徑
- **THEN** 文件資料夾中產生只含 FW-01 命中的 VEX 檔，狀態列顯示路徑與「此檔未加密」提示

### Requirement: Asset identity in the assets screen
TUI 資產詳情 SHALL 顯示廠牌、型號、序號、財產編號、主機名稱、管理介面位址與 IP（每筆「位址（用途）」，以「、」分隔），未填的欄位與沒有 IP 時 SHALL 顯示「-」。TUI MUST NOT 提供開啟管理介面位址或編修這些欄位的操作。

#### Scenario: Identity in the asset detail
- **WHEN** 使用者在資產畫面選取廠牌「Aruba」、型號 `XYZ-100`、IP 為 `192.0.2.1`（用途「管理」）與 `2001:db8::1`、沒有序號的 `FW-EDGE`
- **THEN** 詳情顯示廠牌 Aruba、型號 `XYZ-100`、序號 -，以及 IP `192.0.2.1（管理）、2001:db8::1`

### Requirement: TUI component roles
TUI 資產詳情的元件表格 SHALL 在第一欄顯示每個元件所屬組的名稱（角色中文名稱，套件元件為「套件」），並 SHALL 依 asset-inventory 的 Component grouping by role 的組順序排列，同組的元件相鄰且維持原本順序。其餘欄位（元件、版本、來源、更新時間）SHALL 不變。

#### Scenario: Role column in the asset detail
- **WHEN** 使用者在資產畫面開啟有 Ubuntu、iDRAC9 兩個清冊元件與一個 dpkg 套件元件的 `SRV-01`
- **THEN** 元件表格的第一欄標題為「角色」，各列依序為「BMC」（iDRAC9）、「作業系統」（Ubuntu）、「套件」（該套件）

#### Scenario: Single firmware component
- **WHEN** 使用者開啟只有 PAN-OS 元件的 `FW-EDGE`
- **THEN** 元件表格只有一列，角色欄為「韌體」

### Requirement: Lifecycle in the console
TUI 的資產畫面 SHALL 以一欄標示使用中資產的生命週期狀態（已 EoL、即將 EoL、即將到期、已到期），並 SHALL 可用按鍵 `l` 依序切換生命週期篩選：全部、已 EoL、即將 EoL、保固或合約到期；目前的篩選 SHALL 顯示在畫面標題。資產詳情 SHALL 列出硬體 EoL、各清冊元件的軟體 EoL 與三個到期日的日期、狀態、天數與來源。Dashboard SHALL 顯示已 EoL、即將 EoL 與保固或合約到期的資產數；三者都為 0 時 SHALL 不顯示。

#### Scenario: Filtering EoL assets in the console
- **WHEN** 有 2 個使用中資產已 EoL，使用者在資產畫面按 `l` 一次
- **THEN** 標題顯示篩選「已 EoL」，清單只列出這 2 個資產

#### Scenario: Lifecycle in the asset detail
- **WHEN** 今天為 2027-01-10，使用者開啟 `SRV-01` 的詳情，保固到期為 2026-12-31
- **THEN** 詳情列出「保固到期 2026-12-31 已到期（10 天前）」

#### Scenario: Lifecycle counts on the dashboard
- **WHEN** 有 1 個資產即將 EoL、3 個資產保固或合約到期
- **THEN** Dashboard 顯示「即將 EoL 1、保固或合約到期 3（提醒 90 天）」
