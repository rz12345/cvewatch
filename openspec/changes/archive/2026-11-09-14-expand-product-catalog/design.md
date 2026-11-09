# Design

## Context

- 產品目錄由 `crates/core/catalog/seed.toml`（內建）加 `<data_dir>\catalog.d\*.toml`（使用者）合併而成；別名以 `catalog::normalize`（NFKC、小寫、移除空白與 `-_./`）完全比對，衝突別名會使目錄載入失敗。目前有 11 項產品、5 種版本格式（dotted、panos、dsm、esxi、ubuntu）。
- 版本格式有兩層實作：`catalog/scheme.rs` 只驗證寫法（匯入警告），`matching/version/` 負責解析與比較，`Version` 是列舉。
- GUI 的「加入元件」是帶搜尋框的 `egui::ComboBox`，結果為目錄順序的平鋪清單；「未辨識 → 改為…」是沒有搜尋的 `ComboBox`，列出全部產品。搜尋邏輯在 `frontend::asset_form::search_products`（`to_lowercase().contains`，沒有排序、沒有上限）。
- 未辨識產品的相似建議已存在：`Catalog::suggest`（Jaro-Winkler ≥ 0.85，最多 3 個），匯入報表在用。
- 使用者提供的設備盤點與資產清冊放在 repo 根目錄、未納入版控；`.gitignore` 已排除 `/data`（本機資料目錄，含 `catalog.d`）。

## Goals / Non-Goals

**Goals:**
- 版控內容（OpenSpec、種子目錄、測試、fixture、文件）只出現廠牌與產品名稱，不出現使用者的硬體型號、數量、使用中的版本或資產編號
- 種子目錄的識別資料都經過核對：每個 CPE 在 NVD CPE 字典查得到，每組 CNA vendor/product 在 cvelistV5 至少有一筆記錄用了這個寫法
- 產品選擇器的搜尋、排序與上限是純函式，放在 `frontend`，不需 GUI 也能測

**Non-Goals:**
- 不改變比對引擎的判定規則，只新增四種版本格式
- 不新增執行期的外部 API 呼叫或資料來源
- 不清理既有版控內容或 git 歷史中已有的型號（proposal 的 Out of scope）

## Decisions

### D1 版控只放廠牌與產品，型號放本機 `catalog.d`
- 種子目錄的別名只用「廠牌＋產品／產品線」寫法（例如 `hpe aruba networking aos-cx`、`cisco ios software`）。硬體型號對應到韌體產品的別名，寫在使用者資料目錄的 `catalog.d\site.toml`。
- CVE 識別資料逐型號發布的產品（Zyxel 交換器韌體：CPE 與 CNA product 都是 `<型號>_firmware`；CAYIN 媒體播放器：CNA 逐型號列出）不放種子，整項寫在 `site.toml`。程式只提供它們需要的 `zyxel` 版本格式。
- 替代方案：種子放型號別名（原規劃）。比對最方便，但型號會進版控，使用者明確不接受。
- `site.toml` 由任務 3.11 依使用者的來源檔在本機產生，不 commit；使用說明以虛構型號示範寫法。
- 一台硬體對應多個產品時（例如伺服器有 BMC 與 BIOS 兩個元件），使用者分別加元件，不用型號別名。

### D2 新增產品與識別資料
2026-11-09 已初步查證：CPE 以 NVD API `virtualMatchString` 計 CVE 數、`cpes/2.0?keywordSearch` 查字典；CNA 寫法取自本機 cvelistV5 索引（`data/cache/cvelist/search-index.jsonl.zst`）的 `vendor / product` 實際出現次數。數字為當日總數，括號內為 KEV 數。2026-11-09 實作時（任務 3.1–3.8）以 NVD CPE 字典（`cpes/2.0?cpeMatchString=`）與 `virtualMatchString` 逐一核對，補完原標「待補」「待確認」者，結果已寫入下表。

