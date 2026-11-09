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
