## ADDED Requirements

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
