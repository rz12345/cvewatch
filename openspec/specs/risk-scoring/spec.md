# risk-scoring Specification

## Purpose

依已遭利用狀態、利用機率、嚴重度與資產曝露度，把命中分成 P1–P4 並排序，讓使用者先處理真正緊急的項目。

## Requirements

### Requirement: Priority tiers
系統 SHALL 依下列順序判定命中的基本等級：列入 KEV 為 P1；EPSS 達高門檻或 CVSS 達高門檻為 P2；EPSS 達中門檻或 CVSS 達中門檻為 P3；其餘（含未評分）為 P4。

#### Scenario: KEV listed with low CVSS
- **WHEN** 一筆命中的 CVE 列入 KEV，CVSS 為 5.3
- **THEN** 等級為 P1

#### Scenario: High EPSS without KEV
- **WHEN** CVE 不在 KEV，EPSS 為 0.35，CVSS 為 6.1，預設門檻
- **THEN** 等級為 P2

#### Scenario: Unscored CVE
- **WHEN** CVE 不在 KEV、沒有 EPSS 也沒有 CVSS
- **THEN** 等級為 P4

### Requirement: Exposure adjustment
資產曝露度屬於設定的上調清單（預設 internet、dmz）時，系統 SHALL 將命中等級上調一級，P1 維持不變。

#### Scenario: Internet-facing firewall
- **WHEN** 防火牆曝露度為 internet，命中基本等級為 P3
- **THEN** 最終等級為 P2

### Requirement: Configurable thresholds
EPSS 與 CVSS 的高、中門檻與上調曝露清單 SHALL 可由設定檔調整；變更後下次比對 SHALL 重新計算全部等級。

#### Scenario: Lowering EPSS high threshold
- **WHEN** 使用者將 `epss_high` 改為 0.05 並執行 `cvewatch match`
- **THEN** EPSS 0.07 的命中由 P3 變為 P2

### Requirement: Ordering within a tier
同一等級內，系統 SHALL 依 KEV 加入日期由新到舊、EPSS 由高到低、CVSS 由高到低、資產價值由高到低排序。

#### Scenario: Two P2 findings
- **WHEN** 兩筆 P2 命中的 EPSS 分別為 0.4 與 0.2
- **THEN** EPSS 0.4 的命中排在前面

#### Scenario: Same scores on different assets
- **WHEN** 同一個 CVE 在 `FW-01`（資產價值 4）與 `PC-01`（資產價值 3）各有一筆 P2 命中，KEV、EPSS、CVSS 都相同
- **THEN** `FW-01` 的命中排在前面
