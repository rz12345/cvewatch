## MODIFIED Requirements

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
