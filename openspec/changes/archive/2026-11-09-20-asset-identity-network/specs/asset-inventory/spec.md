## ADDED Requirements

### Requirement: Asset identity fields
每個資產 SHALL 有選填的廠牌、型號、序號、財產編號、主機名稱與管理介面位址。這些欄位 SHALL 去除前後空白，空字串視為未填。廠牌、型號與序號為自由文字；序號 MUST NOT 檢查重複。財產編號、主機名稱與管理介面位址另依 Unique property number、Hostname format 與 Management address 的規則驗證。識別欄位 MUST NOT 影響比對結果。

#### Scenario: Recording identity fields
- **WHEN** 匯入一列 `FW-EDGE`，廠牌「 Aruba 」、型號 `XYZ-100`、序號 `SN0001`、主機名稱 `fw-edge`
- **THEN** 資產記錄廠牌「Aruba」、型號 `XYZ-100`、序號 `SN0001` 與主機名稱 `fw-edge`，`asset show FW-EDGE` 列出它們

#### Scenario: Serial number may repeat
- **WHEN** `SRV-01` 的序號為 `SN0001`，使用者把 `SRV-02` 的序號也填為 `sn0001`
- **THEN** 兩個資產都儲存成功

### Requirement: Unique property number
財產編號 MUST NOT 與其他資產（含 retired，不分大小寫）的財產編號相同。清冊中兩列的財產編號相同，或與清冊以外的既有資產相同時，匯入 SHALL 中止整批並指出列號與已使用該編號的資產；新增或編修時 SHALL 拒絕該筆，資料庫 MUST NOT 變更。同一次匯入中兩個資產互換財產編號 SHALL 允許。

#### Scenario: Duplicate property number in the sheet
- **WHEN** 清冊第 3 列與第 5 列的財產編號分別為 `P-0001` 與 `p-0001`
- **THEN** 匯入中止並指出第 5 列的財產編號與第 3 列重複，資料庫不變

#### Scenario: Property number used by a retired asset
- **WHEN** retired 的 `NAS-02` 財產編號為 `P-0002`，使用者把 `NAS-01` 的財產編號改為 `P-0002`
- **THEN** 系統拒絕並指出財產編號 `P-0002` 已被 `NAS-02` 使用（該資產已退役），`NAS-01` 不變

#### Scenario: Swapping property numbers by import
- **WHEN** `SRV-01` 的財產編號為 `P-0001`、`SRV-02` 為 `P-0002`，匯入的清冊把兩者對調
- **THEN** 匯入成功，`SRV-01` 為 `P-0002`、`SRV-02` 為 `P-0001`

### Requirement: Hostname format
主機名稱 SHALL 只包含英文字母、數字、`-`、`.` 與 `_`，長度 MUST NOT 超過 253 個字元，並 SHALL 保留使用者的大小寫。其他字元（含空白、`/`、`:`、`@`、逗號與非 ASCII 字元）SHALL 被拒絕，並說明允許的字元。主機名稱 MUST NOT 檢查重複。

#### Scenario: Valid hostname
- **WHEN** 新增資產時主機名稱填 `SRV-01.lab.example`
- **THEN** 資產儲存成功，主機名稱為 `SRV-01.lab.example`

#### Scenario: URL in the hostname field
- **WHEN** 匯入時第 4 列的 `hostname` 為 `https://fw-edge`
- **THEN** 匯入中止並指出第 4 列的主機名稱只能包含英數字、`-`、`.` 與 `_`

### Requirement: Management address
管理介面位址 SHALL 是以 `http://` 或 `https://` 開頭、含主機的網址，SHALL 以使用者輸入（去除前後空白）的寫法儲存。其他協定、沒有協定或主機、含空白或控制字元的值 SHALL 被拒絕，並說明需以 `http://` 或 `https://` 開頭。網址包含帳號或密碼（例如 `https://admin:pw@192.0.2.1`）時 SHALL 拒絕，並說明帳號與密碼不得寫入資產資料。

#### Scenario: HTTPS management address
- **WHEN** `FW-EDGE` 的管理介面位址填 `https://192.0.2.1:8443/`
- **THEN** 資產儲存成功，管理介面位址為 `https://192.0.2.1:8443/`

#### Scenario: Address without a scheme
- **WHEN** 管理介面位址填 `192.0.2.1`
- **THEN** 系統拒絕並說明管理介面位址需以 `http://` 或 `https://` 開頭