| key | 顯示名稱（廠商） | 格式 | CPE（NVD CVE 數） | CNA vendor / product（cvelistV5 筆數） | 別名重點 |
|---|---|---|---|---|---|
| `hpe-aoscx` | AOS-CX（HPE Aruba Networking） | dotted | o:hpe:arubaos-cx（57）、o:arubanetworks:aos-cx（字典 0 筆，但 12 筆 CVE 的 NVD 設定仍引用，保留） | Hewlett Packard Enterprise (HPE) / AOS-CX（46）、… / HPE Aruba Networking AOS-CX（5）、… / HPE Aruba Networkign AOS-CX（1，原廠拼字錯誤） | aos-cx、arubaos-cx、hpe aruba networking aos-cx、aruba cx |
| `cisco-ios` | Cisco IOS（Cisco） | cisco | o:cisco:ios（620，KEV 39） | Cisco / IOS（39）、Cisco / Cisco IOS（36）、Cisco / Cisco IOS Software（12） | cisco ios、cisco ios software |
| `mikrotik-routeros` | RouterOS（MikroTik） | dotted | o:mikrotik:routeros（87，KEV 5） | MikroTik / RouterOS（17）、Mikrotik / RouterOS（9）、MicroTik / RouterOS（1）、n/a / MikroTik RouterOS（4）、Tenable / MikroTik RouterOS（1）；CNA 多為 VulnCheck、CERT-PL；關鍵字 `MikroTik RouterOS` | mikrotik routeros、routeros |
| `ruckus-unleashed` | RUCKUS Unleashed（RUCKUS Networks） | dotted | o:ruckuswireless:unleashed（10）、o:ruckuswireless:unleashed_firmware（8） | Ruckus Networks / RUCKUS Unleashed、Ruckus / RUCKUS Unleashed（各 1）；AP 逐型號的記錄（Ruckus Networks / RUCKUS <型號>）不列入；關鍵字 `Ruckus Unleashed` | ruckus unleashed |
| `ruckus-smartzone` | RUCKUS SmartZone（RUCKUS Networks） | dotted | o:commscope:ruckus_smartzone_firmware（6，含 KEV CVE-2023-25717）、o:ruckuswireless:smartzone_ap（1） | RUCKUS / SmartZone（5）、CommScope, Inc. / SmartZone（1）；關鍵字 `SmartZone` | ruckus smartzone、smartzone |
| `pixis` | PIXIS IP 管理（PIXIS） | dotted | 無（CPE 字典 0 筆） | 無（0 筆） | pixis；關鍵字 `PIXIS` |
| `qsan-sanos` | SANOS（QSAN） | dotted | a:qsan:sanos（7）；XEVO 為 a:qsan:xevo（7），不列入 | QSAN / SANOS（7），CNA 為 TWCERT，皆 2021 年；關鍵字 `QSAN SANOS` | qsan sanos |
| `veeam-backup-replication` | Veeam Backup & Replication（Veeam） | dotted | a:veeam:veeam_backup_\&_replication（42，KEV 4） | Veeam / Backup and Replication（21）、Veeam / Backup & Replication（12）、Veeam / Backup and Recovery（12，已確認為 VBR：CVE-2024-40711、CVE-2025-23120 等描述皆為 Backup & Replication）；a:veeam:backup_\&_replication 字典有 25 筆但 0 筆 CVE，不列入；關鍵字 `Veeam Backup & Replication` | veeam、veeam b&r、veeam backup and replication |
| `nakivo-backup-replication` | NAKIVO Backup & Replication（NAKIVO） | dotted | a:nakivo:backup_\&_replication_director（2，含 KEV CVE-2024-48248）、…_transporter（1） | NAKIVO / Backup & Replication Director（2）；關鍵字 `NAKIVO Backup` | nakivo、nakivo backup and replication |
| `freeradius` | FreeRADIUS（FreeRADIUS） | dotted | a:freeradius:freeradius（48） | n/a / freeradius（3）、freeradius / freeradius（1）；關鍵字 `FreeRADIUS` | freeradius |
| `nginx` | nginx（F5） | dotted | a:f5:nginx（41）、a:f5:nginx_open_source（26）、a:nginx:nginx（2） | F5 / NGINX Open Source（35）；索引中查無 n/a / Nginx Web Server（n/a 只有 NGINX Controller，不列入） | nginx、nginx open source、f5 nginx |
| `mariadb` | MariaDB（MariaDB） | dotted | a:mariadb:mariadb（431） | MariaDB / server（27）、MariaDB / MariaDB（10）、MariaDB Foundation / MariaDB Server（1） | mariadb、mariadb server |
| `ruby` | Ruby（ruby-lang.org） | dotted | a:ruby-lang:ruby（113） | Ruby / Ruby（5）、ruby-lang / Ruby（1）；隨附 gem（ruby-lang / CGI、URI，ruby / rexml）版本獨立，不列入（以 SBOM 處理）；關鍵字 `as distributed in Ruby`、`discovered in Ruby`、`in Ruby before`、`in Ruby through`（n/a 記錄約 40 筆，多數 NVD 已涵蓋） | ruby、cruby、ruby mri（不收單獨的 mri） |
| `rails` | Ruby on Rails（Rails） | dotted | a:rubyonrails:rails（122）、a:rubyonrails:ruby_on_rails（50）、a:rubyonrails:actionview（1）；其他元件 gem（activestorage、actionpack、activesupport、activerecord…，含底線寫法）NVD 字典皆 0 筆 | rails / rails（14）、rails / activestorage（6）、actionview（3）、activesupport（5）、actionpack（1）、Rails / Action Pack（1）、rubyonrails / Active Storage（1）；rails-html-sanitizer 等版本獨立的 gem 不列入 | rails、ruby on rails、ror |
| `ckeditor4` | CKEditor 4（CKSource） | dotted | a:ckeditor:ckeditor（24；ckeditor4 為 0） | ckeditor / ckeditor4（12）；排除 ckeditor5；關鍵字 `CKEditor 4`、`CKEditor4` | ckeditor4、ckeditor 4 |
| `bootstrap` | Bootstrap（Bootstrap） | dotted | a:getbootstrap:bootstrap（7） | Bootstrap / Bootstrap（2） | bootstrap、twitter bootstrap |
| `jquery` | jQuery（OpenJS Foundation） | dotted | a:jquery:jquery（11，KEV 1） | jquery / jQuery（2）、n/a / jquery（1）；排除 jquery-ui 等 | jquery |
| `momentjs` | Moment.js（Moment） | dotted | a:momentjs:moment（4） | moment / moment（3）；關鍵字 `Moment.js` | moment、moment.js、momentjs |

只寫在本機 `site.toml` 的產品（版控中只記廠牌與識別方式，不記型號）：
- Zyxel 交換器韌體：版本格式 `zyxel`；CPE 為 `o:zyxel:<型號>_firmware`、CNA 為 `Zyxel / <型號> firmware`，依使用者的型號列出
- CAYIN 媒體播放器：CNA 為 `CAYIN Technology / <型號>`，依使用者的型號列出；關鍵字 `Cayin Signage Media Player`

