# 資產清冊

資產以 Excel／CSV 清冊匯入，也可以在視窗版直接新增與修改；**資料庫是目前狀態，清冊是交換格式**。

```powershell
cvewatch asset template assets.xlsx              # 空白範本（assets、components、systems、ips 工作表與範例列，classes 子類別說明）
cvewatch asset import assets.xlsx --dry-run      # 預覽差異（新增、更新、版本變更、清冊中消失），不寫入
cvewatch asset import assets.xlsx                # 匯入並自動比對
cvewatch asset export inventory.xlsx             # 匯出目前使用中的資產（格式與匯入相同）
cvewatch asset import-isms isms.xlsx --dry-run  # 從 ISMS 資訊資產清冊匯入（見下方；先預覽）
cvewatch asset rename NAS-01 NAS-11              # 更改資產編號（元件、命中與處理紀錄保留）
cvewatch asset system recode NET NWS --dry-run  # 預覽修改資產說明代碼（見下方「資產說明」）
```

## 清冊欄位（`assets` 工作表）

| 欄位 | 必填 | 說明 |
|---|---|---|
| `asset_tag` | ✓ | 資產編號（cvewatch 的識別，不分大小寫唯一） |
| `name` | ✓ | 名稱 |
| `category` | ✓ | ISMS 子類別代碼（例如 `HW01`，不分大小寫、全形字亦可）或子類別名稱（例如「伺服器」）；匯出寫代碼 |
| `product` | ✓ | 第一個元件的產品（其餘元件寫在 `components` 工作表） |
| `version` | | 第一個元件的版本；空白為版本待補（不參與比對，見下方「版本待補」） |
| `exposure` | | `internet`、`dmz`、`internal`、`mgmt`，空白為 `internal` |
| `confidentiality`、`integrity`、`availability` | | 機密性、完整性、可用性（1–4）；空白帶入子類別的範本值。**資產價值 = MAX(C, I, A)**，用於同等級內的風險排序 |
| `isms_id` | | ISMS 資產編號 `系統代碼(3 英數字)-大類(HW/SW)-流水號(3 數字)`，例如 `SYS-HW-001`；大類須與子類別一致，系統代碼須與資產說明一致，不分大小寫唯一（含已退役資產）；留空表示沒有 ISMS 編號 |
| `system` | | 資產說明（ISMS 的系統／服務）：代碼（例如 `SYS`）或名稱，須已存在於資料庫或同一份清冊的 `systems` 工作表；留空時由 ISMS 編號的系統代碼帶出 |
| `responsible_unit`、`custodian`、`user_unit` | | 權責單位、保管人、使用單位 |
| `compliance` | | 法規遵循性（1–4），留空表示未評定。**資通系統防護等級 = MAX(C, I, A, 法規遵循性)**，未評定時不顯示數值 |
| `personal_data` | | 是否含個人資料：`Y`、`N`、`是`、`否`；空白帶入子類別的預設值（內建皆為「否」）；匯出寫 `Y`／`N` |
| `brand`、`model` | | 廠牌（設備的製造商，與產品目錄的「廠商」不一定相同）、型號；自由文字 |
| `serial_number` | | 序號；自由文字，不檢查重複 |
| `property_number` | | 財產編號；不可與其他資產（含已退役、不分大小寫）重複：清冊中兩列相同或與清冊以外的資產相同時整批中止，同一份清冊中兩個資產互換財產編號可以 |
| `hostname` | | 主機名稱；只能包含英數字、`-`、`.` 與 `_`，最長 253 字元，保留大小寫，不檢查重複 |
| `management_url` | | 管理介面位址；須以 `http://` 或 `https://` 開頭並包含主機（例如 `https://192.0.2.1:8443/`），以填寫的寫法儲存。**不可包含帳號或密碼**（例如 `https://admin:pw@…`）：帳號密碼請放在密碼管理工具 |
| `location`、`notes` | | 位置、備註 |

- 識別欄位（`brand` 到 `management_url`）都是選填，前後空白會去除，不影響比對結果
- `isms_id`、`system`、`responsible_unit`、`custodian`、`user_unit`、`compliance`、`personal_data`、`brand`、`model`、`serial_number`、`property_number`、`hostname`、`management_url` **整欄不存在**時保留既有資產的值（報表給出一次提醒），新資產以未填或預設值建立
- `owner`（舊的負責人）**已改名為 `custodian`（保管人）**：仍可匯入（內容視為保管人，報表給出一次提醒）；同一張工作表不可同時有 `owner` 與 `custodian`。`[import.columns]` 中對應到 `owner` 的自訂欄名同樣視為保管人

