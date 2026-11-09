# Spec Delta

## MODIFIED Requirements

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

### Requirement: Assets and pending screens
TUI SHALL 提供資產畫面（資產、元件、各資產命中、未辨識產品）與等待區畫面（原因、進入天數、逾期、KEV 置頂），並 SHALL 可在等待區手動套用到資產或標為不相關。資產畫面 SHALL 標示有版本待補元件的資產，並以「版本待補」呈現待補元件的版本。

#### Scenario: Applying a pending CVE to an asset
- **WHEN** 使用者在等待區選擇一筆 CVE，選擇套用到 `PVE-01`
- **THEN** 該 CVE 移出等待區，並在 Findings 出現一筆 PVE-01 的 probable 命中

#### Scenario: Pending component in the assets screen
- **WHEN** 使用者在資產畫面選取 DSM 元件為版本待補的 `NET-HW-003`
- **THEN** 該資產列有版本待補標示，元件顯示「Synology DSM 版本待補」