查證時的發現：
- 許多舊 CVE 的 CNA 寫成 `n/a / n/a`（MITRE 指派），例如 jQuery CVE-2019-11358、Bootstrap CVE-2019-8331、Ruby CVE-2024-27281／27282，只能靠 NVD CPE 比對。以 `virtualMatchString` 指定 Ruby 3.x 版本查詢時，NVD 沒有回傳 CVE-2024-27282，但其描述寫明「Ruby 3.x through 3.3.0」，表示 NVD 的設定沒有涵蓋或尚未補上。`ruby` 產品因此需要關鍵字（如 `as distributed in Ruby`），任務 2.7 要驗證這兩筆 CVE 會以「僅關鍵字相符」出現。
- Rails 的 CVE 多半記在元件 gem（activestorage、actionpack、activesupport…），不在 `rails` 本身。`rails` 產品要列出各元件 gem 的 CNA product 與 CPE，否則 activestorage 的 CVE-2025-24293（CRITICAL）等會漏掉。
- 「Veeam / Backup and Recovery」可能是 Veeam Backup for Microsoft 365 等其他產品的寫法，任務 2.2 確認後再決定是否列入。

#### 同類別常見品牌（49 項）
由使用者環境的品牌類別向外延伸，依本機 cvelistV5 的 CNA 筆數與 CISA KEV（`known_exploited_vulnerabilities.json`，2026-11-09 下載）篩選。KEV 為 CISA 清單中該產品的筆數。原標「待補」者已於任務 3.4–3.8 查 NVD CPE 字典補完（括號內為 `virtualMatchString` 的 CVE 數）。