- `criticality`（舊的重要性 1–3）**已停用**：匯入時忽略，報表給出一次提醒
- 舊版 category 值仍可匯入並轉換（報表逐列提醒）：network → `HW02`、security → `HW03`、virtualization → `HW01`、server → `HW01`、storage → `HW05`、host → `HW01`（host 若是個人電腦請改為 `HW06`）
- 畫面編修只能選已定義的子類別；修改類別不會改變已設定的 C/I/A 與是否含個人資料

## IP（`ips` 工作表）

一個資產可以有多筆 IP，清冊以 `ips` 工作表一列一筆記錄（作法與 `components` 工作表相同）：

| 欄位 | 必填 | 說明 |
|---|---|---|
| `asset_tag` | ✓ | 資產編號，須在同一份清冊的 `assets` 工作表中 |
| `ip` | ✓ | 一個 IPv4 或 IPv6 位址（例如 `192.0.2.10`、`2001:db8::10`）。不接受網段（`192.0.2.0/24`）、位址範圍、zone ID（`fe80::1%eth0`）與 IPv4 各段的前導零 |
| `purpose` | | 用途或備註（例如「管理」） |

- 位址以標準寫法儲存與顯示（IPv6 為小寫壓縮寫法，`2001:DB8:0:0::1` 存為 `2001:db8::1`）；同一資產不可有兩筆相同位址（寫法不同但位址相同也算），不同資產可以相同（HA 的虛擬 IP、NAT 後的私有位址）
- **有 `ips` 工作表時以工作表為準**：清冊中每個資產的 IP 換成工作表中的列（依列序），工作表中沒有列出的資產會**清空 IP**；清冊以外的資產不變。`--dry-run` 把 IP 有變更的資產列為更新（欄位變更列出 `ip`，並列出新舊位址）
- **沒有 `ips` 工作表時保留**：既有資產的 IP 不變，報表給出一次「清冊沒有 ips 工作表，保留資料庫中既有資產的 IP」提醒
- **CSV 清冊**：以 `--ips` 另外指定同樣欄位的 IP 檔（UTF-8），例如 `cvewatch asset import assets.csv --components components.csv --ips ips.csv`；沒有指定 `--ips` 時同上保留既有 IP。Excel 清冊請使用 `ips` 工作表，不能同時指定 `--ips`
- `asset_tag` 不在 `assets` 工作表或 IP 無效時整批中止，錯誤指出「ips 工作表第 N 列」
- `ips` 工作表與 `systems` 工作表一樣只認標準欄名（`asset_tag`、`ip`、`purpose`），不套用 `[import.columns]` 的自訂欄名
- IP 不影響比對結果；從 ISMS 清冊匯入時不會改變 IP

## 資產說明（`systems` 工作表）

資產說明對應 ISMS 清冊的「資產說明」與系統代碼（例如 `SYS` 核心系統）：一個資產說明下有多個資產，資產的 ISMS 編號前三碼必須是它的代碼。

| 欄位 | 必填 | 說明 |
|---|---|---|
| `code` | ✓ | 三碼英數字（不分大小寫、以大寫儲存），建立後不可修改 |
| `name` | ✓ | 名稱，不可與其他資產說明重複 |

