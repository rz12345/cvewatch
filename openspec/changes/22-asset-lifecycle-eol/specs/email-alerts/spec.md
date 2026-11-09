## ADDED Requirements

### Requirement: Lifecycle reminders in the daily digest
每日摘要 SHALL 有「生命週期提醒」段落，列出使用中資產在提醒天數內將 EoL 或到期的項目（資產、項目、日期、剩餘天數），與在摘要期間內 EoL 或到期的項目，並以一行列出目前已 EoL 與已到期的資產數。在摘要期間內進入提醒天數、或在摘要期間內 EoL 或到期的項目 SHALL 算新事項；其他生命週期內容 MUST NOT 算新事項。沒有任何生命週期項目時 SHALL 省略此段落。

#### Scenario: A support contract enters the reminder window
- **WHEN** `reminder_days` 為 90，`SRV-01` 的維護合約到期為 2027-04-10，摘要涵蓋 2027-01-09 至 2027-01-10
- **THEN** 摘要的生命週期提醒列出 `SRV-01` 維護合約到期 2027-04-10（剩 90 天），摘要不註明「無新事項」

#### Scenario: Reminder already sent before
- **WHEN** 摘要期間沒有其他變化，`FW-EDGE` 的 PAN-OS EoL 已在數天前進入提醒天數
- **THEN** 摘要註明「無新事項」，生命週期提醒仍列出 `FW-EDGE` 的 PAN-OS EoL 與剩餘天數

#### Scenario: Hardware reached end of life during the period
- **WHEN** `SW-CORE` 的硬體 EoL 為 2027-01-10，摘要涵蓋 2027-01-09 至 2027-01-10
- **THEN** 摘要列出 `SW-CORE` 硬體 EoL 已於 2027-01-10 到期，已 EoL 資產數包含 `SW-CORE`
