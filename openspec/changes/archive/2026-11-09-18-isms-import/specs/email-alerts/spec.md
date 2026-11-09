# Spec Delta

## MODIFIED Requirements

### Requirement: Daily digest
`cvewatch notify digest` SHALL 寄出涵蓋上次成功摘要至今的摘要，包含同步狀態、依等級排列的新命中、自動狀態變化、等待區進出與逾期，以及無法送出的通知。寄出時有版本待補的元件時，摘要 SHALL 另列待補的元件數與資產數，並說明這些元件未納入比對；這一行不算新事項。

#### Scenario: Digest after two days offline
- **WHEN** 筆電離線兩天後開機，摘要工作執行
- **THEN** 摘要涵蓋這兩天的所有變化，只寄一封

#### Scenario: Nothing new
- **WHEN** 摘要期間沒有任何變化且 `send_empty` 為預設值
- **THEN** 寄出一封註明「無新事項」並附同步狀態的摘要

#### Scenario: Pending versions in the digest
- **WHEN** 摘要期間沒有任何變化，有 2 個資產共 3 個元件為版本待補
- **THEN** 摘要註明「無新事項」，並列出「版本待補 3 個元件（2 個資產），未納入比對」
