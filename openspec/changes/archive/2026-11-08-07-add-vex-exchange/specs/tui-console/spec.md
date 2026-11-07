# Spec Delta

## ADDED Requirements

### Requirement: Export VEX from findings view
Findings 畫面 SHALL 提供以目前篩選條件匯出 CycloneDX VEX 的操作，並 SHALL 讓使用者確認或修改輸出路徑。

#### Scenario: Exporting filtered findings
- **WHEN** 使用者在 Findings 篩選 `FW-01` 後按 `E` 並確認預設路徑
- **THEN** 文件資料夾中產生只含 FW-01 命中的 VEX 檔，狀態列顯示路徑與「此檔未加密」提示
