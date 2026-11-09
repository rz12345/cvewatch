## ADDED Requirements

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
