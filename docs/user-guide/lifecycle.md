# 資產生命週期（EoL 與到期提醒）

cvewatch 除了比對 CVE，也判斷資產的軟體版本分支與硬體是否已停止原廠支援（EoL），並追蹤保固、維護合約與軟體授權的到期日。生命週期資料**不影響比對與風險等級**：修改日期、覆寫或同步 EoL 資料都不會觸發重新比對。

## 會判斷哪些項目

| 項目 | 資料來源（依優先順序） |
|---|---|
| 硬體 EoL | 資產的手動覆寫 → 產品目錄的硬體型號清單（以資產的廠牌＋型號對應） |
| 軟體 EoL（每個清冊元件） | 元件的手動覆寫 → 產品目錄宣告的版本分支 EoL → endoflife.date |
| 保固到期 `warranty_end` | 資產欄位 |
| 維護合約到期 `support_end` | 資產欄位 |
| 軟體授權到期 `license_end` | 資產欄位 |

主機套件元件（dpkg／rpm／SBOM）與未辨識產品不判斷 EoL。已退役資產可在詳情查看，但不列入標示、篩選、Dashboard 與摘要。

## 狀態與提醒天數

- **已 EoL**：硬體或任一軟體元件的 EoL 日期已到（含當天），或 endoflife.date 標示已 EoL 但日期未公布
- **即將 EoL**：EoL 日期在提醒天數內
- **已到期／即將到期**：保固、維護合約或軟體授權到期日已到，或在提醒天數內

提醒天數在設定檔調整（EoL 與到期共用一個值，允許 1–3650，預設 90）：

```toml
[lifecycle]
reminder_days = 90
```

沒有資料的項目會顯示原因：「未填」「未填廠牌或型號」「目錄沒有此型號」「無法對應版本分支（版本）」「版本待補，無法判斷 EoL」「無 EoL 資料」「endoflife.date 查無此產品」「尚未取得 EoL 資料」。這些都**不**視為已 EoL。

## 清冊欄位與 `eol` 工作表

`assets` 工作表可加三個日期欄：`warranty_end`、`support_end`、`license_end`。日期寫 `YYYY-MM-DD` 或 `YYYY/MM/DD`（月日可為一位數），也可以是 Excel 的日期儲存格；其他寫法或不存在的日期（例如 `2027-02-30`）會讓匯入中止並指出欄位與原文。範本與匯出的日期欄為文字格式。

手動覆寫寫在選填的 `eol` 工作表（只認標準欄名，不套用 `[import.columns]`）：

| asset_tag | product | eol | source |
|---|---|---|---|
| SW-CORE |  | 2027-06-30 | 原廠延長支援函 |
| SRV-01 | ESXi | 2028-12-31 | 延長支援合約 |

- `product` 空白為**硬體**覆寫；否則為該資產在此清冊中的元件產品（以產品目錄別名解析）
- `eol` 與 `source` 都必填；同一資產的硬體或同一元件只能有一列
- 有 `eol` 工作表時，清冊中各資產的覆寫以檔案為準（工作表沒有的覆寫會移除，預覽列出「將移除」）
- 清冊缺少日期欄、或沒有 `eol` 工作表（CSV 清冊一律沒有）時，既有值保留，匯入報表有提醒
- 元件被移除時其覆寫一併移除；修改版本不影響覆寫

匯出寫出使用中資產的日期與所有覆寫，匯出後立刻重匯為「新增 0、更新 0」。ISMS 清冊匯入不處理生命週期欄位（保留既有值）。

視窗版的資產對話框有「生命週期」區塊可輸入三個日期與硬體覆寫，元件列的「EoL 覆寫…」可設定或清除元件覆寫；TUI 只顯示不編修。

## endoflife.date 同步