#### Scenario: Credentials in the address
- **WHEN** 管理介面位址填 `https://admin:secret@192.0.2.1/`
- **THEN** 系統拒絕並說明帳號與密碼不得寫入管理介面位址，資料庫不變

### Requirement: Asset IP addresses
每個資產 SHALL 可有零到多筆 IP，每筆為一個 IPv4 或 IPv6 位址，加上選填的用途（自由文字，去除前後空白）。位址 SHALL 以標準寫法儲存（IPv6 為小寫壓縮寫法）。網段、位址範圍、帶 zone ID 的位址與其他無法解析的值 SHALL 被拒絕。同一資產 MUST NOT 有兩筆相同位址（以解析後的位址比較）；不同資產的位址 MAY 相同。IP MUST NOT 影響比對結果。

#### Scenario: Several addresses on one asset
- **WHEN** `SRV-01` 有 `192.0.2.10`（用途「管理」）與 `2001:db8::10` 兩筆 IP
- **THEN** 資產記錄兩筆 IP，`asset show SRV-01` 依序列出 `192.0.2.10（管理）` 與 `2001:db8::10`

#### Scenario: IPv6 written differently
- **WHEN** 同一資產的 IP 填了 `2001:db8::1` 與 `2001:DB8:0:0::1`
- **THEN** 系統拒絕並指出第二筆與 `2001:db8::1` 重複

#### Scenario: Network instead of an address
- **WHEN** IP 填 `192.0.2.0/24`
- **THEN** 系統拒絕並說明只能填單一 IPv4 或 IPv6 位址

#### Scenario: Same address on two assets
- **WHEN** `FW-EDGE` 與 `FW-EDGE-2` 都有 IP `192.0.2.1`
- **THEN** 兩個資產都儲存成功

### Requirement: Inventory ips sheet
清冊 SHALL 可包含 `ips` 工作表（標準欄名 `asset_tag`、`ip`、`purpose`，一列一筆）；CSV 清冊 SHALL 可以 `--ips` 另外指定同樣欄位的 IP 檔。有 `ips` 工作表時，清冊中每個資產的 IP SHALL 以該表為準，沒有列出的資產 SHALL 清空 IP；清冊以外的資產 MUST NOT 改變。`asset_tag` 不在 `assets` 工作表或 IP 無效時，匯入 SHALL 中止並列出工作表與列號。沒有 `ips` 工作表時，既有資產的 IP SHALL 保留，報表 SHALL 給出一次提醒。預覽 SHALL 列出 IP 有變更的資產。

#### Scenario: Importing addresses
- **WHEN** 清冊的 `ips` 工作表有兩列 `SRV-01`（`192.0.2.10`、`2001:db8::10`）
- **THEN** 匯入後 `SRV-01` 有這兩筆 IP，`--dry-run` 預覽把 `SRV-01` 列為更新並說明 IP 變更

#### Scenario: Asset not in the assets sheet
- **WHEN** `ips` 工作表第 4 列的 `asset_tag` 為 `SRV-99`，`assets` 工作表沒有 `SRV-99`
- **THEN** 匯入中止並指出 `ips` 工作表第 4 列的 `SRV-99` 不在 assets 工作表中，資料庫不變

#### Scenario: Asset left out of the ips sheet
- **WHEN** `NAS-01` 原有 IP `192.0.2.20`，匯入的清冊有 `ips` 工作表、`assets` 工作表有 `NAS-01`，但 `ips` 工作表沒有 `NAS-01` 的列
- **THEN** 匯入後 `NAS-01` 沒有 IP

#### Scenario: Inventory without an ips sheet
- **WHEN** `SRV-01` 有 IP `192.0.2.10`，使用者重匯沒有 `ips` 工作表的清冊
- **THEN** `SRV-01` 的 IP 不變，報表有一則「清冊沒有 ips 工作表，保留資料庫中既有資產的 IP」提醒

#### Scenario: CSV inventory with an IP file
- **WHEN** 執行 `cvewatch asset import assets.csv --ips ips.csv`，`ips.csv` 有一列 `SRV-01`、`192.0.2.10`
- **THEN** 匯入後 `SRV-01` 有 IP `192.0.2.10`

### Requirement: Inventory without identity columns
清冊的 `assets` 工作表缺少 `brand`、`model`、`serial_number`、`property_number`、`hostname` 或 `management_url` 欄時，既有資產的對應值 SHALL 保留，新增的資產 SHALL 以未填建立。缺少的欄位 SHALL 與 Inventory without new columns 的欄位合併在同一則提醒中列出。

