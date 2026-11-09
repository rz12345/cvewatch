## ADDED Requirements

### Requirement: GUI ISMS draft
資產畫面工具列的清冊檔案組 SHALL 提供「由 ISMS 清冊產生草稿」：先以開啟檔案對話框選擇 ISMS 清冊（.xlsx），再以另存新檔對話框選擇草稿路徑（預設為文件資料夾、檔名 `cvewatch-isms-draft-<日期>.xlsx`），寫出與 `cvewatch asset isms-draft` 相同的草稿。完成後 SHALL 顯示草稿路徑、轉換與略過的數量、待補版本的元件數，並提醒補上版本後再用「匯入清冊」匯入、不要勾選退役選項。MUST NOT 覆蓋既有檔案，也 MUST NOT 修改資料庫；任一對話框取消時 SHALL 不產生檔案。

#### Scenario: Generating a draft from the window
- **WHEN** 使用者按「由 ISMS 清冊產生草稿」，選擇 ISMS 清冊並確認預設草稿檔名
- **THEN** 文件資料夾中產生草稿，視窗顯示路徑、例如「轉換 6 個資產（沿用既有 1）、略過 10 列、待補版本 7 個元件」，以及補版本後再匯入的提醒

#### Scenario: Not an ISMS workbook in the window
- **WHEN** 使用者選擇的檔案不是 ISMS 資訊資產清冊
- **THEN** 視窗顯示找不到 ISMS 清冊表頭的錯誤，不產生草稿

#### Scenario: Cancelling the save dialog
- **WHEN** 使用者選好 ISMS 清冊後，在另存新檔對話框按「取消」
- **THEN** 不產生任何檔案，狀態列顯示已取消
