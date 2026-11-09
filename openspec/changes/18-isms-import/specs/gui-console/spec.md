# Spec Delta

## ADDED Requirements

### Requirement: GUI ISMS import
資產畫面工具列的清冊檔案組 SHALL 提供「從 ISMS 清冊匯入」：以開啟檔案對話框選擇 ISMS 清冊（.xlsx）後，先顯示與 `cvewatch asset import-isms --dry-run` 相同的預覽，使用者確認後才寫入，完成後自動比對並重新載入資料。預覽 MUST NOT 提供退役選項。執行方式與「匯入清冊」相同，同資料夾找不到 `cvewatch.exe` 時 SHALL 停用並說明原因。

#### Scenario: Importing from the window
- **WHEN** 使用者按「從 ISMS 清冊匯入」並選擇 ISMS 清冊
- **THEN** 視窗顯示預覽（例如「新增 5、更新 1、版本待補 7 個元件、略過 10 列」與明細），資料庫尚未變更；按「匯入」後才寫入，並自動比對與重新載入

#### Scenario: Cancelling the ISMS preview
- **WHEN** 使用者在 ISMS 匯入預覽按「取消」
- **THEN** 資料庫不變

#### Scenario: Not an ISMS workbook in the window
- **WHEN** 使用者選擇的檔案不是 ISMS 資訊資產清冊
- **THEN** 視窗顯示找不到 ISMS 清冊表頭的錯誤，資料庫不變

### Requirement: GUI pending versions
資產清單 SHALL 標示有版本待補元件的資產；資產詳情與編輯對話框 SHALL 以「版本待補」呈現待補元件的版本，並提示該元件未納入比對。資產畫面 SHALL 可篩選只顯示有版本待補元件的資產。在編輯對話框中未修改的待補元件 MUST NOT 阻擋儲存。

#### Scenario: Pending component in the detail pane
- **WHEN** 使用者選取 DSM 元件為版本待補的 `NET-HW-003`
- **THEN** 清單中該列有版本待補標示，詳情顯示「Synology DSM 版本待補（未納入比對）」

#### Scenario: Saving other fields with a pending component
- **WHEN** 使用者編輯 `NET-HW-003` 的保管人並儲存，DSM 元件仍為版本待補
- **THEN** 保管人更新，DSM 元件仍為版本待補

## MODIFIED Requirements

### Requirement: GUI navigation and dashboard
視窗版 SHALL 提供 Dashboard、命中、資產、等待區、同步紀錄與設定六個畫面，可用側邊導覽或 `Ctrl+1`～`Ctrl+6` 切換。Dashboard SHALL 顯示與 TUI 相同的內容：各等級未處理命中數、KEV 命中數、等待區數量與逾期數、來源矛盾數、版本待補的元件數與資產數，以及每個來源最後一次同步的時間與結果；點選等級或 KEV 數字 SHALL 開啟套用對應篩選的命中畫面，點選版本待補數字 SHALL 開啟只顯示有版本待補元件的資產畫面。

#### Scenario: Jumping from a dashboard count
- **WHEN** 使用者在 Dashboard 點選 P1 未處理命中數
- **THEN** 切換到命中畫面，篩選為「未處理、P1」

#### Scenario: Jumping to pending versions
- **WHEN** 有 2 個資產共 3 個元件為版本待補，使用者在 Dashboard 點選版本待補數字
- **THEN** 切換到資產畫面，只列出這 2 個資產

#### Scenario: A source failed last time
- **WHEN** 最近一次 OSV 同步失敗
- **THEN** Dashboard 的 OSV 來源標示為失敗並顯示時間

## REMOVED Requirements

### Requirement: GUI ISMS draft
**Reason**: ISMS 清冊改為直接匯入，草稿功能移除。
**Migration**: 改用資產畫面的「從 ISMS 清冊匯入」；匯入後在編輯對話框補上版本。