- 匯入時，`systems` 工作表的每一列會新增資產說明，或修改同代碼資產說明的名稱（`--dry-run` 會列出）；資料庫中有、但工作表中沒有的資產說明會保留。沒有 `systems` 工作表時資產說明不變
- 匯出的 `systems` 工作表列出所有資產說明（含目前沒有使用中資產者），`assets` 的 `system` 欄寫代碼
- 視窗版資產畫面的「新增資產說明」可新增、編輯（代碼與名稱）與刪除；仍有資產（含已退役）歸屬的資產說明不能刪除
- **修改代碼**（視窗版「編輯」或 `cvewatch asset system recode <舊代碼> <新代碼> [--dry-run]`）：新代碼的規則與新增時相同。代碼改變時，在同一個交易內把歸屬的資產（含已退役）改為歸屬新代碼、ISMS 編號前三碼改為新代碼（`NET-HW-001` → `NWS-HW-001`，其餘寫法保留），資產編號等於舊 ISMS 編號（不分大小寫）的資產一併更名（比照 `asset rename`：元件、命中與處理紀錄保留，比對結果不變）。任何一個新編號已被其他資產（含已退役、不分大小寫）使用時整批拒絕並列出衝突的編號。視窗版在代碼改變且有資產歸屬時先顯示確認；CLI 以 `--dry-run` 預覽。**系統不保留舊代碼**：舊清冊（含 `systems` 工作表）、已匯出的 VEX 與尚未更新的 ISMS 清冊中的舊代碼與舊編號不會對應，請重新匯出清冊並更新 ISMS 清冊
- 欄位格式錯誤（例如法規遵循性 5）在讀取清冊時就回報；資產說明須對照資料庫，因此「資產說明尚未定義」這類錯誤會在欄位錯誤都修正後才列出

## ISMS 子類別

內建 ISMS 資訊資產清冊 v2.1 的實體（HW）與軟體（SW）子類別；人員（PE）、資訊（IF）、服務（SE）不納入 cvewatch。C/I/A 為範本值：

| 代碼 | 名稱 | C/I/A | 代碼 | 名稱 | C/I/A |
|---|---|---|---|---|---|
| HW01 | 伺服器 | 2/3/4 | SW01 | 應用系統 | 無（必填） |
| HW02 | 通訊設備 | 2/3/4 | SW02 | 作業系統 | 2/3/3 |
| HW03 | 資安設備 | 2/3/4 | SW03 | 資料庫管理系統 | 2/3/3 |
| HW04 | 週邊設備 | 2/3/3 | SW04 | 網站服務系統 | 2/3/3 |
| HW05 | 儲存設備 | 2/3/4 | SW05 | 應用工具軟體 | 2/3/3 |
| HW06 | 個人電腦 | 2/3/3 | SW06 | 資通安全系統 | 2/3/3 |
| HW07 | 公共使用電腦 | 2/3/2 | SW07 | 一般套裝軟體 | 2/2/2 |
| HW08 | 日常作業事務機器 | 2/3/3 | | | |
| HW09 | 行動裝置 | 2/3/3 | | | |
| HW10 | 物聯網設備 | 2/2/3 | | | |
| HW11 | 基礎設施 | 3/3/4 | | | |

設定檔可覆寫內建子類別的名稱、描述與範本值，或新增子類別（代碼為 `HW`／`SW` 加兩位數字；新增時 `name` 必填，C/I/A 三項全寫或全不寫）。範本值只影響之後新增或留空匯入的資產，**不會改變既有資產的 C/I/A**；範本的 `classes` 工作表依目前設定列出：

```toml
[asset_classes.HW05]
confidentiality = 3          # 只覆寫有寫的欄位

[asset_classes.HW06]
personal_data = true         # 是否含個人資料的預設值（true／false）

[asset_classes.HW12]
name = "實驗儀器"
description = "示波器、量測儀器等具網路功能的設備"
confidentiality = 2
integrity = 3
availability = 3
```

## 產品目錄

清冊的 `product` 欄以產品目錄辨識：比對前一律忽略大小寫、空白與 `-_./`，只要符合任一產品的顯示名稱或別名即可（例如「HPE Aruba Networking AOS-CX」「Ckeditor4」「moment.js」）。認不得的寫法列為未辨識產品，匯入報表與視窗版會列出相似的產品建議。

```powershell
cvewatch asset catalog                 # 列出全部產品（key、顯示名稱、廠商、版本格式、角色、別名與識別資料筆數）
cvewatch asset catalog show hpe-aoscx  # 單一產品的角色、別名、CPE、CNA 寫法與描述關鍵字
```

內建產品（顯示名稱，依 `asset catalog` 的分類）：

