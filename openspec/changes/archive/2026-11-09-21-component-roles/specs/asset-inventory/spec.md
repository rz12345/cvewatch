## ADDED Requirements

### Requirement: Component grouping by role
顯示資產的元件時，系統 SHALL 依產品目錄宣告的角色分組。角色 SHALL 只由目錄決定：元件 MUST NOT 記錄或個別覆寫角色，目錄的角色變更 SHALL 在下次載入目錄時反映，不需重新匯入。主機套件元件（SBOM、dpkg、rpm）SHALL 不論目錄另成「套件」組；目錄中已沒有的產品 SHALL 歸入「其他」。組的順序 SHALL 固定為韌體、BIOS、BMC、作業系統、資料庫管理系統、網站服務系統、應用工具軟體、資通安全系統、一般套裝軟體、應用程式元件、其他、套件；沒有元件的組 MUST NOT 顯示，同組內 SHALL 維持原本的元件順序。未辨識產品不是元件，MUST NOT 列入任何組。分組只影響顯示，MUST NOT 改變比對、匯入與匯出。

#### Scenario: Server components in role order
- **WHEN** `SRV-01` 依序有 MySQL、Ubuntu、PowerEdge BIOS、nginx、iDRAC9 五個清冊元件與兩個 dpkg 套件元件
- **THEN** 元件依序分為「BIOS」（PowerEdge BIOS）、「BMC」（iDRAC9）、「作業系統」（Ubuntu）、「資料庫管理系統」（MySQL）、「網站服務系統」（nginx）、「套件」（兩個套件）六組，沒有元件的組不出現

#### Scenario: Role changed locally
- **WHEN** 使用者在 `catalog.d` 把 `nginx` 的角色改為 `tool` 後重新開啟 GUI
- **THEN** `SRV-01` 的 nginx 出現在「應用工具軟體」組，不需重新匯入清冊，元件與命中不變

#### Scenario: Product removed from the catalog
- **WHEN** 資產 `FW-EDGE` 的元件產品 `example-fw` 原本宣告於 `catalog.d`，使用者刪除該條目後重新開啟 GUI
- **THEN** 該元件出現在「其他」組

#### Scenario: Pending component keeps its group
- **WHEN** `FW-EDGE` 的 PAN-OS 元件版本待補
- **THEN** 該元件出現在「韌體」組，並以「版本待補」呈現版本

### Requirement: Asset show groups components by role
`cvewatch asset show <asset_tag>` SHALL 依 Component grouping by role 列出元件：每組先輸出角色中文名稱的標題，再列出該組元件與各元件的版本歷史。未辨識產品 SHALL 在所有組之後列出。

#### Scenario: Grouped output on the command line
- **WHEN** 使用者執行 `cvewatch asset show SRV-01`，`SRV-01` 有 Ubuntu、iDRAC9 兩個清冊元件與一個 dpkg 套件元件
- **THEN** 「元件：」之下依序輸出「BMC」組（iDRAC9）、「作業系統」組（Ubuntu）與「套件」組（該套件），每個元件列在其組標題之下

#### Scenario: Unresolved products after the groups
- **WHEN** `FW-EDGE` 有 PAN-OS 元件，另有一筆未辨識產品「Example Agent」
- **THEN** 輸出先列「韌體」組的 PAN-OS，最後才列未辨識產品「Example Agent」