| key | 顯示名稱（廠商） | 格式 | CPE（NVD CVE 數） | CNA vendor / product（筆數）、KEV |
|---|---|---|---|---|
| `fortinet-fortios` | FortiOS（Fortinet） | dotted | o:fortinet:fortios（279） | Fortinet / FortiOS（179）、Fortinet / Fortinet FortiOS（39）；KEV 32 |
| `fortinet-forticlient` | FortiClient（Fortinet） | dotted | a:fortinet:forticlient（86） | 已確認 Fortinet / FortiClientWindows（30）、FortiClientMac（15）、FortiClientLinux（7），以及 Fortinet FortiClientWindows／Mac／Linux、Fortinet FortiClient for Windows、FortiClient for Mac OS 等舊寫法（EMS、iOS 不列入）；KEV 3 |
| `cisco-asa` | Cisco ASA（Cisco） | cisco | a:cisco:adaptive_security_appliance_software（37） | Cisco / Cisco Adaptive Security Appliance (ASA) Software（159）、Cisco / Cisco Secure Firewall Adaptive Security Appliance (ASA) Software（48）、Cisco / Cisco Adaptive Security Appliance Software（2）；CNA 版本寫成點分 `9.8.2.14`；與 FTD 合計 KEV 14 |
| `cisco-ftd` | Cisco Secure Firewall Threat Defense（Cisco） | dotted | a:cisco:secure_firewall_threat_defense（277）、a:cisco:firepower_threat_defense（3） | Cisco / Cisco Firepower Threat Defense Software（160）、Cisco / Cisco Secure Firewall Threat Defense (FTD) Software（81）、Cisco / Cisco Firepower Threat Defense (FTD) Software（8） |
| `checkpoint-gaia` | Check Point Gaia（Check Point） | numeric | o:checkpoint:gaia_os（9） | checkpoint / Quantum Security Gateway（10）；KEV 1 |
| `sonicwall-sonicos` | SonicOS（SonicWall） | numeric | o:sonicwall:sonicos（68） | SonicWall / SonicOS（75）；KEV 4 |
| `sophos-firewall` | Sophos Firewall（Sophos） | dotted | o:sophos:sfos（8）、o:sophos:xg_firewall_firmware（9）、a:sophos:firewall（2）；a:sophos:sfos 為 0 | Sophos / Sophos Firewall（21）；KEV 3 |
| `watchguard-fireware` | Fireware OS（WatchGuard） | dotted | o:watchguard:fireware（75） | WatchGuard / Fireware OS（91） |
| `ivanti-connect-secure` | Ivanti Connect Secure（Ivanti） | junos | a:ivanti:connect_secure（133） | Ivanti / Connect Secure（65）、n/a / Pulse Connect Secure（21）；25.x 起版本改為點分 `25.1.1.3`；KEV 19 |
| `f5-bigip` | F5 BIG-IP（F5） | dotted | a:f5:big-ip_ 加 local_traffic_manager（541）、access_policy_manager（630）、advanced_firewall_manager（552）、application_security_manager（580）、advanced_web_application_firewall（195）、domain_name_system（469）、global_traffic_manager（490）、link_controller（525）、policy_enforcement_manager（533）、application_acceleration_manager（524）、analytics（511）、fraud_protection_service（405）、carrier-grade_nat（109）、ddos_hybrid_defender（166）、ssl_orchestrator（147）、webaccelerator（297）、edge_gateway（293） | F5 / BIG-IP（259）、n/a / BIG-IP（110）、F5 / BIG-IP APM（12）、n/a / BIG-IP APM（19）；KEV 8 |
| `citrix-netscaler` | NetScaler ADC（Cloud Software Group） | numeric | a:citrix:netscaler_application_delivery_controller（34）、a:citrix:netscaler_gateway（41）、a:citrix:application_delivery_controller（4）、a:citrix:gateway（19） | NetScaler / ADC（19）、NetScaler / Gateway（18）、Citrix NetScaler / ADC、Gateway（各 8）、Citrix / NetScaler Gateway（4）、Cloud Software Group / NetScaler ADC（3）等 10 種寫法；KEV 16 |
| `cisco-iosxe` | Cisco IOS XE（Cisco） | dotted | o:cisco:ios_xe（560） | Cisco / Cisco IOS XE Software（248）；KEV 26 |
| `cisco-nxos` | Cisco NX-OS（Cisco） | cisco | o:cisco:nx-os（275） | Cisco / Cisco NX-OS Software（102）；KEV 1 |
| `juniper-junos` | Junos OS（Juniper Networks） | junos | o:juniper:junos（794） | Juniper Networks / Junos OS（821）；KEV 7 |
| `hpe-aos-switch` | AOS-Switch（HPE Aruba Networking） | dotted | o:hpe:arubaos-switch（3）；o:arubanetworks:arubaos-switch、aos-switch、aos-s 皆為 0 | Hewlett Packard Enterprise (HPE) / AOS-Switch (AOS-S)（9）、Hewlett Packard Enterprise / ArubaOS-Switch（3）、HPE / ArubaOS-S Switch（1） |
| `arista-eos` | Arista EOS（Arista Networks） | dotted | o:arista:eos（46） | Arista Networks / EOS（77）；KEV 4 |
| `hpe-arubaos` | ArubaOS（HPE Aruba Networking） | dotted | o:arubanetworks:arubaos（233）、o:arubanetworks:instant（32）、o:hpe:arubaos（1） | Hewlett Packard Enterprise (HPE) / ArubaOS (AOS)（105）、… / AOS-8 Instant and AOS-10 AP（18）、… / Aruba Access Points running InstantOS and ArubaOS 10（16）等 7 種寫法；Instant On 為另一產品，不列入 |
| `ubiquiti-unifi-network` | UniFi Network（Ubiquiti） | dotted | a:ui:unifi_network_application（9） | Ubiquiti Inc / UniFi Network Application（13）；KEV 6 |
| `qnap-qts` | QTS（QNAP） | numeric | o:qnap:qts（320） | QNAP Systems Inc. / QTS（252）；與 QuTS hero 合計 KEV 12 |
| `qnap-quts-hero` | QuTS hero（QNAP） | numeric | o:qnap:quts_hero（247） | QNAP Systems Inc. / QuTS hero（235） |
| `asustor-adm` | ADM（ASUSTOR） | dotted | o:asustor:data_master（45） | ASUSTOR / ADM（23）、ASUSTOR Inc. / ADM（11） |
| `truenas` | TrueNAS（iXsystems） | dotted | o:ixsystems:truenas_firmware（3）；o:ixsystems:truenas、truenas_core、truenas_scale 皆為 0 | iXsystems / TrueNAS CORE（2）、TrueNAS SCALE（1） |
| `netapp-ontap` | ONTAP（NetApp） | dotted | a:netapp:ontap_9（8）、a:netapp:clustered_data_ontap（145）；7-Mode 的 a:netapp:data_ontap 不列入 | NetApp / ONTAP 9（4）、NetApp / Clustered Data ONTAP（4）、n/a / Clustered Data ONTAP（15） |
| `acronis-cyber-protect` | Acronis Cyber Protect（Acronis） | numeric | a:acronis:cyber_protect（86） | Acronis / Acronis Cyber Protect 15、16、17、Cloud Agent（Home Office 為個人版，不列入）；版本只寫 build（`unspecified < 39938`）；KEV 2 |
| `commvault` | Commvault（Commvault） | dotted | a:commvault:commvault（22） | Commvault / Commvault Cloud、CommCell；KEV 2 |
| `veritas-netbackup` | NetBackup（Veritas） | dotted | a:veritas:netbackup（66） | Veritas / NetBackup（1）；其餘靠 NVD（Cohesity 接手後的 NetBackup Flex 等不列入） |
| `veritas-backup-exec` | Backup Exec（Veritas） | dotted | a:veritas:backup_exec（8） | 已確認 CNA 記錄沒有此產品，只靠 NVD；KEV 3 |
| `apache-httpd` | Apache HTTP Server（Apache） | dotted | a:apache:http_server（371） | Apache Software Foundation / Apache HTTP Server（139）；KEV 4 |
| `apache-tomcat` | Apache Tomcat（Apache） | dotted | a:apache:tomcat（276） | Apache Software Foundation / Apache Tomcat（141）；KEV 6 |
| `mysql` | MySQL（Oracle） | dotted | a:oracle:mysql（1328） | Oracle Corporation / MySQL Server（1117） |
| `postgresql` | PostgreSQL（PostgreSQL） | dotted | a:postgresql:postgresql（209） | n/a / PostgreSQL（63）、n/a / postgresql（16） |
| `mssql-server` | Microsoft SQL Server（Microsoft） | dotted | a:microsoft:sql_server_2016（100）、2017（155）、2019（207）、2022（181）、2025（86） | Microsoft / Microsoft SQL Server 20xx (GDR／CU)；KEV 2 |
| `mongodb` | MongoDB Server（MongoDB） | dotted | a:mongodb:mongodb（188） | MongoDB / MongoDB Server（94）、MongoDB Inc / MongoDB Server（41）；KEV 2 |
| `redis` | Redis（Redis） | dotted | a:redis:redis（46） | redis / redis（45）；KEV 1 |
| `php` | PHP（PHP Group） | dotted | a:php:php（760） | PHP Group / PHP（103）；KEV 10 |
| `python` | Python（Python Software Foundation） | dotted | a:python:python（145） | Python Software Foundation / CPython（80） |
| `nodejs` | Node.js（OpenJS Foundation） | dotted | a:nodejs:node.js（189） | NodeJS / Node（75）、nodejs / node（54）；KEV 1 |
| `java-se` | Java SE（Oracle） | dotted | a:oracle:jdk（809）、a:oracle:jre（817）、a:oracle:java_se（6） | Oracle Corporation / Oracle Java SE（106）、Oracle Corporation / Java SE JDK and JRE（102）、Oracle / Java SE（14）；Java SE Embedded 不列入；KEV 9 |
| `django` | Django（Django Software Foundation） | dotted | a:djangoproject:django（157） | djangoproject / Django（50） |
| `laravel` | Laravel（Laravel） | dotted | a:laravel:framework（9） | laravel / framework（8）；KEV 3 |
| `wordpress` | WordPress（WordPress） | dotted | a:wordpress:wordpress（585） | WordPress / WordPress（18）、WordPress Foundation / WordPress（8）；KEV 8（只收核心，不含外掛） |
| `drupal` | Drupal（Drupal） | dotted | a:drupal:drupal（734） | Drupal / Drupal core（52）；KEV 5 |
| `tinymce` | TinyMCE（Tiny） | dotted | a:tiny:tinymce（16） | tinymce / tinymce（14） |
| `lodash` | Lodash（OpenJS Foundation） | dotted | a:lodash:lodash（10） | lodash / lodash（3） |
| `jquery-ui` | jQuery UI（OpenJS Foundation） | dotted | a:jqueryui:jquery_ui（7） | jquery / jquery-ui（4） |
| `openssl` | OpenSSL（OpenSSL） | dotted | a:openssl:openssl（321） | OpenSSL / OpenSSL（141）；KEV 1 |
| `openssh` | OpenSSH（OpenBSD） | dotted | a:openbsd:openssh（146） | OpenBSD / OpenSSH（32） |
| `samba` | Samba（Samba） | dotted | a:samba:samba（216） | n/a / samba（25）、n/a / Samba（15）；KEV 1 |
| `bind9` | BIND 9（ISC） | dotted | a:isc:bind（193） | ISC / BIND 9（87）、ISC / BIND9（23）；KEV 1 |

