# Spec Delta

## ADDED Requirements

### Requirement: Pending versions are not matched
比對 MUST NOT 評估版本待補的元件，也 MUST NOT 為它們建立 probable 命中。元件改為版本待補時，其既有命中與處理紀錄 SHALL 保留但不再更新，來源矛盾 MUST NOT 計入 Dashboard。只與版本待補元件的產品有關的等待區 CVE SHALL 維持原狀，MUST NOT 因該產品未參與比對而移出。填上版本後，下一次比對 SHALL 照常評估該元件，並沿用原有的命中。

#### Scenario: Pending component is skipped
- **WHEN** `NET-HW-003` 的 Synology DSM 元件為版本待補，本機有 5 筆 DSM 的 CVE
- **THEN** 比對後 `NET-HW-003` 沒有任何命中

#### Scenario: Known version becomes pending
- **WHEN** `FW-01` 的 PAN-OS 元件有 1 筆 open 命中，重匯後該元件改為版本待補
- **THEN** 該命中仍為 open 且不再更新，直到版本填上後的下一次比對

#### Scenario: Waiting CVE of a pending product
- **WHEN** 等待區有一筆只與 Synology DSM 有關的 CVE，唯一的 DSM 元件改為版本待補後比對
- **THEN** 該 CVE 仍在等待區，進入時間不變

### Requirement: Pending versions in matching summary
`cvewatch match` 與 `cvewatch sync` 結束時，有版本待補的元件時 SHALL 印出待補的元件數與資產數，並說明這些元件未納入比對；沒有時 MUST NOT 印出。

#### Scenario: Summary after sync
- **WHEN** 有 2 個資產共 3 個元件為版本待補，`cvewatch sync` 結束
- **THEN** 輸出末尾有「版本待補 3 個元件（2 個資產），未納入比對」