`cvewatch sync` 的最後一個來源 `eol` 從 [endoflife.date](https://endoflife.date/) API 取得版本分支與 EoL 日期，也可單獨執行：

```powershell
cvewatch sync --source eol
```

- 只查詢**使用中資產的清冊元件**所屬、且目錄宣告了代稱的產品（版本待補的元件也算）
- 使用條件請求（`If-None-Match`），未變更時只更新取得時間；請求間隔 250 毫秒
- 代稱在 endoflife.date 不存在（404）時不算失敗：該次執行記錄提醒（`cvewatch sync --history` 可看），保留先前資料
- 連線錯誤或重試後仍 429／5xx 時，該代稱保留先前資料，`eol` 來源記為失敗（結束碼 4），其他代稱照常更新
- 設定檔 `[sources] eol_enabled = false` 可停用（`--source eol` 仍會執行）

## 版本分支怎麼對應

版本與分支名稱都以 `.`、`-`、`_`、空白、括號與 `/` 切成段落；分支的段落是版本段落的前綴時即符合，取段數最多者。數字段以數值比較，且只比版本段開頭的數字（`17.9.4a` 對應分支 `17.9`、`8u421` 對應分支 `8`；分支 `1` 不會對到 `10.1`）。沒有符合時，再以分支的「最新版本」比對（Windows Server 的 OS build `10.0.20348.2700` 對應分支 `2022`）。ESXi／vCenter 先換成「主.次」版本，只填 build 時以目錄的 ESXi build 對照表換成名稱。

仍無法唯一決定時標為「無法對應版本分支」，**不猜測**（例如 SQL Server 的分支 `13.0-sp3`）。這時請在 `eol` 工作表或視窗版為該元件設定手動覆寫，或在 `catalog.d` 宣告分支。

## 在 `catalog.d` 宣告 EoL

```toml
# 新增或改寫 endoflife.date 代稱；寫 "" 取消查詢
[[product]]
key = "nginx"
endoflife = "nginx"

# endoflife.date 沒有的產品：自行宣告版本分支（相同分支覆寫內建宣告、新分支加入）
[[product]]
key = "synology-dsm"
eol_cycles = [
  { cycle = "7.3", eol = "2028-10-01", source = "https://kb.synology.com/en-global/WP/Software_Life_Cycle_Policy/1" },
]

# 硬體型號（內建目錄沒有任何型號；以下為虛構範例）
[[hardware]]
vendor = "Aruba"
vendor_aliases = ["HPE Aruba Networking"]
model = "XYZ-100"
aliases = ["XYZ-100F"]
eol = "2026-10-31"
source = "https://vendor.example/eol/xyz-100"
```

- 代稱只能包含小寫英數字與 `-`；分支與硬體的 `eol` 為 `YYYY-MM-DD`、`source` 為 `http`／`https` 網址，缺欄或格式不符時拒絕載入並指出產品、分支或型號
- 硬體型號以廠牌＋型號比對（不分大小寫、全形視同半形、忽略空白與 `-_./`），型號也比對別名；只有型號沒有廠牌的資產不比對
- 相同廠牌＋型號以後載入者為準；同一個寫法屬於兩個型號時拒絕載入
- `cvewatch asset catalog show <key>` 顯示產品的代稱與目錄宣告的分支

## 內建目錄的 EoL 涵蓋範圍

endoflife.date 代稱（2026-11-09 確認皆存在）：

| 目錄產品 | 代稱 |
|---|---|
| `paloalto-panos` | `panos` |
| `cisco-iosxe` | `cisco-ios-xe` |
| `mikrotik-routeros` | `routeros` |
| `fortinet-fortios` | `fortios` |
| `vmware-esxi` | `esxi` |
| `vmware-vcenter` | `vcenter` |
| `proxmox-ve` | `proxmox-ve` |
| `truenas` | `truenas` |
| `netapp-ontap` | `netapp-ontap` |
| `veeam-backup-replication` | `veeam-backup-and-replication` |
| `commvault` | `commvault` |
| `nginx` | `nginx` |
| `apache-httpd` | `apache-http-server` |
| `apache-tomcat` | `tomcat` |
| `mariadb` | `mariadb` |
| `mysql` | `mysql` |
| `postgresql` | `postgresql` |
| `mssql-server` | `mssqlserver` |
| `mongodb` | `mongodb` |
| `redis` | `redis` |
| `ruby` | `ruby` |
| `rails` | `rails` |
| `php` | `php` |
| `python` | `python` |
| `nodejs` | `nodejs` |
| `java-se` | `oracle-jdk` |
| `django` | `django` |
| `laravel` | `laravel` |
| `wordpress` | `wordpress` |
| `drupal` | `drupal` |
| `ckeditor4` | `ckeditor` |
| `bootstrap` | `bootstrap` |
| `jquery` | `jquery` |
| `jquery-ui` | `jquery-ui` |
| `openssl` | `openssl` |
| `samba` | `samba` |
| `bind9` | `bind-9` |
| `debian` | `debian` |
| `ubuntu` | `ubuntu` |
| `rhel` | `rhel` |
| `windows-server` | `windows-server` |

目錄宣告的版本分支 EoL：只有 Synology DSM（`synology-dsm`），依 Synology「Software Life Cycle Policy」與「End-of-Life Announcement for DSM 6.2」整理。原廠公告只寫到月份時取該月 1 日；DSM 7.1 的延伸支援只適用部分機型，內建資料取 End of Maintenance（2024-06），適用延伸支援的機型請以元件覆寫。

| 分支 | EoL |
|---|---|
| 7.4 | 2028-06-01 |
| 7.3 | 2028-10-01 |
| 7.2 | 2025-12-01 |
| 7.1 | 2024-06-01 |
| 7.0 | 2023-06-01 |
| 6.2 | 2024-10-01 |
| 6.1 | 2019-06-01 |
| 6.0 | 2018-06-01 |

其他產品（例如 Dell iDRAC9：其 EoL 依 PowerEdge 世代訂定，改以伺服器機型的硬體 EoL 涵蓋；AOS-CX、Cisco IOS、Junos、QNAP 等）內建目錄不宣告 EoL，需要時請在 `catalog.d` 自行宣告。

## 在哪裡看

- **CLI**：`cvewatch asset show <asset_tag>` 的「生命週期（提醒 N 天）」段落，每項一行，例如「PAN-OS 10.2.9（分支 10.2）EoL 2027-03-01 即將 EoL（剩 50 天），來源 endoflife.date」
- **視窗版**：資產清單名稱後標示「（已 EoL）」「（即將 EoL）」「（已到期）」「（即將到期）」，工具列的「生命週期」下拉選單可篩選；詳情的「生命週期」區塊列出每一項，來源網址可點選開啟；Dashboard 的生命週期數字可點選跳到篩選後的資產清單
- **TUI**：資產表格的「週期」欄，按 `l` 依序切換篩選（全部 → 已 EoL → 即將 EoL → 保固或合約到期）；詳情與 Dashboard 同樣列出
- **每日摘要**：「生命週期提醒」段落列出提醒天數內與摘要期間內到期的項目，以及目前已 EoL／已到期的資產數；期間內進入提醒天數或到期才算新事項
