# Spec Delta

## MODIFIED Requirements

### Requirement: Ordering within a tier
同一等級內，系統 SHALL 依 KEV 加入日期由新到舊、EPSS 由高到低、CVSS 由高到低、資產價值由高到低排序。

#### Scenario: Two P2 findings
- **WHEN** 兩筆 P2 命中的 EPSS 分別為 0.4 與 0.2
- **THEN** EPSS 0.4 的命中排在前面

#### Scenario: Same scores on different assets
- **WHEN** 同一個 CVE 在 `FW-01`（資產價值 4）與 `PC-01`（資產價值 3）各有一筆 P2 命中，KEV、EPSS、CVSS 都相同
- **THEN** `FW-01` 的命中排在前面
