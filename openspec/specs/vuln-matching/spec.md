# vuln-matching Specification

## Purpose

判定每個資產元件是否受各 CVE 影響，結果必須可解釋（說明依據的來源與版本區間），並在來源矛盾時讓使用者看得到。

## Requirements

### Requirement: Vendor-specific version comparison
系統 SHALL 依產品目錄宣告的版本格式比較版本，並 SHALL 支援 dotted、PAN-OS（含 `-hN` 熱修）、ESXi（Update 名稱與 build 編號）、Synology DSM、Cisco（`15.2(7)E8`、`9.18(4)22`、`10.2(3t)`）、Juniper（`21.4R3-S5`、`22.7R2.4`）、數字分段（`7.0.1-5145`、`14.1-29.72`、`5.1.7.2770 build 20240520`、`R81.20`）與 Zyxel 韌體（`V主.次(型號代碼.修補)C數字`）格式。Cisco 版本 SHALL 只在相同主次版本與 train 內比較，Zyxel 版本 SHALL 只在相同型號代碼內比較；無法比較時結果為未知。

#### Scenario: PAN-OS hotfix ordering
- **WHEN** 比較 `11.1.4-h7` 與 `11.1.4-h12`
- **THEN** `11.1.4-h7` 小於 `11.1.4-h12`

#### Scenario: PAN-OS hotfix versus next maintenance release
- **WHEN** 比較 `11.1.4-h12` 與 `11.1.5`
- **THEN** `11.1.4-h12` 小於 `11.1.5`

#### Scenario: ESXi update naming
- **WHEN** 比較 `8.0 U3c` 與 `8.0 Update 2`
- **THEN** `8.0 U3c` 大於 `8.0 Update 2`

#### Scenario: DSM version without build
- **WHEN** CNA 記錄 `version: 7.2.1, status: affected`，元件版本為 `7.2.1-69057 Update 3`
- **THEN** 判定為受影響（沒寫 build 的版本涵蓋整條 7.2.1 版本線）

#### Scenario: Pre-release suffix in dotted versions
- **WHEN** 比較 `3.0.0-rc2`、`3.0.0-rc10`、`3.0.0` 與 `3.0.0a`
- **THEN** 順序為 `3.0.0-rc2` < `3.0.0-rc10` < `3.0.0` < `3.0.0a`

#### Scenario: Unparseable version
- **WHEN** 元件版本無法以其格式解析
- **THEN** 該元件對任何版本區間的判定結果為未知，不判定為不受影響

#### Scenario: Cisco ordering within a train
- **WHEN** 比較 `15.2(7)E8`、`15.2(7)E10` 與 `15.2(6)E2`
- **THEN** 順序為 `15.2(6)E2` < `15.2(7)E8` < `15.2(7)E10`

#### Scenario: Cisco versions from different trains
- **WHEN** CNA 記錄 `version: 15.2(7)E8, lessThan: 15.2(7)E11, status: affected`，元件版本為 `15.2(4)M3`
- **THEN** 判定結果為未知，不判定為不受影響

#### Scenario: Zyxel firmware ordering
- **WHEN** 比較 `V2.80(ABCD.0)C0`、`V2.80(ABCD.1)C0` 與 `V2.70(ABCD.3)C0`
- **THEN** 順序為 `V2.70(ABCD.3)C0` < `V2.80(ABCD.0)C0` < `V2.80(ABCD.1)C0`

#### Scenario: Zyxel firmware for another model
- **WHEN** 元件版本為 `V2.80(ABCD.1)C0`，CNA 記錄 `lessThanOrEqual: V2.80(WXYZ.1)C0`
- **THEN** 判定結果為未知

#### Scenario: Cisco ASA interim releases
- **WHEN** 比較 `9.18(4)22` 與 `9.18(4)8`
- **THEN** `9.18(4)8` 小於 `9.18(4)22`

#### Scenario: Juniper service releases
- **WHEN** 比較 `21.4R3`、`21.4R3-S5` 與 `21.4R3-S10`
- **THEN** 順序為 `21.4R3` < `21.4R3-S5` < `21.4R3-S10`

#### Scenario: Juniper versions from different lines
- **WHEN** CNA 記錄 `version: 22.2, lessThan: 22.2R3-S3, status: affected`，元件版本為 `21.4R3-S5`
- **THEN** 判定為不在該區間（`21.4` 早於 `22.2`）；與 `lessThan: 21.4R3-S6` 的區間比較時判定為受影響

