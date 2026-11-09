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

### Requirement: endoflife.date product declaration
產品 SHALL 可宣告 endoflife.date 的產品代稱（只含小寫英數字與 `-`）；格式不符時 SHALL 拒絕載入並指出產品 key。`catalog.d` SHALL 可為既有產品新增或改寫代稱，寫成空字串時 SHALL 取消該產品的 endoflife.date 查詢。`cvewatch asset catalog show <key>` SHALL 顯示產品的代稱與目錄宣告的版本分支 EoL。

#### Scenario: Declaring a slug locally
- **WHEN** 使用者在 `catalog.d\local.toml` 為 `nginx` 宣告代稱 `nginx`
- **THEN** 下次 `cvewatch sync --source eol` 在有 nginx 元件時查詢 endoflife.date 的 `nginx`

#### Scenario: Disabling a seed slug
- **WHEN** 種子目錄為 `proxmox-ve` 宣告了代稱，使用者在 `catalog.d` 將其設為空字串
- **THEN** `eol` 來源不再查詢該產品，Proxmox VE 元件只使用目錄宣告的 EoL 或手動覆寫

#### Scenario: Malformed slug
- **WHEN** `catalog.d` 為某產品宣告代稱 `Proxmox VE`
- **THEN** 系統拒絕載入目錄並指出該產品 key 的代稱格式不符

### Requirement: Catalog software end-of-life dates
產品 SHALL 可宣告版本分支的 EoL：每筆有分支名稱、EoL 日期（`YYYY-MM-DD`）與來源網址（`http` 或 `https`）；缺欄或格式不符時 SHALL 拒絕載入並指出產品 key 與分支。`catalog.d` 中相同產品、相同分支的宣告 SHALL 覆寫先前的宣告，新分支 SHALL 加入。同一產品同時有目錄宣告與 endoflife.date 資料時，符合目錄宣告分支的元件 SHALL 採用目錄宣告。

#### Scenario: Adding a cycle locally
- **WHEN** 種子目錄為 Synology DSM 宣告分支 `7.1` 與 `7.2`，使用者在 `catalog.d` 加入分支 `7.3` 的 EoL 與來源網址
- **THEN** DSM 有三個分支的 EoL，`7.3.x` 的元件採用使用者宣告的日期

#### Scenario: Correcting a seed date
- **WHEN** 使用者在 `catalog.d` 為 DSM 分支 `7.1` 宣告不同的 EoL 日期與來源
- **THEN** `7.1.x` 的元件採用使用者宣告的日期與來源

#### Scenario: Missing source URL
- **WHEN** `catalog.d` 為某產品宣告分支 EoL 但沒有來源網址
- **THEN** 系統拒絕載入目錄並指出產品 key 與分支缺少來源網址

### Requirement: Hardware model lifecycle catalog
目錄 SHALL 可宣告硬體型號清單：每筆有廠牌、型號、選填的型號別名與廠牌別名、EoL 日期與來源網址（規則同 Catalog software end-of-life dates）。相同廠牌與型號的宣告 SHALL 以後載入者為準；同一個型號寫法（含別名）在同一廠牌下屬於兩個不同型號時 SHALL 拒絕載入並指出兩個型號。種子目錄 MUST NOT 包含任何硬體型號，硬體型號 SHALL 只由使用者在 `catalog.d` 宣告。

#### Scenario: Declaring a model locally
- **WHEN** 使用者在 `catalog.d` 宣告廠牌 `Aruba`、型號 `XYZ-100`、EoL 2026-10-31 與來源網址
- **THEN** 廠牌與型號相符的資產採用此 EoL

#### Scenario: Conflicting model alias
- **WHEN** `catalog.d` 的 `Aruba XYZ-100` 與 `Aruba XYZ-200` 都宣告別名 `XYZ-1X0`
- **THEN** 系統拒絕載入目錄，錯誤訊息指出別名與兩個型號

#### Scenario: Seed has no models
- **WHEN** 沒有任何 `catalog.d` 檔案時載入目錄
- **THEN** 硬體型號清單為空，所有資產的硬體 EoL 顯示「目錄沒有此型號」或「未填廠牌或型號」

### Requirement: Seed end-of-life coverage
種子目錄 SHALL 為 endoflife.date 涵蓋的種子產品宣告代稱，並 SHALL 為 endoflife.date 未涵蓋的 Synology DSM 宣告依原廠公告整理的版本分支 EoL 與來源網址；其他未涵蓋的產品（例如 EoL 依伺服器世代訂定的 Dell iDRAC9）不在種子中宣告 EoL。種子中的每個代稱 SHALL 在 endoflife.date 存在，每筆 EoL 宣告 SHALL 有可公開存取的原廠來源網址。

#### Scenario: Seed products with slugs
- **WHEN** 使用者執行 `cvewatch asset catalog show paloalto-panos`
- **THEN** 輸出顯示 endoflife.date 代稱 `panos`

#### Scenario: Seed product with catalog dates
- **WHEN** 使用者執行 `cvewatch asset catalog show synology-dsm`
- **THEN** 輸出列出 DSM 的版本分支、EoL 日期與原廠來源網址，沒有 endoflife.date 代稱
