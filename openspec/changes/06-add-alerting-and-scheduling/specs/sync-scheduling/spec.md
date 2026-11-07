# Spec Delta

## Purpose

讓 cvewatch 在 Windows 上自動定期同步與寄送每日摘要，不需要使用者手動執行，也不會在排程執行時跳出視窗。

## ADDED Requirements

### Requirement: Installing scheduled tasks
`cvewatch schedule install` SHALL 以目前登入的 Windows 使用者建立「定期 sync」與「每日摘要」兩個排程工作，間隔與時間取自設定檔；重複執行 SHALL 以新設定取代既有工作。

#### Scenario: First installation
- **WHEN** 使用者執行 `cvewatch schedule install`，設定為每 4 小時與 08:30
- **THEN** 工作排程器中出現 `cvewatch\sync` 與 `cvewatch\digest`，觸發時間符合設定

#### Scenario: Reinstalling with a new interval
- **WHEN** 使用者將 `sync_every_hours` 改為 2 後再次執行 install
- **THEN** `cvewatch\sync` 的間隔更新為 2 小時，不產生重複工作

### Requirement: Catch-up and network conditions
排程工作 SHALL 在錯過預定時間後、電腦可用時補執行，且 SHALL 只在有網路時執行，使用電池時不得停止。

#### Scenario: Laptop was off at digest time
- **WHEN** 筆電在 08:30 時關機，09:10 開機登入
- **THEN** 每日摘要工作在開機後執行一次

### Requirement: Windowless execution
排程工作 SHALL 透過無主控台的包裝程式執行，執行期間 MUST NOT 顯示主控台視窗，並 SHALL 傳回實際命令的結束碼。

#### Scenario: Scheduled sync runs while user is working
- **WHEN** 排程 sync 在使用者操作其他程式時啟動
- **THEN** 畫面上不出現任何主控台視窗，工作排程器記錄的結果碼等於 sync 的結束碼

### Requirement: Schedule status and removal
系統 SHALL 提供 `cvewatch schedule status` 顯示各工作的上次執行時間、結果與下次執行時間，並 SHALL 提供 `schedule uninstall` 移除兩個工作。

#### Scenario: Checking status
- **WHEN** 使用者執行 `cvewatch schedule status`
- **THEN** 輸出顯示 sync 與 digest 各自的上次時間、上次結果碼與下次時間

#### Scenario: Status when not installed
- **WHEN** 尚未安裝排程時執行 status
- **THEN** 輸出說明尚未安裝並提示 `schedule install`