#### Scenario: Re-importing an inventory from before this version
- **WHEN** `FW-EDGE` 的廠牌為「Aruba」、財產編號為 `P-0001`，使用者重匯沒有識別欄位的舊清冊
- **THEN** `FW-EDGE` 的廠牌與財產編號不變，報表的提醒列出缺少 `brand`、`model`、`serial_number`、`property_number`、`hostname` 與 `management_url` 欄

### Requirement: Showing asset identity
`cvewatch asset show` SHALL 列出資產有值的廠牌、型號、序號、財產編號、主機名稱與管理介面位址，並 SHALL 依儲存順序列出每筆 IP（有用途時以「位址（用途）」呈現）。未填的欄位與沒有 IP 時 SHALL 不列出該行。

#### Scenario: Showing a firewall with identity data
- **WHEN** `FW-EDGE` 的廠牌為「Aruba」、型號 `XYZ-100`、管理介面位址 `https://192.0.2.1/`、IP `192.0.2.1`（用途「管理」），沒有序號，執行 `cvewatch asset show FW-EDGE`
- **THEN** 輸出含「廠牌：Aruba」「型號：XYZ-100」「管理介面：https://192.0.2.1/」與「IP：192.0.2.1（管理）」，沒有序號一行

## MODIFIED Requirements

### Requirement: Blank template export
系統 SHALL 提供 `cvewatch asset template <path.xlsx>`，產出含 `assets`、`components`、`systems` 與 `ips` 工作表、標準欄名與範例列的空白範本，並 SHALL 附一張 `classes` 工作表列出目前生效的子類別代碼、名稱、描述、C/I/A 範本值與是否含個人資料的預設值；匯入時 `classes` 工作表 SHALL 被忽略。

#### Scenario: Generating a template
- **WHEN** 使用者執行 `cvewatch asset template D:\inv.xlsx`
- **THEN** 檔案包含 `assets`、`components`、`systems`、`ips`、`classes` 五個工作表，`assets`、`components`、`systems` 與 `ips` 第一列為標準欄名，`assets` 的表頭含 `brand`、`model`、`serial_number`、`property_number`、`hostname`、`management_url`

#### Scenario: Class sheet follows configuration
- **WHEN** 設定檔新增子類別 `HW12`，使用者產出範本
- **THEN** `classes` 工作表包含 `HW12` 與其名稱、範本值

### Requirement: Inventory export
系統 SHALL 提供 `cvewatch asset export <path.xlsx>`，把所有 active 資產匯出為含 `assets`、`components`、`systems` 與 `ips` 工作表、標準欄名的 Excel：每個資產一列 `assets`（含識別欄位；第一個元件寫在該列的 product / version），其餘清冊元件與未辨識產品列在 `components`，`systems` 列出所有資產說明，`ips` 每筆 IP 一列。匯出 MUST NOT 包含 retired 資產與主機套件元件；目的檔已存在時 SHALL 拒絕且不覆蓋。

#### Scenario: Round trip without changes
- **WHEN** 使用者執行 `cvewatch asset export D:\inv.xlsx` 後，立刻以 `cvewatch asset import D:\inv.xlsx --dry-run` 匯入同一檔案
- **THEN** 預覽結果為「新增 0、更新 0」、沒有版本變更、資產說明變更、IP 變更與清冊中消失的資產

#### Scenario: Unresolved product is kept
- **WHEN** `PVE-01` 有一筆未辨識產品 `Proxmox Backup 3.2`，執行匯出
- **THEN** `components` 工作表有一列 `PVE-01`、`Proxmox Backup`、`3.2` 原文，重匯後該資產仍列出同一筆未辨識產品

#### Scenario: Pending version is exported blank
- **WHEN** `NET-HW-003` 的 DSM 元件為版本待補，執行匯出後再以 `--dry-run` 匯入同一檔案
- **THEN** 檔案中該元件的 `version` 為空白，預覽結果為「新增 0、更新 0」，該元件仍為版本待補

#### Scenario: Retired assets and packages are left out
- **WHEN** `NAS-02` 為 retired，`WEB-01` 另有 dpkg 匯入的套件元件，執行匯出
- **THEN** 檔案中沒有 `NAS-02`，`WEB-01` 只匯出清冊元件，不含套件

#### Scenario: Export target exists
- **WHEN** `D:\inv.xlsx` 已存在
- **THEN** 指令以錯誤結束並說明目的檔已存在，原檔內容不變