| 類別 | 產品 |
|---|---|
| 網路／資安 | PAN-OS、AOS-CX、AOS-Switch、ArubaOS、Cisco IOS、Cisco IOS XE、Cisco NX-OS、Cisco ASA、Cisco Secure Firewall Threat Defense、Junos OS、Arista EOS、RouterOS、FortiOS、FortiClient、SonicOS、Sophos Firewall、Fireware OS、Check Point Gaia、Ivanti Connect Secure、F5 BIG-IP、NetScaler ADC、RUCKUS Unleashed、RUCKUS SmartZone、UniFi Network、PIXIS IP 管理 |
| 虛擬化／伺服器 | ESXi、vCenter Server、Proxmox VE、iDRAC9、PowerEdge BIOS |
| 儲存 | DiskStation Manager、QTS、QuTS hero、ADM、TrueNAS、ONTAP、SANOS |
| 備份 | Veeam Backup & Replication、NAKIVO Backup & Replication、Acronis Cyber Protect、Commvault、NetBackup、Backup Exec |
| 網站與資料庫 | nginx、Apache HTTP Server、Apache Tomcat、MariaDB、MySQL、PostgreSQL、Microsoft SQL Server、MongoDB Server、Redis |
| 執行環境與框架 | Ruby、Ruby on Rails、PHP、Python、Node.js、Java SE、Django、Laravel、WordPress、Drupal |
| 前端函式庫 | CKEditor 4、TinyMCE、Bootstrap、jQuery、jQuery UI、Lodash、Moment.js |
| 系統元件 | OpenSSL、OpenSSH、Samba、BIND 9、FreeRADIUS |
| 作業系統 | Debian、Ubuntu、Red Hat Enterprise Linux、Windows Server |

- 內建別名只有「廠牌＋產品」寫法，**不含硬體型號**：清冊若寫設備型號（例如「Aruba XYZ-100」），請在 `catalog.d` 為對應的韌體產品加別名（見下節）
- 一台設備有多個元件時（例如伺服器的 BMC 與 BIOS）請分別加元件，不要用型號別名
- 前端函式庫（jQuery、Bootstrap 等）可以用目錄產品，也可以匯入含這些套件的 SBOM，**擇一使用**：兩者都有時同一個 CVE 會由兩個元件各命中一次
- Linux 上由發行版套件安裝的 nginx、MariaDB、Ruby、OpenSSL、OpenSSH、Samba、BIND、PHP、Python、PostgreSQL 等，版本含發行版修補（例如 `1.24.0-2ubuntu7`），以上游版本比對會誤判；這類主機請改用 `cvewatch asset import-packages`，目錄產品用於自行編譯或官方套件

### 版本寫法

依產品的版本格式（`asset catalog` 的 scheme 欄）照設備畫面填寫；無法解析的寫法會在匯入時給版本警告，比對判定為未知（不會判定為不受影響）。

| 格式 | 產品 | 寫法 |
|---|---|---|
| `cisco` | Cisco IOS、NX-OS、ASA | `15.2(7)E8`、`10.2(3t)`、`9.18(4)22`（ASA 也接受 `9.18.4.22`）；只在同一主次版本與 train 內比較 |
| `junos` | Junos OS、Ivanti Connect Secure | `21.4R3-S5`、`22.7R2.4`、`25.1.1.3` |
| `numeric` | SonicOS、NetScaler、QTS、QuTS hero、Check Point Gaia、Acronis | `7.0.1-5145`、`14.1-29.72`、`5.1.7.2770 build 20240520`、`h5.1.7.2770 build 20240520`、`R81.20`；Acronis 只填 build 編號（例如 `39938`，原廠公告只寫 build） |
| `zyxel` | Zyxel 韌體（`catalog.d` 宣告） | `V2.80(ABCD.1)C0`（型號代碼照設備畫面填寫）；只在同一型號代碼內比較 |

Sophos 的 `21.0 MR1`、Check Point 的「Jumbo Hotfix Take」寫法無法比較，請填數字版本（例如 `21.0.1`、`R81.20`）。

### 元件角色

每個目錄產品有一個元件角色，`cvewatch asset show`、視窗版的資產詳情與編輯對話框、TUI 的資產詳情都依角色把元件分組（主機套件另成最後一組「套件」，沒有元件的組不顯示）。角色只影響顯示：不影響比對、匯入與匯出，修改角色也不會重新比對。各產品的角色以 `cvewatch asset catalog list` 的 role 欄查看，`asset catalog show <key>` 顯示「角色：中文名稱（值）」。

