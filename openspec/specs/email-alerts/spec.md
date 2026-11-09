# email-alerts Specification

## Purpose

以 Email 在每次同步後即時通知緊急命中（列入 KEV 或 CVSS 達門檻的未處理命中），並以每日摘要彙整同步狀態、新命中、狀態變化與等待區進出；同一事件只通知一次，送出失敗時保留於送出佇列並以遞增間隔重送。

## Requirements

### Requirement: Urgent findings definition
命中狀態為未處理，且其 CVE 列入 KEV 或顯示用 CVSS 大於等於設定門檻（預設 9.0）時，SHALL 視為緊急；既有命中的 CVE 新列入 KEV SHALL 視為新的緊急事件。

#### Scenario: New critical finding
- **WHEN** sync 產生一筆 CVSS 9.8、未列入 KEV 的未處理命中
- **THEN** 該命中被視為緊急

#### Scenario: Existing finding becomes KEV
- **WHEN** 一筆兩週前建立、CVSS 7.5 的未處理命中，其 CVE 今天列入 KEV
- **THEN** 產生一個「列入 KEV」的緊急事件

### Requirement: Immediate urgent email
每次 sync 後若有新的緊急事件，系統 SHALL 寄出一封合併當次所有緊急事件的 Email；版本待確認的命中 SHALL 在信中標示。

#### Scenario: Two urgent findings in one sync
- **WHEN** 一次 sync 產生兩個緊急事件
- **THEN** 只寄出一封信，信中列出兩筆

### Requirement: No duplicate notifications
同一命中的同一類事件 SHALL 只通知一次。

#### Scenario: Next sync with no changes
- **WHEN** 下一次 sync 時上述兩筆仍為緊急但沒有新事件
- **THEN** 不再寄出緊急信

### Requirement: Daily digest
`cvewatch notify digest` SHALL 寄出涵蓋上次成功摘要至今的摘要，包含同步狀態、依等級排列的新命中、自動狀態變化、等待區進出與逾期，以及無法送出的通知。寄出時有版本待補的元件時，摘要 SHALL 另列待補的元件數與資產數，並說明這些元件未納入比對；這一行不算新事項。

#### Scenario: Digest after two days offline
- **WHEN** 筆電離線兩天後開機，摘要工作執行
- **THEN** 摘要涵蓋這兩天的所有變化，只寄一封

#### Scenario: Nothing new
- **WHEN** 摘要期間沒有任何變化且 `send_empty` 為預設值
- **THEN** 寄出一封註明「無新事項」並附同步狀態的摘要

#### Scenario: Pending versions in the digest
- **WHEN** 摘要期間沒有任何變化，有 2 個資產共 3 個元件為版本待補
- **THEN** 摘要註明「無新事項」，並列出「版本待補 3 個元件（2 個資產），未納入比對」

### Requirement: Retry failed sends
Email 送出失敗時，系統 SHALL 保留於送出佇列並以遞增間隔重送；超過 10 次 SHALL 標為無法送出並在摘要與 Dashboard 顯示。

#### Scenario: SMTP temporarily unreachable
- **WHEN** 寄送緊急信時 SMTP 連線失敗
- **THEN** 信件留在佇列，下次 sync 或摘要執行時重送，成功後才記錄為已通知

#### Scenario: Password not stored yet
- **WHEN** Email 已啟用並設定帳號，但 Credential Manager 中還沒有 SMTP 密碼
- **THEN** 緊急信留在佇列且不計入重送次數，每次執行說明缺少密碼；存入密碼後下次執行即寄出

### Requirement: Encrypted SMTP transport
SMTP 連線 SHALL 使用 STARTTLS（伺服器不支援時失敗，不得降級）或 SMTPS，且 SHALL 驗證伺服器憑證；系統 MUST NOT 提供略過憑證驗證的選項，內部 CA SHALL 以設定的 CA 檔加入信任。

#### Scenario: Server does not offer STARTTLS
- **WHEN** 加密設為 STARTTLS，伺服器未提供 STARTTLS
- **THEN** 寄送失敗並顯示「伺服器不支援 STARTTLS」，不以明文寄出

#### Scenario: Internal CA
- **WHEN** SMTP 憑證由機構自簽 CA 簽發，且 `ca_file` 指向該 CA 的 PEM
- **THEN** 連線驗證成功

### Requirement: Insecure transport exception
只有在設定檔手動設定 `allow_insecure_transport = true` 時，系統 SHALL 接受無加密 SMTP；此時 SHALL 只允許無驗證寄送，MUST NOT 經明文連線送出 SMTP 密碼。

#### Scenario: Plain SMTP with password configured
- **WHEN** 例外開關已啟用、加密為 none，且憑證庫中有 SMTP 密碼
- **THEN** 寄送失敗並說明明文連線不得傳送密碼

### Requirement: Email configuration test
系統 SHALL 提供 `cvewatch notify test-email`，以目前設定寄送測試信並顯示 SMTP 錯誤細節。

#### Scenario: Wrong password
- **WHEN** Credential Manager 中的 SMTP 密碼錯誤
- **THEN** 指令失敗並顯示伺服器回傳的驗證錯誤，不顯示密碼