#### Scenario: Numeric segments with build numbers
- **WHEN** 比較 `14.1-29.72` 與 `14.1-8.50`，以及 `5.1.7.2770 build 20240520` 與 `5.1.7.2770 build 20240315`
- **THEN** `14.1-8.50` 小於 `14.1-29.72`，`build 20240315` 的版本小於 `build 20240520` 的版本

### Requirement: CNA affected range evaluation
系統 SHALL 依 CVE 記錄格式 5.x 的 `versions`、`lessThan`、`lessThanOrEqual`、`changes` 與 `defaultStatus` 語意判定元件版本狀態。

#### Scenario: Version inside lessThan range
- **WHEN** CNA 記錄 `version: 11.1.0, lessThan: 11.1.6, status: affected`，元件版本為 `11.1.4-h7`
- **THEN** 判定為受影響

#### Scenario: Hotfix listed in changes
- **WHEN** 同一區間含 `changes: [{at: 11.1.4-h4, status: unaffected}]`，元件版本為 `11.1.4-h7`
- **THEN** 判定為不受影響

#### Scenario: No matching entry and no default
- **WHEN** 元件版本不在任何 versions 項目內，且未提供 `defaultStatus`
- **THEN** 判定為未知

#### Scenario: Affected without version
- **WHEN** CNA 記錄 `versions: [{version: n/a, status: affected}]`、`defaultStatus: unaffected`
- **THEN** 判定為未知，不採用 `defaultStatus`

#### Scenario: Vendor product-level statement
- **WHEN** 原廠公告對整個產品（沒有版本）標示 known_not_affected，且沒有任何版本列涵蓋元件版本，NVD 判定受影響
- **THEN** 不建立命中，並記錄一筆來源矛盾

### Requirement: NVD configuration evaluation
系統 SHALL 以 NVD 標記為 vulnerable 的 CPE 條件與其版本區間判定元件；AND 設定中非 vulnerable 的平台條件 SHALL 被忽略並記錄於證據。

#### Scenario: Any update in CPE
- **WHEN** NVD 條件為 `cpe:2.3:o:paloaltonetworks:pan-os:11.1.4:*`，元件版本 `11.1.4-h7`
- **THEN** 判定為受影響（update 為 `*` 涵蓋該版本的所有 hotfix）

#### Scenario: Range with excluding end
- **WHEN** NVD 條件為 `cpe:2.3:a:proxmox:virtual_environment:*`、`versionEndExcluding: 8.2.4`，元件版本 `8.2.2`
- **THEN** 判定為受影響

### Requirement: Evidence precedence
系統 SHALL 以原廠公告、CNA、NVD、ADP 的優先序，取第一個有明確結論的來源作為最終判定，並 SHALL 保存所有來源的判定作為證據。套件元件 SHALL 以 OSV 結果判定。

#### Scenario: Vendor advisory overrides NVD
- **WHEN** 原廠公告判定版本 `11.1.6` 不受影響，NVD 判定受影響
- **THEN** 不建立命中，並記錄一筆來源矛盾

#### Scenario: Only NVD has an answer
- **WHEN** 原廠公告與 CNA 皆為未知，NVD 判定受影響
- **THEN** 建立信心等級為 confirmed 的命中，判定來源為 NVD

### Requirement: Conflicts are visible
最終判定為不受影響但有其他來源判定受影響時，系統 SHALL 保存該矛盾，並 SHALL 可由 `cvewatch finding conflicts` 列出。

#### Scenario: Listing conflicts
- **WHEN** 使用者執行 `cvewatch finding conflicts`
- **THEN** 系統列出元件、CVE、最終判定來源與持相反結論的來源

### Requirement: Confidence levels
系統 SHALL 將版本落在受影響範圍的命中標為 confirmed；產品相符但所有來源都無法判定版本時 SHALL 建立 probable 命中。

#### Scenario: Product matches but no version data
- **WHEN** CNA affected 只列出產品名稱而無版本，NVD 尚未分析
- **THEN** 建立 probable 命中

### Requirement: Matching runs automatically
系統 SHALL 在每次 sync 結束後與每次資產匯入後重新比對所有 active 元件，並 SHALL 提供 `cvewatch match` 手動重算。

#### Scenario: Asset upgraded in spreadsheet
- **WHEN** 使用者重匯 Excel，將 `FW-01` 升級到已修正版本
- **THEN** 匯入結束後比對自動執行，該 CVE 對 `FW-01` 的判定更新為不受影響

#### Scenario: Retired asset
- **WHEN** 資產被標為退役
- **THEN** 其元件不再參與比對，既有命中保留但不再更新

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
