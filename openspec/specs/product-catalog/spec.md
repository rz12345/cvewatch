# product-catalog Specification

## Purpose

把清冊中各種產品寫法正規化為單一產品識別，並提供比對所需的 CPE、CNA vendor/product 與版本格式，讓資產能對應到 CVE 資料。

## Requirements

### Requirement: Alias resolution
系統 SHALL 將產品名稱以 NFKC、小寫並移除空白與 `-_./` 後，與目錄別名完全比對來決定產品。

#### Scenario: Different spellings of the same product
- **WHEN** 清冊中分別出現「PAN-OS」「Palo Alto PAN-OS」「panos」
- **THEN** 三者都對應到產品 `paloalto-panos`

### Requirement: Unresolved products are reported
無法辨識的產品名稱 SHALL 被標為未辨識，匯入報表 SHALL 列出該名稱與最多 3 個相似別名建議；未辨識的列 MUST NOT 建立元件。

#### Scenario: Typo in product name
- **WHEN** 清冊填寫「Synolgy DSM」
- **THEN** 報表列出未辨識「Synolgy DSM」並建議 `synology-dsm`，該列不產生元件

#### Scenario: Listing unresolved names
- **WHEN** 使用者執行 `cvewatch asset catalog --unresolved`
- **THEN** 系統列出所有目前未辨識的產品名稱及出現的資產

### Requirement: Product identifiers for matching
每個目錄產品 SHALL 可定義零到多組 CPE（part、vendor、product）、零到多組 CNA vendor/product 寫法、零到多組 CSAF 公告中的 vendor/product 名稱，以及零到多個描述關鍵字（`keywords`）。關鍵字用於 CVE 沒有可辨識的 affected 或 CPE 時，以英文描述判斷是否可能相關。

#### Scenario: Product with CNA identifiers only
- **WHEN** 某產品沒有 CPE 但有 CNA vendor/product
- **THEN** 目錄載入成功，`asset catalog show` 顯示其 CPE 為空、CNA 識別一筆

#### Scenario: Product keywords
- **WHEN** 使用者在 `catalog.d` 為 `proxmox-ve` 設定 `keywords = ["Proxmox Virtual Environment"]`
- **THEN** `asset catalog show proxmox-ve` 列出該關鍵字，且未寫 `keywords` 的使用者條目沿用種子目錄的關鍵字

### Requirement: Version scheme declaration
每個目錄產品 SHALL 宣告版本格式；匯入時版本字串無法以該格式解析者 SHALL 產生警告但仍保存原始字串。

#### Scenario: Unparseable PAN-OS version
- **WHEN** PAN-OS 元件版本填寫 `11.1 latest`
- **THEN** 匯入成功，報表對該列給出「版本無法解析」警告

### Requirement: User catalog extensions
系統 SHALL 載入內建種子目錄後，再載入資料目錄 `catalog.d` 下的 TOML 檔；相同 `key` 的產品 SHALL 以使用者檔中有寫的欄位覆寫、未寫的欄位沿用，`aliases` SHALL 與既有別名合併；新 `key` SHALL 新增。

#### Scenario: Adding an alias locally
- **WHEN** 使用者在 `catalog.d\local.toml` 為 `vmware-esxi` 增加別名 `xyz-200 esxi`
- **THEN** 重匯後清冊中的「XYZ-200 ESXi」被辨識為 `vmware-esxi`

#### Scenario: Conflicting alias
- **WHEN** 兩個不同產品宣告相同別名
- **THEN** 系統拒絕載入目錄，錯誤訊息指出衝突的別名與兩個產品 key

### Requirement: Seed catalog coverage
內建種子目錄 SHALL 涵蓋網通（防火牆／VPN、交換器／路由、無線）、儲存、備份、網站與應用伺服器、資料庫、執行環境／框架／CMS、前端函式庫與常用系統元件各類的常見產品，產品清單見各情境。每項產品 SHALL 至少有一組 CPE、CNA 識別或關鍵字，沒有任何 CVE 識別資料的產品 SHALL 以關鍵字比對。`cvewatch asset catalog` 的輸出 SHALL 與使用說明的產品清單一致。

