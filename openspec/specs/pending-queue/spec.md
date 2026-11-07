# pending-queue Specification

## Purpose

追蹤可能與資產有關、但目前資料不足以判定的 CVE，在每次同步後重新評估，直到資料補齊或由使用者人工決定。

## Requirements

### Requirement: Entering the pending queue
可能相關但沒有任何來源可判定版本的 CVE，以及僅以描述關鍵字被保留的 CVE，SHALL 進入等待區並記錄原因與進入時間。

#### Scenario: Newly published CVE without details
- **WHEN** 一筆新 CVE 的 CNA affected 只寫 Proxmox VE 而無版本，NVD 狀態為未分析
- **THEN** 該 CVE 進入等待區，原因為缺少版本資料

#### Scenario: Vendor still investigating
- **WHEN** 原廠 CSAF 對元件版本標示 under_investigation，其他來源皆無法判定
- **THEN** 該 CVE 進入等待區，原因為原廠調查中

#### Scenario: Keyword-only CVE
- **WHEN** 一筆 CVE 僅因描述包含 `DiskStation Manager` 被保留
- **THEN** 該 CVE 進入等待區，原因為僅關鍵字相符

### Requirement: Re-evaluation after each sync
每次 sync 結束後，系統 SHALL 重新評估等待區中的每筆 CVE；資料足以判定時 SHALL 移出等待區，並依判定建立或不建立命中，同時記錄移出原因。

#### Scenario: Details published later
- **WHEN** 三天後 NVD 補上 CPE 與版本區間，且元件版本在區間內
- **THEN** 該 CVE 移出等待區並建立 confirmed 命中

#### Scenario: Details show not affected
- **WHEN** 補上的資料顯示元件版本不受影響
- **THEN** 該 CVE 移出等待區且不建立命中，移出原因記錄為「資料補齊，不受影響」

#### Scenario: Details still inconclusive
- **WHEN** 補上的資料有版本區間，但元件版本不在任何區間內且沒有 defaultStatus
- **THEN** 該 CVE 移出等待區，保留 probable 命中，移出原因記錄為「已有版本資料但無法判定」

#### Scenario: Related product temporarily missing from catalog
- **WHEN** 等待區中僅關鍵字相符的 CVE 所屬產品暫時從產品目錄移除（catalog.d 設定錯誤），重新評估時找不到該產品
- **THEN** 該 CVE 維持在等待區（原因、進入時間與逾期判斷不變），與該產品無關的 CVE 照常重新評估

### Requirement: Stale pending items
在等待區超過設定天數（預設 60 天）的 CVE SHALL 被標為逾期。

#### Scenario: Pending for 61 days
- **WHEN** 一筆 CVE 已在等待區 61 天
- **THEN** 該 CVE 被標為逾期，`cvewatch pending list --stale` 會列出它

### Requirement: Manual resolution of pending items
使用者 SHALL 能將等待區 CVE 手動移出，並選擇對指定元件建立 probable 命中，或標記為不相關。

#### Scenario: User decides it applies
- **WHEN** 使用者對等待區中的 CVE 選擇「套用到 PVE-01」
- **THEN** 該 CVE 移出等待區，對 PVE-01 建立判定來源為 manual 的 probable 命中

#### Scenario: User applies a CVE that already has a finding
- **WHEN** 使用者對等待區中的 CVE 選擇「套用到 PVE-01」，而 PVE-01 的該命中已為已處理
- **THEN** 該命中重新開啟並記錄使用者事件

#### Scenario: User dismisses a pending CVE
- **WHEN** 使用者將等待區中的 CVE 標記為不相關，且系統已對 PVE-01 建立未處理的 probable 命中
- **THEN** 該 CVE 移出等待區且之後不再自動放回，PVE-01 的 probable 命中轉為忽略（原因 false_positive）並記錄使用者事件

#### Scenario: New component after dismissal
- **WHEN** 已標記為不相關的 CVE 之後對新加入的 PVE-09 產生 probable 命中
- **THEN** 該命中直接為忽略（原因 false_positive），時間軸記錄系統事件說明原因

#### Scenario: KEV listing while pending
- **WHEN** 等待區中的 CVE 被列入 KEV
- **THEN** 該 CVE 在等待區清單置頂並標示 KEV
