## ADDED Requirements

### Requirement: GUI lifecycle fields input
資產對話框 SHALL 有「生命週期」區塊，可輸入保固到期、維護合約到期、軟體授權到期與硬體 EoL 覆寫（日期與來源）；每個清冊元件 SHALL 可設定或清除軟體 EoL 覆寫（日期與來源）。日期欄 SHALL 接受與清冊相同的寫法，驗證錯誤 SHALL 標在對應欄位旁，有錯誤時 MUST NOT 儲存。

#### Scenario: Entering a warranty date
- **WHEN** 使用者在 `SRV-01` 的對話框把保固到期填為 `2027/12/31` 並儲存
- **THEN** 保固到期儲存為 2027-12-31，詳情顯示 2027-12-31

#### Scenario: Invalid date in the dialog
- **WHEN** 使用者把維護合約到期填為 `2027-13-01` 並按儲存
- **THEN** 該欄旁顯示日期無效，資產不變

#### Scenario: Component override in the dialog
- **WHEN** 使用者為 `SRV-01` 的 ESXi 元件設定 EoL 覆寫 2028-12-31、來源「延長支援合約」並儲存
- **THEN** 詳情中 ESXi 的軟體 EoL 為 2028-12-31，來源顯示「延長支援合約（手動覆寫）」

### Requirement: GUI lifecycle markers and filter
資產清單 SHALL 在使用中資產的名稱後標示「已 EoL」「即將 EoL」或「即將到期」「已到期」（保固、維護合約或軟體授權），同一資產有多種狀態時 SHALL 全部標示，已 EoL 與已到期 SHALL 以警示色顯示。資產畫面 SHALL 提供生命週期篩選：全部、已 EoL、即將 EoL、保固或合約到期（含軟體授權的即將到期與已到期）；篩選 SHALL 可與「顯示已退役」「只顯示版本待補」同時使用。

#### Scenario: Marker in the list
- **WHEN** 今天為 2027-01-10，`FW-EDGE` 的 PAN-OS EoL 為 2027-03-01
- **THEN** 清單中 `FW-EDGE` 列標示「即將 EoL」

#### Scenario: Filtering expiring contracts
- **WHEN** `SRV-01` 的維護合約將在 30 天內到期、`FW-EDGE` 沒有到期項目，使用者選擇「保固或合約到期」
- **THEN** 清單只列出 `SRV-01`

### Requirement: GUI lifecycle in asset detail
資產詳情 SHALL 有「生命週期」區塊，列出硬體 EoL、每個清冊元件的軟體 EoL（含版本分支）與三個到期日，每一項 SHALL 顯示日期、狀態、剩餘或已過天數與來源；來源為網址時 SHALL 可點選以預設瀏覽器開啟。沒有資料的項目 SHALL 顯示原因（例如「未填」「目錄沒有此型號」「無法對應版本分支」「無 EoL 資料」）。

#### Scenario: Opening the source of an EoL date
- **WHEN** 使用者在 `SW-CORE` 的詳情點選硬體 EoL 的來源網址
- **THEN** 以預設瀏覽器開啟該網址

#### Scenario: Unmapped version in the detail pane
- **WHEN** `PVE-01` 的 Proxmox VE 版本無法對應任何版本分支
- **THEN** 詳情顯示「Proxmox VE 無法對應版本分支」與元件版本

### Requirement: GUI dashboard lifecycle reminder
Dashboard SHALL 顯示已 EoL、即將 EoL 與保固或合約到期（含即將到期與已到期）的資產數，以及提醒天數；點選任一數字 SHALL 開啟套用對應生命週期篩選的資產畫面。三者都為 0 時 SHALL 不顯示此區塊。

#### Scenario: Jumping to EoL assets
- **WHEN** 有 2 個使用中資產已 EoL，使用者在 Dashboard 點選已 EoL 數字
- **THEN** 切換到資產畫面，生命週期篩選為「已 EoL」，只列出這 2 個資產