| 順序 | 值 | 名稱 | 內建產品 |
|---|---|---|---|
| 1 | `firmware` | 韌體 | PAN-OS、AOS-CX、AOS-Switch、ArubaOS、Cisco IOS、Cisco IOS XE、Cisco NX-OS、Cisco ASA、Cisco Secure Firewall Threat Defense、Junos OS、Arista EOS、RouterOS、FortiOS、SonicOS、Sophos Firewall、Fireware OS、Check Point Gaia、Ivanti Connect Secure、F5 BIG-IP、NetScaler ADC、RUCKUS Unleashed、RUCKUS SmartZone、DiskStation Manager、QTS、QuTS hero、ADM、ONTAP、SANOS |
| 2 | `bios` | BIOS | PowerEdge BIOS |
| 3 | `bmc` | BMC | iDRAC9 |
| 4 | `os` | 作業系統 | ESXi、Proxmox VE、TrueNAS、Debian、Ubuntu、Red Hat Enterprise Linux、Windows Server |
| 5 | `dbms` | 資料庫管理系統 | MariaDB、MySQL、PostgreSQL、Microsoft SQL Server、MongoDB Server、Redis |
| 6 | `web` | 網站服務系統 | nginx、Apache HTTP Server、Apache Tomcat |
| 7 | `tool` | 應用工具軟體 | UniFi Network、vCenter Server、OpenSSH、Samba、BIND 9、FreeRADIUS |
| 8 | `security` | 資通安全系統 | FortiClient、PIXIS IP 管理 |
| 9 | `suite` | 一般套裝軟體 | Veeam Backup & Replication、NAKIVO Backup & Replication、Acronis Cyber Protect、Commvault、NetBackup、Backup Exec |
| 10 | `app` | 應用程式元件 | Ruby、Ruby on Rails、PHP、Python、Node.js、Java SE、Django、Laravel、WordPress、Drupal、CKEditor 4、TinyMCE、Bootstrap、jQuery、jQuery UI、Lodash、Moment.js、OpenSSL |
| 11 | `other` | 其他 | （未宣告角色的產品，以及已從目錄移除的產品） |

作業系統到一般套裝軟體的名稱與 ISMS 子類別 SW02–SW07 相同，但角色是目錄的屬性，不隨設定檔 `[asset_classes]` 改名。角色可在 `catalog.d` 覆寫（見下節）：新產品寫 `role` 宣告角色，未寫時為「其他」；既有產品寫 `role` 即覆寫，未寫時沿用內建角色；多個檔案都寫時以檔名排序最後者為準；寫 `role = "other"` 可改回「其他」。值必須是上表的英文值（小寫、完全相符），寫錯時目錄載入失敗，錯誤訊息會指出檔案、產品 key 與可用值。修改後下次執行命令或開啟視窗即依新角色分組，不需重新匯入清冊。

### 在 `catalog.d` 加型號別名與逐型號產品

在資料目錄的 `catalog.d\` 建立任意 `*.toml`（例如 `site.toml`），依檔名順序載入；既有 key 只寫要增加的欄位，`aliases` 會與內建別名合併。以下為虛構型號示範：

```toml
# 型號 → 韌體產品
[[product]]
key = "hpe-aoscx"
aliases = ["Aruba XYZ-100", "Aruba XYZ-200"]

# CVE 識別資料逐型號發布的產品（例如 Zyxel 交換器韌體：CPE 與 CNA 都是「<型號>_firmware」）
[[product]]
key = "zyxel-switch"
display_name = "Zyxel 交換器韌體"
vendor_display = "Zyxel"
version_scheme = "zyxel"
aliases = ["Zyxel ABC-100"]
cpe = [{ part = "o", vendor = "zyxel", product = "abc-100_firmware" }]
cna = [{ vendor = "Zyxel", product = "ABC-100 firmware" }]
role = "firmware"

# 覆寫內建產品的元件角色（其他欄位沿用內建）
[[product]]
key = "freeradius"
role = "security"

