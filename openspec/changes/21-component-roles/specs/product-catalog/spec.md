## ADDED Requirements

### Requirement: Component role declaration
每個目錄產品 SHALL 可宣告一個元件角色 `role`，可用值與中文名稱為：`firmware` 韌體、`bios` BIOS、`bmc` BMC、`os` 作業系統、`dbms` 資料庫管理系統、`web` 網站服務系統、`tool` 應用工具軟體、`security` 資通安全系統、`suite` 一般套裝軟體、`app` 應用程式元件、`other` 其他。未宣告角色的產品 SHALL 視為「其他」。`role` 的值不在上述清單時，系統 SHALL 拒絕載入目錄，錯誤訊息 SHALL 指出寫了該值的檔案、產品 key 與可用值。角色 MUST NOT 影響比對：修改角色後命中與判定不變，也 MUST NOT 觸發重新比對。

#### Scenario: Product without a role
- **WHEN** 使用者在 `catalog.d` 新增 key 為 `example-fw` 的產品，未寫 `role`
- **THEN** 目錄載入成功，`example-fw` 的角色為「其他」

#### Scenario: Unknown role value
- **WHEN** `catalog.d\site.toml` 為 `example-fw` 寫 `role = "database"`
- **THEN** 系統拒絕載入目錄，錯誤訊息指出 `site.toml`、產品 `example-fw`、值 `database` 與可用值清單

#### Scenario: Role change does not rematch
- **WHEN** 使用者在 `catalog.d` 把 `freeradius` 的角色改為 `security` 後執行 `cvewatch match`
- **THEN** 系統不因這項目錄變更重新比對全部資產，既有命中與判定不變

### Requirement: Local role overrides
`catalog.d` 的使用者檔 SHALL 可為新產品宣告角色，也 SHALL 可為既有 key 覆寫角色；使用者檔未寫 `role` 時 SHALL 沿用先前來源的角色。多個使用者檔為同一產品寫 `role` 時，SHALL 以依檔名排序後最後載入者為準。寫成 `other` 時 SHALL 把該產品改為「其他」。

#### Scenario: Overriding a seed role
- **WHEN** 種子目錄的 `freeradius` 角色為 `tool`，使用者在 `catalog.d` 為 `freeradius` 寫 `role = "security"`
- **THEN** `freeradius` 的角色為「資通安全系統」，別名、CPE 與 CNA 識別沿用種子目錄

#### Scenario: Override without a role keeps the seed role
- **WHEN** 使用者在 `catalog.d` 只為 `vmware-esxi` 增加別名、未寫 `role`
- **THEN** `vmware-esxi` 的角色仍為「作業系統」

#### Scenario: Role declared for a local product
- **WHEN** 使用者在 `catalog.d` 新增 key 為 `example-bmc`、`role = "bmc"` 的產品
- **THEN** `example-bmc` 的角色為「BMC」

### Requirement: Seed catalog roles
內建種子目錄的每個產品 SHALL 宣告角色，且 MUST NOT 宣告為 `other`。各產品的角色 SHALL 與使用說明的產品清單一致，代表性的指定見各情境。

#### Scenario: Network and storage firmware
- **WHEN** 只載入內建種子目錄
- **THEN** PAN-OS、AOS-CX、Cisco IOS、FortiOS、RUCKUS Unleashed 與 DiskStation Manager 的角色都是「韌體」

#### Scenario: Server components
- **WHEN** 只載入內建種子目錄
- **THEN** ESXi、Proxmox VE、Ubuntu 與 Windows Server 為「作業系統」，iDRAC9 為「BMC」，PowerEdge BIOS 為「BIOS」，vCenter Server 為「應用工具軟體」

#### Scenario: Server software
- **WHEN** 只載入內建種子目錄
- **THEN** MySQL 與 PostgreSQL 為「資料庫管理系統」，nginx 與 Apache Tomcat 為「網站服務系統」，Veeam Backup & Replication 為「一般套裝軟體」，FortiClient 為「資通安全系統」，PHP、Ruby on Rails 與 jQuery 為「應用程式元件」

#### Scenario: Every seed product has a role
- **WHEN** 只載入內建種子目錄
- **THEN** 每個產品都有明確宣告的角色，沒有產品的角色為「其他」

### Requirement: Component role in catalog output
`cvewatch asset catalog list`（以及不帶子命令的 `asset catalog`）SHALL 為每個產品列出角色的值；`cvewatch asset catalog show <key>` SHALL 以「中文名稱（值）」顯示角色，未宣告角色的產品 SHALL 顯示「其他（other）」。

#### Scenario: Role in the product list
- **WHEN** 使用者執行 `cvewatch asset catalog list`
- **THEN** `paloalto-panos` 那一列的角色欄為 `firmware`，`mysql` 那一列為 `dbms`

#### Scenario: Role in product details
- **WHEN** 使用者執行 `cvewatch asset catalog show dell-idrac9`
- **THEN** 輸出包含「角色：BMC（bmc）」

#### Scenario: Local product without a role in details
- **WHEN** 使用者在 `catalog.d` 新增未寫 `role` 的 `example-fw` 後執行 `cvewatch asset catalog show example-fw`
- **THEN** 輸出包含「角色：其他（other）」
