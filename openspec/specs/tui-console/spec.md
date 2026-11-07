# tui-console Specification

## Purpose

提供終端機互動介面，讓使用者瀏覽風險概況、篩選命中、查看判定證據、變更處理狀態並管理等待區，行為與 CLI 一致。

## Requirements

### Requirement: Dashboard overview
TUI 開啟時 SHALL 顯示 Dashboard，內容包含各等級未處理命中數、KEV 命中數、等待區數量與逾期數、來源矛盾數，以及每個來源最後一次同步的時間與結果。

#### Scenario: Opening the console
- **WHEN** 使用者執行 `cvewatch tui`
- **THEN** 畫面顯示 Dashboard 及上述各項數字

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
TUI SHALL 提供資產畫面（資產、元件、各資產命中、未辨識產品）與等待區畫面（原因、進入天數、逾期、KEV 置頂），並 SHALL 可在等待區手動套用到資產或標為不相關。

#### Scenario: Applying a pending CVE to an asset
- **WHEN** 使用者在等待區選擇一筆 CVE，選擇套用到 `PVE-01`
- **THEN** 該 CVE 移出等待區，並在 Findings 出現一筆 PVE-01 的 probable 命中

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