- 同一廠商的多條產品線（Cisco ASA 與 FTD、QNAP QTS 與 QuTS hero）版本線不同，各自成一項，比照 D3。
- 只用 CPE／CNA，不設關鍵字（產品名稱多為常見英文字，關鍵字容易誤中）。
- Apache HTTP Server、OpenSSL、OpenSSH、Samba、BIND、PHP、Python、PostgreSQL、MySQL、Redis、Node.js 在 Linux 上多由發行版套件安裝，同上方的發行版版本說明：這類主機建議改用 `asset import-packages`。

既有產品補別名（只加廠牌＋產品寫法）：`vmware-esxi` 加 vmware vsphere、vsphere；`ubuntu` 加 ubuntu server（已有的不重複加）。

- 關鍵字只取不會誤中的完整字詞（例如 `Moment.js` 而不是 `moment`、`Bootstrap` 改用 CPE/CNA 不設關鍵字），避免英文描述大量誤命中。
- 函式庫以目錄產品處理（使用者決定）。日後若同一主機也匯入含這些套件的 SBOM，同一 CVE 會由兩個元件各命中一次；在使用說明註明擇一使用。
- 發行版套件安裝的 nginx、MariaDB、Ruby（例如 Ubuntu 的 `1.24.0-2ubuntu7`）版本含發行版修補，以上游版本比對會誤判；使用說明建議這類主機改用 `asset import-packages`，目錄產品用於自行編譯或官方套件。

### D3 RUCKUS 分成 Unleashed 與 SmartZone 兩項
兩者版本線不同（Unleashed 200.x、SmartZone 6.x/7.x），放在同一產品會把不同版本線互相比較。AP 型號由使用者在 `site.toml` 加為對應管理模式產品的別名。

### D4 `cisco` 版本格式
解析 `主.次(rebuild[字母])train[數字]`：IOS `15.2(7)E8` → (15, 2, 7, "", "e", 8)、ASA `9.18(4)22` → (9, 18, 4, "", "", 22)、NX-OS `10.2(3t)` → (10, 2, 3, "t", "", 0)，大小寫不拘、允許前綴 `Version`。比較：主次版本與 train 都相同時才依 rebuild、rebuild 字母、train 數字排序，否則 `cmp` 回傳 None（未知）。沒有 train 的 `12.4(25)` 視為 train 為空的獨立系列。
- 替代方案：只做字串相等。Cisco 的 CNA 記錄多半逐版列舉，相等就夠用；但 NVD CPE 與 `lessThan` 的區間仍會出現，同 train 排序成本低，所以一起做。
- 實作時的調整（2026-11-09）：
  - 等值判斷（`covers`）在版本線不同時回傳「不相等」而不是未知：Cisco 的 CNA 記錄逐版列舉上百個跨 train 的版本，若一律未知，所有 Cisco CVE 都會變成 probable 命中；版本線不同的兩個版本確定不相等，不違反「無法比較時為未知」。區間（`lessThan` 等）仍需排序，跨 train 時維持未知
  - ASA 的 CNA 記錄改用點分寫法（CVE-2023-20269 的 `9.8.2.14`），`cisco` 另接受 `主.次.rebuild[.interim]`，視同 `主.次(rebuild)interim`（train 為空），讓清冊照設備畫面寫的 `9.8(2)14` 能與 CNA 比較

