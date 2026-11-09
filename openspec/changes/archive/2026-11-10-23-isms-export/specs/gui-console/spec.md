# Spec Delta

## ADDED Requirements

### Requirement: GUI ISMS export
資產畫面工具列的清冊檔案組 SHALL 在「從 ISMS 清冊匯入」之後提供「匯出 ISMS 清冊」，以 Windows 另存新檔對話框選擇路徑（預設文件資料夾、檔名含當天日期），匯出內容 SHALL 與 `cvewatch asset export-isms` 相同，MUST NOT 覆蓋既有檔案。完成後 SHALL 顯示路徑、匯出的資產數、略過的資產編號、讀回警告與「此檔未加密」提示。此按鈕 MUST NOT 依賴同資料夾的 `cvewatch.exe`。取消對話框時 SHALL 不產生檔案並顯示已取消。

#### Scenario: Exporting the ISMS workbook from the window
- **WHEN** 有 5 個有 ISMS 編號的 active 資產，使用者按「匯出 ISMS 清冊」並確認預設檔名
- **THEN** 文件資料夾中產生 `cvewatch-isms-<當天日期>.xlsx`，視窗顯示路徑、「已匯出 5 個資產」與「此檔未加密」提示

#### Scenario: Skipped assets in the window
- **WHEN** active 資產 `SRV-01` 沒有 ISMS 編號，使用者匯出 ISMS 清冊
- **THEN** 完成訊息以警告樣式列出 `SRV-01` 因沒有 ISMS 編號而略過

#### Scenario: Cancelling the ISMS export
- **WHEN** 使用者在另存新檔對話框按取消
- **THEN** 不產生檔案，視窗顯示「已取消匯出 ISMS 清冊」

#### Scenario: ISMS export without the companion
- **WHEN** 同資料夾找不到 `cvewatch.exe`
- **THEN** 「匯出 ISMS 清冊」仍可使用，「從 ISMS 清冊匯入」依 GUI ISMS import 停用
