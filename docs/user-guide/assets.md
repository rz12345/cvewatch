# 資產清冊

資產以 Excel／CSV 清冊匯入，也可以在視窗版直接新增與修改；**資料庫是目前狀態，清冊是交換格式**。

```powershell
cvewatch asset template assets.xlsx              # 空白範本（assets、components 工作表與範例列，classes 子類別說明）
cvewatch asset import assets.xlsx --dry-run      # 預覽差異（新增、更新、版本變更、清冊中消失），不寫入
cvewatch asset import assets.xlsx                # 匯入並自動比對
cvewatch asset export inventory.xlsx             # 匯出目前使用中的資產（格式與匯入相同）
cvewatch asset rename NAS-01 NAS-11              # 更改資產編號（元件、命中與處理紀錄保留）
```

## 清冊欄位（`assets` 工作表）

| 欄位 | 必填 | 說明 |
|---|---|---|
| `asset_tag` | ✓ | 資產編號（cvewatch 的識別，不分大小寫唯一） |
| `name` | ✓ | 名稱 |
| `category` | ✓ | ISMS 子類別代碼（例如 `HW01`，不分大小寫、全形字亦可）或子類別名稱（例如「伺服器」）；匯出寫代碼 |
| `product`、`version` | ✓ | 第一個元件（其餘元件寫在 `components` 工作表） |
| `exposure` | | `internet`、`dmz`、`internal`、`mgmt`，空白為 `internal` |
| `confidentiality`、`integrity`、`availability` | | 機密性、完整性、可用性（1–4）；空白帶入子類別的範本值。**資產價值 = MAX(C, I, A)**，用於同等級內的風險排序 |
| `isms_id` | | ISMS 資產編號 `系統代碼(3 英數字)-大類(HW/SW)-流水號(3 數字)`，例如 `SYS-HW-001`；大類須與子類別一致，不分大小寫唯一（含已退役資產）；留空表示沒有 ISMS 編號，整欄不存在時保留既有資產的編號 |
| `location`、`owner`、`notes` | | 位置、負責人、備註 |

- `criticality`（舊的重要性 1–3）**已停用**：匯入時忽略，報表給出一次提醒
- 舊版 category 值仍可匯入並轉換（報表逐列提醒）：network → `HW02`、security → `HW03`、virtualization → `HW01`、server → `HW01`、storage → `HW05`、host → `HW01`（host 若是個人電腦請改為 `HW06`）
- 畫面編修只能選已定義的子類別；修改類別不會改變已設定的 C/I/A

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
cvewatch asset catalog                 # 列出全部產品（key、顯示名稱、廠商、版本格式、別名與識別資料筆數）
cvewatch asset catalog show hpe-aoscx  # 單一產品的別名、CPE、CNA 寫法與描述關鍵字
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
```

- 修改後執行 `cvewatch sync --rescan`（以新的識別資料重新篩選 CVE），再重新匯入清冊，或在視窗版以「改為…」對應未辨識產品
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

## 匯出與更名

- `asset export`：每個資產一列 `assets`（第一個元件寫在該列的 product / version），其餘元件與未辨識產品列在 `components`；產品寫目錄的顯示名稱、版本寫原始字串，因此匯出後直接重匯為「新增 0、更新 0」。不含已退役資產與主機套件（套件仍以 `asset import-packages` 管理）；目的檔已存在時不覆蓋；匯出檔未加密，含資產清單，請妥善保管
- `asset rename <舊編號> <新編號>`：只能更名使用中資產；新編號不可與其他資產（含已退役、不分大小寫）重複，只改大小寫可以（之後匯入的清冊須使用新的寫法：與既有編號只差大小寫的列會被拒絕）。元件、版本歷史、命中與處理紀錄都保留，比對結果不變。**系統不記錄舊編號**：更名後，舊清冊與已匯出的 VEX 中的舊編號不會對應到此資產（匯入舊清冊會把舊編號當成新資產，搭配 `--prune` 時更名後的資產會被標為退役；VEX 的判定列為未對應），舊編號可再被新增使用。已送出的通知與既有 Redmine 議題內容保留舊編號，之後的通知與註記使用新編號
- 匯入會以檔案內容**覆寫**資產欄位與清冊元件（檔案中沒有的清冊元件會被移除）。在視窗版修改過資產後，請依「**先匯出、再編輯、再匯入**」的順序，避免舊清冊蓋掉畫面上的修改（更名後也一樣，舊清冊中的舊編號不會對應到更名後的資產）：

```powershell
cvewatch asset export inventory-new.xlsx         # 1. 匯出目前狀態
#                                                  2. 用 Excel 編輯 inventory-new.xlsx
cvewatch asset import inventory-new.xlsx --dry-run   # 3. 確認差異只有剛才的編輯
cvewatch asset import inventory-new.xlsx
```