### D4b `junos` 版本格式
解析 `主.次 類型 數字[.數字][-S數字]`：Junos `21.4R3-S5` → (21, 4, "r", 3, 0, 5)、Ivanti `22.7R2.4` → (22, 7, "r", 2, 4, 0)；只寫 `21.4` 視為該版本線的起點。依主、次、release 數字、小數、S 數字依序比較，跨主次版本也可比較（Juniper 的 CNA 區間以 `22.2` 起、`22.2R3-S3` 止）。類型不是 `R` 的特殊版本（`X`、`F`、`D`）只與相同類型比較，否則為 None。
- 替代方案：沿用 dotted。`R`、`-S` 會被當成尾碼字串比較，`S10` 會排在 `S5` 之前，因此不採用。
- 實作時的調整：Juniper CNA 的服務版本也有小數（`23.2R2-S7.7`），一併解析；Ivanti 25.x 起改用點分寫法（CVE-2026-118527 的 `25.1.1.3`），`junos` 另接受 `主.次.release[.小數]`，視同 `主.次R release.小數`，與 22.x 的 `R` 寫法可互相比較
- code review 後的調整：NVD CPE 把版本拆成 version 與 update 兩欄（`junos:21.4:r3-s5`、`connect_secure:22.7:r2.4`），合成版本字串時 `junos` 直接相接（`21.4r3-s5`），不加 `-`；update 為 `*` 時（`junos:21.4:*`）以 `same_line` 判斷，只比對基準有寫的部分（`21.4` 涵蓋 21.4 的所有 release、`21.4R3` 涵蓋 `21.4R3-S5`），避免預設的完全相等造成 NVD 判定為不受影響

### D4c `numeric` 版本格式
可有一個字母前綴（`R81.20`、`h5.1.7`），其後為以 `.`、`-`、`_` 或 ` build `（不分大小寫）分隔的數字段，逐段依數值比較、缺段補 0。適用 SonicOS `7.0.1-5145`、NetScaler `14.1-29.72`、QNAP `5.1.7.2770 build 20240520`、Check Point `R81.20`、Acronis `15 build 41800`。
- 前綴字母只用於辨識，不參與比較；前綴不同（例如 `h` 與無前綴）時 `cmp` 回傳 None。
- Sophos `21.0 MR1` 等含「MR」的寫法無法解析，維持 dotted 並產生版本警告（判定為未知），記在 Risks。
- 實作時的調整：開頭的 `v` 視為版本標記而非前綴（`v7.0.1` 與 `7.0.1` 相同）；結尾的 `.x`（QNAP CNA 的 `h5.1.x`）表示版本線起點；`_` 與 `version_text::canonical` 一致視為空白分隔。Acronis 的 CNA 記錄只寫 build 編號（`unspecified < 39938`），清冊也只填 build，寫在種子註解與使用說明
- code review 後的調整：結尾 `.x` 改為記住「整條版本線」：排序時仍為版本線起點（下界），等值與 `≤` 上界時涵蓋以它開頭的所有版本（`version: 5.1.x` 涵蓋 `5.1.7.2770 build 20240520`）；NVD update 為 `*` 時較短的版本涵蓋以它開頭的版本；NVD 的 `qts:5.1.7.2770:build_20240520` 合成為 `5.1.7.2770 build_20240520`（以空白相接，`_` 解析時視為空白）。逐段補 0 比較與 dotted 共用 `matching::version::cmp_padded`

### D5 `zyxel` 版本格式
解析 `V主.次(型號代碼.修補)C數字`，例如 `V2.80(ABCD.1)C0` → (2, 80, "abcd", 1, 0)，`V` 可省略。型號代碼不同時 `cmp` 回傳 None。測試與文件只用虛構代碼（`ABCD`、`WXYZ`）。
- 型號代碼由使用者照設備畫面填寫；種子不維護「型號 → 代碼」對照。
- 與 `cisco` 相同，型號代碼不同時等值判斷回傳「不相等」，區間比較為未知。
- 四種格式的文法放在 `version_text::vendor`，目錄的寫法驗證（`catalog::scheme`）與比較器（`matching::version`）共用，避免清冊接受的寫法在比對時無法解析；寫法驗證改以 `version_text::canonical` 判斷（`_` 視為空白），存入資料庫的 `version_norm` 不變

### D6 搜尋、排序與上限放在 `frontend`
`search_products(catalog, query)` 改為回傳排序後的完整結果；新增 `PICKER_LIMIT = 12` 與一個回傳「前 N 筆＋其餘筆數」的函式供 GUI 使用。
- 比對：query 與每個候選字串（顯示名稱、別名、廠商、key）都先經 `catalog::normalize`，所以 `aoscx` 會中 `AOS-CX`、`ckeditor 4` 會中 `CKEditor4`。
- 排序分兩級：顯示名稱或任一別名以 query 開頭者為第一級，其餘為第二級；同級依顯示名稱（不分大小寫）排序。空 query 全部屬第一級。
- 替代方案：模糊比對（Jaro-Winkler）。使用者輸入時看到的是逐字縮小的清單，部分字串比對較可預期；模糊比對只用在未辨識建議（沿用 `Catalog::suggest`）。
- 實作時的決定：搜尋、排序、上限與未辨識模式的候選組合放在新模組 `frontend::product_search`（`asset_form.rs` 已逾 700 行），`asset_form` 重新匯出 `search_products`、`ProductChoice`；`product_choices` 因此改為依顯示名稱排序（原為目錄順序）。廠商只參與中間相符（第二級）。
- spec 情境「Picking with the keyboard」原寫輸入 `cisco` 後 Enter 加入 Cisco IOS，但依「同級依顯示名稱排序」第一筆是 Cisco ASA；改為輸入 `cisco ios`（gui-console spec delta 同步修改），排序規則不變。