#### Scenario: Network firmware and appliances
- **WHEN** 使用者執行 `cvewatch asset catalog`
- **THEN** 清單包含 PAN-OS、AOS-CX、HPE ArubaOS、HPE AOS-Switch、Cisco IOS、Cisco IOS XE、Cisco NX-OS、Cisco ASA、Cisco Secure Firewall Threat Defense、Juniper Junos OS、Arista EOS、MikroTik RouterOS、Fortinet FortiOS、FortiClient、SonicWall SonicOS、Sophos Firewall、WatchGuard Fireware、Check Point Gaia、Ivanti Connect Secure、F5 BIG-IP、NetScaler ADC、RUCKUS Unleashed、RUCKUS SmartZone、UniFi Network、PIXIS

#### Scenario: Storage and backup products
- **WHEN** 使用者執行 `cvewatch asset catalog`
- **THEN** 清單包含 DiskStation Manager、QNAP QTS、QNAP QuTS hero、ASUSTOR ADM、TrueNAS、NetApp ONTAP、QSAN SANOS、Veeam Backup & Replication、NAKIVO Backup & Replication、Acronis Cyber Protect、Commvault、Veritas NetBackup、Veritas Backup Exec

#### Scenario: Server software and libraries
- **WHEN** 使用者執行 `cvewatch asset catalog`
- **THEN** 清單包含 nginx、Apache HTTP Server、Apache Tomcat、MariaDB、MySQL、PostgreSQL、Microsoft SQL Server、MongoDB、Redis、Ruby、Ruby on Rails、PHP、Python、Node.js、Java SE、Django、Laravel、WordPress、Drupal、CKEditor 4、TinyMCE、Bootstrap、jQuery、jQuery UI、Lodash、Moment.js、OpenSSL、OpenSSH、Samba、BIND 9、FreeRADIUS

#### Scenario: Brand and product spellings
- **WHEN** 清冊元件分別填寫「HPE Aruba Networking AOS-CX」「Cisco IOS XE Software」「Juniper Junos」「FortiGate FortiOS」「QNAP QTS」「Apache httpd」「Microsoft SQL Server」「moment.js」「Ckeditor4」「Ruby on Rails」
- **THEN** 依序辨識為 AOS-CX、Cisco IOS XE、Juniper Junos OS、Fortinet FortiOS、QNAP QTS、Apache HTTP Server、Microsoft SQL Server、Moment.js、CKEditor 4、Ruby on Rails 產品

#### Scenario: Product with keywords only
- **WHEN** 使用者執行 `cvewatch asset catalog show` 查看 PIXIS 產品
- **THEN** 顯示其 CPE 與 CNA 識別為空、關鍵字至少一筆

### Requirement: Seed catalog excludes hardware models
種子目錄的別名 SHALL 只由廠牌、產品或產品線名稱組成，MUST NOT 包含硬體型號；CVE 識別資料逐型號發布的產品 MUST NOT 列入種子目錄。硬體型號對應到韌體產品的別名，以及逐型號的產品，SHALL 由使用者在 `catalog.d` 宣告。

#### Scenario: Model spelling without a local alias
- **WHEN** 清冊元件填寫「Aruba XYZ-100」，且 `catalog.d` 沒有對應的別名
- **THEN** 該列列為未辨識產品，不建立元件

#### Scenario: Model alias declared locally
- **WHEN** 使用者在 `catalog.d` 為 `hpe-aoscx` 加入別名「Aruba XYZ-100」後重新匯入
- **THEN** 清冊中的「Aruba XYZ-100」辨識為 AOS-CX

#### Scenario: Per-model product declared locally
- **WHEN** 使用者在 `catalog.d` 新增 key 為 `zyxel-switch`、版本格式為 `zyxel`、含兩組型號 CPE 的產品
- **THEN** 目錄載入成功，`asset catalog show zyxel-switch` 列出兩組 CPE 與 `zyxel` 版本格式

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
