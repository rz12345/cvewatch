## ADDED Requirements

### Requirement: ISMS import keeps identity fields
從 ISMS 清冊匯入時，更新的既有資產的廠牌、型號、序號、財產編號、主機名稱、管理介面位址與 IP MUST NOT 改變；新增的資產 SHALL 不填這些欄位、沒有 IP。ISMS 匯入的預覽與報表 MUST NOT 因此列出這些欄位的變更，也 MUST NOT 給出缺欄或缺少 `ips` 工作表的提醒。

#### Scenario: Updating an asset that has identity data
- **WHEN** `NET-HW-001` 的廠牌為「Aruba」、財產編號為 `P-0001`、IP 為 `192.0.2.1`，使用者從 ISMS 清冊匯入含 `NET-HW-001` 的列
- **THEN** `NET-HW-001` 的 ISMS 欄位依清冊更新，廠牌、財產編號與 IP 不變，報表沒有這些欄位的變更或提醒

#### Scenario: New asset from the ISMS workbook
- **WHEN** ISMS 清冊有一列新的 `SYS-HW-007`
- **THEN** 新資產 `SYS-HW-007` 沒有廠牌、型號、序號、財產編號、主機名稱、管理介面位址與 IP