### D7 GUI 共用選擇器
新增 `crates/gui/src/dialogs/product_picker.rs`：一個按鈕＋`egui::Popup`（取代 `ComboBox`），狀態為 `{ open, search, highlighted }`，以 egui id 區分「加入元件」與每筆未辨識產品。
- 鍵盤：搜尋框有焦點時以 `ui.input_mut(|i| i.consume_key(...))` 取用 ↑、↓、Enter、Esc，避免被 TextEdit 處理；搜尋字改變時把反白歸零。
- 未辨識模式：清單頂端固定列「保留」，接著是 `Catalog::suggest(product_raw)` 的建議（標示「建議」），再接搜尋結果（已在建議中的不重複列出）。
- 回傳 `Option<Picked>`（`Keep` 或 `Product(key)`），呼叫端各自套用；`AssetEdit` 的 `search`／`picker_open` 移入選擇器狀態。
- 替代方案：繼續用 `ComboBox` 只加搜尋框。`ComboBox` 的內容區無法可靠攔截方向鍵，也沒有反白狀態，所以不採用。
- 實作時的決定：`AssetEdit` 的 `search`／`picker_open` 改為 `add_picker: PickerState`，每筆未辨識產品的選擇器狀態存在 `unresolved_pickers`（以 `asset_unresolved.id` 為鍵）；選擇器按鈕以 ComboBox 角色與目前值（例如「改為 DiskStation Manager」）提供給無障礙介面，與原本的 ComboBox 一致。未辨識模式的建議固定列在搜尋結果之上（不隨搜尋字過濾），12 筆上限只套用在搜尋結果；popup 以點外部關閉（點選擇器內的搜尋框不關閉）。
- code review 後的調整：清單只在搜尋字改變時重新計算（快取在 `PickerState`，egui 每次重繪不再重跑搜尋與相似建議）；↑↓ 移動時捲動到反白項目；未辨識模式中搜尋相符的產品都已列在建議時不顯示「沒有相符的產品」（`PickerList::footer`）；「改為…」按鈕恢復原本 ComboBox 的提示「改為目錄產品（沿用原版本字串）」。按鈕參數集中為 `PickerButton`。

### D8 commit 前敏感字詞檢查
新增 `scripts/check-sensitive.ps1`：讀取 `data\sensitive-patterns.txt`（每行一個字詞或正規表示式，`/data` 已被 gitignore），比對 `git diff --cached` 新增的行，有命中時列出檔名與行號並以非 0 結束。腳本本身不含任何字詞；清單不存在時提示建立方式並以非 0 結束，避免誤以為檢查通過。
- 可由使用者自行掛到 `.git/hooks/pre-commit`（本機，不入版控）；`/opsx-auto` 的 commit 步驟前也執行一次。
- 替代方案：把字詞清單放在版控中的設定檔。清單本身就是型號，等於公開，因此不採用。
- 實作時的決定：清單每行一項，一般字詞逐字比對、`re:` 開頭為 .NET 正規表示式、`#` 開頭為註解，一律不分大小寫；結束碼 `0` 通過、`1` 有命中、`2` 設定問題（清單不存在、為空、規則無效或不在 repo 內）。腳本含中文訊息，存成 UTF-8 BOM 以便 Windows PowerShell 5.1 正確解析。diff 依 hunk 標頭的行數追蹤（code review 後的調整），hunk 內以 `+++ `／`--- ` 開頭的新增或刪除內容不會被誤認為檔頭。
- 寫法驗證（`catalog::scheme`）對 dotted、panos、dsm、esxi、ubuntu 仍各自維護正規表示式，與比較器可能分歧；改為直接以比較器驗證需要重構 ESXi build 對照表的載入順序，不在本 change 範圍（新增的四種格式已共用 `version_text::vendor` 文法）。

### D9 種子目錄的其他實作決定（2026-11-09）
- 關鍵字只設在 CNA 覆蓋差或描述常用全名的產品：PIXIS、RouterOS（`MikroTik RouterOS`）、RUCKUS Unleashed、SmartZone、QSAN SANOS、Veeam、NAKIVO、FreeRADIUS、Ruby（4 個片語）、CKEditor 4、Moment.js；其餘只用 CPE／CNA。僅關鍵字相符只在沒有任何 CPE／CNA 判定時進入等待區，不建立命中
- CNA vendor 為 `n/a` 的寫法只在產品名稱可明確對應時列入（例如 `n/a / BIG-IP`、`n/a / Clustered Data ONTAP`、`n/a / Pulse Connect Secure`）；`n/a / https://github.com/rails/rails` 等網址寫法不列入
- 廠商名稱帶公司型態字樣者只寫主名（`Ubiquiti`、`QNAP Systems`、`Oracle`、`MongoDB`、`CommScope`）：記錄端以 `normalize_vendor` 移除 `Inc.`、`Corporation` 等後比對
- 既有測試 `user_file_overrides_fields_and_adds_products` 以 `fortinet-fortios` 示範「新增產品」，種子已有此 key，改用虛構的 `example-fw`
- 代表性 CVE fixture 依既有慣例精簡（`versions` 前 7 筆、references 前 2 筆、移除 credits 與 `adp`），來源列在 `tests/fixtures/ingest/README.md`