# 本機產品宣告角色（虛構產品）
[[product]]
key = "example-bmc"
display_name = "Example BMC"
vendor_display = "Example"
version_scheme = "dotted"
role = "bmc"
```

- 修改後執行 `cvewatch sync --rescan`（以新的識別資料重新篩選 CVE），再重新匯入清冊，或在視窗版以「改為…」對應未辨識產品
- 只修改 `role` 時不需要 `sync --rescan` 或重新匯入，角色只影響分組顯示
- 回復到不支援 `role` 的舊版前，請先刪除 `catalog.d` 中所有 `role = …` 行：舊版不認得此欄位，會拒絕載入目錄
- `catalog.d` 是本機設定，不在備份的資料庫裡：**換電腦或重建資料目錄前請一併備份 `catalog.d`**
- 升級後若 `catalog.d` 的別名與新版內建別名衝突（同一個寫法屬於兩個產品），目錄會載入失敗，錯誤訊息會指出該別名與兩個產品 key；刪除 `catalog.d` 中重複的別名即可。本版新增的內建產品與別名以 `cvewatch asset catalog` 查看

## 從舊版升級（內建目錄不再收硬體型號）

舊版的內建目錄曾為少數產品收錄設備型號寫法（例如防火牆、伺服器 BIOS、NAS 的機型名稱）；新版只保留「廠牌＋產品」寫法。清冊的產品欄若填的是機型寫法，升級後會變成未辨識產品：

1. 升級前先以 `cvewatch asset import --dry-run <清冊>` 記下目前的結果
2. 升級後再執行一次；「未辨識產品」中出現的機型寫法，在 `catalog.d\site.toml` 為對應的產品加別名（虛構型號示範）：

   ```toml
   [[product]]
   key = "paloalto-panos"
   aliases = ["Palo Alto XYZ-100"]
   ```

3. 再執行 `--dry-run`，確認為「新增 0、更新 0」且沒有未辨識產品後，照常匯入。資料庫中既有的元件在重新匯入前不會改變

`cvewatch vex import` 同樣以目錄別名辨識 VEX 文件中的產品名稱：其他工具產生的 VEX 若以機型寫法稱呼產品，也要在 `catalog.d` 補上同一個別名，否則該筆判定會列為未對應。

## 從舊版升級（類別與重要性改為 ISMS 子類別與 C/I/A）

1. **替換執行檔之前**，先以目前（舊版）的 cvewatch 備份：`cvewatch db backup D:\backup\cvewatch-before-13.db`。新版第一次開啟資料庫時就會轉換（包含新版的 `db backup` 與排程執行的 `sync`），因此以新版執行的備份已是轉換後的內容；轉換後的資料庫無法以舊版開啟，回復請以舊版的 `cvewatch db restore` 還原上述備份並改回舊版執行檔
2. 轉換規則：舊 category 依上方對照表轉為子類別代碼（含已退役資產）；C/I/A 帶入轉換後子類別的**內建**範本值（不套用設定檔的覆寫）；舊重要性附記在備註末尾（「舊重要性：3」）供對照。元件、版本歷史、命中與處理紀錄不變
3. 升級後執行一次 `cvewatch asset export inventory-new.xlsx` 取得新格式清冊，在其中調整類別（例如 host → `HW06`）與 C/I/A，之後以新格式清冊維護。舊清冊仍可匯入，但沒有 C/I/A 欄時會以範本值覆寫畫面上調整過的值（`--dry-run` 的欄位變更會列出）；沒有 `isms_id` 欄時則保留資料庫中既有的 ISMS 編號（報表給出一次提醒）

## 從舊版升級（ISMS 管理欄位與資產說明）

1. 與上一節相同，**替換執行檔之前**先以舊版執行 `cvewatch db backup`
2. 轉換規則：`owner`（負責人）轉為保管人；既有 ISMS 編號中出現的每個系統代碼建立一個資產說明（**名稱暫時等於代碼**，例如「SYS SYS」），有 ISMS 編號的資產（含已退役）歸屬對應的資產說明；權責單位、使用單位、法規遵循性為未填，是否含個人資料為「否」
3. 升級後在視窗版「新增資產說明」對話框以「編輯」把資產說明改為正式名稱，再執行一次 `cvewatch asset export` 取得含 `systems` 工作表與新欄位的清冊

## 從舊版升級（識別欄位與 IP）

1. 升級後既有資產的廠牌、型號、序號、財產編號、主機名稱、管理介面位址都是空白，沒有 IP；可以在視窗版的資產對話框「識別與網路」區塊補上，或執行一次 `cvewatch asset export` 取得含新欄位與 `ips` 工作表的清冊，填好後再匯入
2. 舊清冊仍可匯入：沒有識別欄位時保留既有資產的值，沒有 `ips` 工作表時保留既有 IP（各給出一次提醒）。更早版本的清冊會在同一則提醒列出更多欄位；匯出一次新格式的清冊即可消除提醒
3. 若 `[import.columns]` 把「財產編號」之類的欄名對應到 `asset_tag`，行為不變；想把資產編號與財產編號分開記錄時，改為把該欄名對應到 `property_number`，並另外加上 `asset_tag` 欄
4. 不提供降版：舊版程式會拒絕開啟較新版本的資料庫，要換回舊版請從升級前的備份還原

## 由 ISMS 清冊建立資產

已在機構 ISMS「資訊資產清冊」（v2.0／v2.1 Excel）登錄的資產，可以直接匯入，不必重新輸入資產說明、類別、C/I/A 與管理欄位。ISMS 清冊沒有版本，所以描述中的產品匯入後是**版本待補**的元件（見下方「版本待補」），之後在視窗版的編輯對話框補上版本：

```powershell
cvewatch asset import-isms D:\isms.xlsx --dry-run   # 1. 預覽（ISMS 報表與匯入差異；不寫入資料庫）
cvewatch asset import-isms D:\isms.xlsx             # 2. 匯入並自動比對
#                                                    3. 在視窗版補上版本待補元件的版本
```

視窗版是資產畫面的「從 ISMS 清冊匯入」：先顯示與 `--dry-run` 相同的預覽，按「匯入」才寫入。需要 `cvewatch.exe` 與視窗版在同一資料夾。

- **讀取**：依表頭文字找出表頭列（前 10 列中同時含「資產編號」「資產類別」「資產描述」的列）與各欄，文件資訊列與底部的編碼說明列會略過；同一系統代碼或資產說明垂直合併成一格時，視為每一列都填了該值。兩欄「資產編號」組成 ISMS 編號（`NET-` ＋ `HW-001` → `NET-HW-001`），也就是新資產的 `asset_tag` 與 `isms_id`
- **只匯入 HW、SW 列**：PE、IF、SE 列只計數。ISMS 編號格式錯誤、大類與資產類別不符、子類別未定義、編號重複、C/I/A 或法規遵循性不是 1–4、是否含個人資料不是 Y／N、新資產的資產描述空白（沒有可建立的元件），ISMS 編號已是另一個資產的 asset_tag（請先以 `asset rename` 更改該資產的編號），或對應的既有資產有產品已不在目錄中的元件、有已可由目錄辨識且與既有元件重複的未辨識產品（請先在畫面處理）的列會被略過並列在報表中，不影響其他列。法規遵循性為 `－` 時視為未評定
- **欄位**：資產說明的名稱取該代碼第一個非空的「資產說明」（同一代碼有不同寫法時用第一個，報表警告）；名稱取資產描述第一行（去除 `*2` 這類數量與「Core:」這類標籤）；notes 為「ISMS 描述：」加原文；曝露度為 internal（請依實際情況在畫面修改）。資產價值與防護等級欄不讀。資產說明的名稱已被其他代碼使用時整批中止，資料庫不變
- **元件**：資產描述依換行、頓號、逗號、分號拆成多行，以產品目錄別名比對（開頭的中文字詞如「備份軟體」會去除後再比對一次）。比對到的行成為版本待補的目錄產品元件；比對不到的行以原文成為版本待補的未辨識產品。同一列有兩行比對到同一產品時合併成一個元件——兩處版本不同時，請匯入後在畫面另外新增資產
- **比對不到的描述行**：報表列出原文與相似產品。在 `catalog.d` 為它加別名（見上方「在 `catalog.d` 加型號別名」）後，可在畫面把該未辨識產品改為目錄產品
- **已在 cvewatch 的資產**：ISMS 編號已屬於某個使用中資產時（或 asset_tag 與 ISMS 編號相同、且沒有其他 ISMS 編號），匯入更新該資產的類別、資產說明、C/I/A 與管理欄位（預覽逐欄列出新舊值，例如 `機密性 2 → 3`）；編號、名稱、曝露度、識別欄位（廠牌到管理介面位址）、IP、地點、備註與既有元件（含版本）不變，描述中有、資產還沒有的產品加入為版本待補元件。ISMS 編號屬於已退役資產時該列會被略過，請先恢復該資產
- **報表**：預覽與完成摘要列出新增、更新、版本待補的元件數、依大類略過的列數、略過的列與原因、合併的產品、未辨識的描述行、資產說明，以及資料庫中有、但不在這次匯入結果中的資產
- **不會退役資產**：不在 ISMS 清冊中的資產只列在報表，維持使用中
- 尚未執行 `cvewatch init`（沒有資料庫）時以錯誤結束，請先執行 `cvewatch init`
- 同一份 ISMS 清冊重複匯入時為「新增 0、更新 0」；ISMS 清冊改版後再匯入即可更新 ISMS 欄位

## 版本待補

清冊元件與未辨識產品的版本可以空白，狀態為**版本待補**。版本待補的元件**不參與比對**：不會產生任何命中（也不會產生 probable 命中），所以這段期間該產品**沒有受到監控**。

- **哪裡會提醒**：視窗版與 TUI 的 Dashboard 顯示「版本待補 N 個元件（M 個資產），未納入比對」（視窗版點數字會列出這些資產）；資產清單標示有版本待補元件的資產，詳情顯示「版本待補（未納入比對）」；`cvewatch match`、`cvewatch sync` 結束時與每日摘要 email 也會列出
- **如何補上**：在視窗版的編輯對話框填上版本並儲存，元件即成為一般元件並自動比對（補上版本不會留下版本歷史）。也可以匯出清冊、在 Excel 填上 version 後重匯
- **一般清冊匯入**：`version` 空白的元件列會成為版本待補，不會中止匯入；預覽與完成摘要列出「版本待補 N 個元件」。資料庫中已有版本、但重匯的清冊中該元件 `version` 空白時，元件會**改為版本待補**（預覽列為 `11.1.6 → 待補`，舊版本記入版本歷史，可從 `asset show` 找回後填回）
- **畫面編修**：新增資產與加入元件仍須填版本；已有版本的元件不能清成空白
- **匯出**：版本待補的元件 `version` 寫空白，匯出後直接重匯仍為「新增 0、更新 0」
- 元件改為版本待補前已有的命中與處理紀錄保留，但不再更新；版本填上後的下一次比對沿用原有的命中
- **不支援降版**：舊版程式會把空白版本當成無法解析的版本，產生大量 probable 命中。換回舊版執行檔前，請先補上所有版本待補元件的版本（或刪除這些元件）

## 匯出與更名

- `asset export`：每個資產一列 `assets`（第一個元件寫在該列的 product / version，含識別欄位），其餘元件與未辨識產品列在 `components`，每筆 IP 一列 `ips`（依資產編號與儲存順序）；產品寫目錄的顯示名稱、版本寫原始字串，因此匯出後直接重匯為「新增 0、更新 0」。不含已退役資產與主機套件（套件仍以 `asset import-packages` 管理）；目的檔已存在時不覆蓋；匯出檔未加密，含資產清單，請妥善保管
- `asset rename <舊編號> <新編號>`：只能更名使用中資產；新編號不可與其他資產（含已退役、不分大小寫）重複，只改大小寫可以（之後匯入的清冊須使用新的寫法：與既有編號只差大小寫的列會被拒絕）。元件、版本歷史、命中與處理紀錄都保留，比對結果不變。**系統不記錄舊編號**：更名後，舊清冊與已匯出的 VEX 中的舊編號不會對應到此資產（匯入舊清冊會把舊編號當成新資產，搭配 `--prune` 時更名後的資產會被標為退役；VEX 的判定列為未對應），舊編號可再被新增使用。已送出的通知與既有 Redmine 議題內容保留舊編號，之後的通知與註記使用新編號
- 匯入會以檔案內容**覆寫**資產欄位與清冊元件（檔案中沒有的清冊元件會被移除）。在視窗版修改過資產後，請依「**先匯出、再編輯、再匯入**」的順序，避免舊清冊蓋掉畫面上的修改（更名後也一樣，舊清冊中的舊編號不會對應到更名後的資產）：

```powershell
cvewatch asset export inventory-new.xlsx         # 1. 匯出目前狀態
#                                                  2. 用 Excel 編輯 inventory-new.xlsx
cvewatch asset import inventory-new.xlsx --dry-run   # 3. 確認差異只有剛才的編輯
cvewatch asset import inventory-new.xlsx
```