#### Scenario: Systems are exported
- **WHEN** 有資產說明 `NET` 與 `SYS`，只有 `NET` 有 active 資產
- **THEN** `systems` 工作表列出 `NET` 與 `SYS` 兩列，`assets` 工作表的 `system` 欄寫資產說明代碼

#### Scenario: Addresses are exported
- **WHEN** `SRV-01` 有 IP `192.0.2.10`（用途「管理」）與 `2001:db8::10`，`NAS-01` 沒有 IP，執行匯出
- **THEN** `ips` 工作表有 `SRV-01` 的兩列（依儲存順序、`purpose` 分別為「管理」與空白），沒有 `NAS-01` 的列

### Requirement: Creating an asset
系統 SHALL 可不經清冊新增一個 active 資產；欄位驗證（必填欄位、類別、曝露度、C/I/A、ISMS 編號、資產說明、法規遵循性、是否含個人資料、識別欄位與 IP 的允許值與預設值）SHALL 與清冊匯入相同，且 SHALL 至少有一個以產品目錄產品與版本指定的元件。`asset_tag` 已被任何資產（含 retired，不分大小寫）使用時 SHALL 拒絕；財產編號依 Unique property number 檢查。

#### Scenario: Adding a new firewall
- **WHEN** 新增 `FW-02`，名稱 `fw02`、類別 `HW03`、元件 PAN-OS `11.2.3`，未指定曝露度、C/I/A 與是否含個人資料
- **THEN** 系統建立 active 資產 `FW-02`（曝露度 internal、C/I/A 2/3/4、資產價值 4、不含個人資料、防護等級未評定）與一個元件

#### Scenario: Tag used by a retired asset
- **WHEN** 新增 `NAS-02`，而 `NAS-02` 已存在且為 retired
- **THEN** 系統拒絕並說明該編號屬於已退役資產，可改為恢復該資產

#### Scenario: No component given
- **WHEN** 新增資產時沒有任何元件
- **THEN** 系統拒絕並說明至少需要一個元件，資料庫不變

#### Scenario: Adding an asset with addresses
- **WHEN** 新增 `FW-EDGE`，主機名稱 `fw-edge`、IP `192.0.2.1` 與 `192.0.2.2`，並加入一個元件
- **THEN** 系統建立 `FW-EDGE`，主機名稱為 `fw-edge`，IP 依輸入順序為 `192.0.2.1`、`192.0.2.2`

### Requirement: Editing asset fields
系統 SHALL 可修改 active 資產的 name、類別、曝露度、機密性、完整性、可用性、ISMS 編號、資產說明、權責單位、保管人、使用單位、法規遵循性、是否含個人資料、廠牌、型號、序號、財產編號、主機名稱、管理介面位址、IP、location、notes，驗證規則與清冊匯入相同；IP 以整份清單取代。修改類別 MUST NOT 自動改變已設定的 C/I/A 與是否含個人資料。`asset_tag` SHALL 只能依 Renaming an asset 的規則修改。

#### Scenario: Raising exposure
- **WHEN** 將 `NAS-01` 的曝露度由 internal 改為 internet
- **THEN** 資產欄位更新，之後的比對以 internet 曝露度計算該資產命中的風險等級

#### Scenario: Invalid criticality
- **WHEN** 將機密性改為 `5`
- **THEN** 系統拒絕並說明機密性只能是 1–4，資產不變

#### Scenario: Changing class keeps ratings
- **WHEN** `PVE-01` 類別為 `HW01`、C/I/A 為 3/3/4，使用者把類別改為 `HW05`
- **THEN** 類別變為 `HW05`，C/I/A 仍為 3/3/4

#### Scenario: Moving an asset to another system
- **WHEN** `FW-01` 歸屬 `SYS`、ISMS 編號為 `SYS-HW-003`，使用者把資產說明改為 `NET` 但未修改 ISMS 編號
- **THEN** 系統拒絕並指出 ISMS 編號的系統代碼 `SYS` 與資產說明 `NET` 不符，資產不變

#### Scenario: Replacing addresses
- **WHEN** `SRV-01` 有 IP `192.0.2.10` 與 `192.0.2.11`，使用者移除 `192.0.2.11`、加入 `2001:db8::10` 後儲存
- **THEN** `SRV-01` 的 IP 為 `192.0.2.10` 與 `2001:db8::10`

#### Scenario: One invalid address saves nothing
- **WHEN** 使用者把 `SRV-01` 的名稱改為 `srv01`，並加入 IP `192.0.2.300` 後儲存
- **THEN** 系統拒絕並指出 `192.0.2.300` 不是有效的 IP 位址，名稱與 IP 都不變
