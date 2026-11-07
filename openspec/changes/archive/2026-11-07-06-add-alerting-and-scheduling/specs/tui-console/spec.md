# Spec Delta

## ADDED Requirements

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