## Risks / Trade-offs

- [CPE／CNA 寫法查證錯誤，導致永遠比對不到] → 任務要求逐項附查證記錄（CVE 編號或 CPE 名稱），並以 `tests/fixtures` 加入代表性 CVE 記錄樣本做比對測試
- [型號別名只在本機，換電腦或重建資料目錄後失效] → 使用說明提醒備份 `catalog.d`；未辨識時 GUI 的「改為…」仍可手動對應
- [fixture 取自真實 CVE 記錄，可能含使用者的型號] → fixture 只選不含使用者型號的記錄，並經 D8 檢查
- [產品名稱常見（Python、Redis、Samba、BIND），別名誤中清冊其他寫法] → 別名只收「廠牌＋產品」或產品全名，不收單字縮寫（例如 `bind9`、`isc bind`，不收 `bind`）
- [部分產品版本寫法不在支援格式內（Sophos MR、Check Point Jumbo Take）] → 匯入時給版本警告、判定為未知，不判定為不受影響；使用說明列出建議寫法
- [目錄 78 項後 GUI 與 `asset catalog` 輸出變長] → 本 change 的選擇器搜尋與上限；`asset catalog` 輸出維持依顯示名稱排序
- [新關鍵字造成大量「僅關鍵字相符」] → 只對沒有 CPE/CNA 的產品（PIXIS）或必要時設關鍵字；以現有 cvelistV5 fixture 跑一次比對確認沒有新增誤命中
- [12 筆上限讓既有 GUI 測試點不到目標產品] → 既有測試改為先輸入搜尋字再選；新增測試涵蓋上限提示
- [別名與使用者 `catalog.d` 衝突，升級後目錄載入失敗] → 衝突時的錯誤訊息已指出別名與兩個 key；在使用說明的升級注意事項列出本版新增的別名
- [目錄 fingerprint 改變，下次 match 全量重算] → 預期行為，只影響第一次比對時間
- [CNA 記錄的版本寫法不在支援範圍：GitHub 式 `>= 1.12.0, < 3.5.0`（jQuery、Rails、MariaDB、TinyMCE、Laravel、Redis 等 GitHub CNA）、SonicWall 的 `7.3.3-7015 and older versions`、PHP 的 `8.2.*`、MySQL 的 `8.4.0-8.4.12`、NetScaler 的 `14.1-73.46 FIPS`、F5 的 `Hotfix-BIGIP-…-ENG`] → 判定為未知（probable 命中），不會判定為不受影響；多數同時有 NVD CPE 可判定。改善 CNA 版本字串的解析不在本 change 範圍，列為後續 change 的候選
- [Veeam 的 CNA 上界寫 `12.1.2`，實際 build 為 `12.1.2.172`] → dotted 下 `12.1.2.172` 不在 `≤ 12.1.2` 內；NVD CPE 的上界為 `12.1.2.172`，兩者矛盾時以「來源矛盾」呈現，不會靜默漏報
- [Microsoft SQL Server 的 CNA 產品名稱逐 CU 不同（`Microsoft SQL Server 2019 (CU 32)`）] → 種子只列 GDR 與常見寫法，其餘靠 NVD CPE

## Migration Plan

不需資料庫 migration。sync／啟動時 `sync_catalog` 會把新增產品寫入資料庫；先前未辨識的寫法要在本機 `site.toml` 加別名後重新匯入清冊（或在 GUI 以「改為…」處理）才會變成元件。回復舊版時，新產品的元件會因目錄找不到 key 而顯示為未知產品（與刪除 `catalog.d` 條目的行為相同）。

## Open Questions

以下皆依建議做法處理（2026-11-09，`/opsx-auto`），記錄於此：
- 使用者的 QSAN 儲存設備執行 SANOS 或 XEVO：採 SANOS（設備屬 XCubeSAN 系列，執行 SANOS；QSAN 的 2021 年 CVE 同時列出 SANOS 與 XEVO）。若設備畫面顯示 XEVO，在 `catalog.d` 為該設備另宣告 a:qsan:xevo 與 QSAN / XEVO
- 驗證主機的軟體：採 FreeRADIUS（本機 `site.toml` 把 eduroam 驗證主機的寫法設為 FreeRADIUS 別名）；若實際為 Windows NPS，該主機改用既有的 `windows-server`
- 使用者的 MikroTik 交換器執行 RouterOS 或 SwOS：採 RouterOS（`site.toml` 註明改用 SwOS 時的處理）
- RUCKUS AP 由 Unleashed 還是 SmartZone 管理：兩者都建；盤點中沒有 SmartZone 控制器，`site.toml` 預設把 AP 型號設為 Unleashed 別名，並註明改為 SmartZone 的方式

## 架構圖

不需更新（2026-11-09 判斷）：workspace members、crate 間的 `path =` 相依與執行檔都沒有變動；沒有新增或改換外部系統（NVD、cvelistV5 等來源與協定不變，CPE／CNA 查證只在實作時進行，不是執行期呼叫）。變更都在 core 內部模組（版本格式、種子目錄）、frontend 的搜尋模組與 GUI 對話框；`scripts/check-sensitive.ps1` 是本機開發工具，不屬於系統架構。
