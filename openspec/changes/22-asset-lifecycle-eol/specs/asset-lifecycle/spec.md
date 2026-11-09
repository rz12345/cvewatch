## Purpose

判斷資產的軟體版本分支與硬體是否已停止原廠支援（EoL），以及保固、維護合約與軟體授權何時到期，並在畫面、指令與每日摘要中依提醒天數標示，讓使用者在支援結束前安排升級或續約。

## ADDED Requirements

### Requirement: Software end-of-life sync
`cvewatch sync` SHALL 包含來源 `eol`：對使用中資產的清冊元件所屬、且在產品目錄宣告了 endoflife.date 代稱的每個產品，取得 endoflife.date 的版本分支名稱與 EoL 日期並保存在本機。`eol` SHALL 可用 `--source eol` 單獨執行，SHALL 可在設定檔停用，並 SHALL 依 cve-ingestion 的 Source failure isolation 與 Sync run records 處理失敗與紀錄。某個代稱在 endoflife.date 不存在時，系統 SHALL 在該次執行記錄提醒並保留其先前資料，MUST NOT 因此使其他產品失敗。`eol` 的資料變更 MUST NOT 觸發重新比對。

#### Scenario: Refreshing EoL data only
- **WHEN** 使用者執行 `cvewatch sync --source eol`
- **THEN** 只有 `eol` 來源被執行，產品的版本分支與 EoL 日期更新，其他來源游標不變，命中不重新比對

#### Scenario: endoflife.date unavailable
- **WHEN** 執行 `cvewatch sync`，endoflife.date 在重試後仍回應 503
- **THEN** 其他來源照常更新，先前取得的 EoL 資料保留，`sync_run` 記錄 `eol` 失敗原因，結束碼為 4

#### Scenario: Unknown product slug
- **WHEN** 目錄為某產品宣告的代稱在 endoflife.date 回應 404
- **THEN** 其他產品照常更新，`eol` 的執行紀錄有一則指出該代稱與產品的提醒

#### Scenario: Products without components are skipped
- **WHEN** 目錄宣告了 `nginx` 的代稱，但沒有使用中資產有 nginx 元件
- **THEN** `eol` 不查詢 nginx

### Requirement: Software release cycle mapping
系統 SHALL 依可解釋的規則把清冊元件的版本對應到一個版本分支：版本與分支名稱都切成以 `.`、`-`、`_`、空白與括號分隔的段落（數字段以數值比較、其他段不分大小寫），分支名稱的段落為版本段落的前綴時即符合，多個符合時取段落最多者；沒有符合時，再以分支的最新版本名稱做同樣的前綴比對。仍無法唯一決定時，該元件 SHALL 標為「無法對應版本分支」並列出版本，MUST NOT 猜測。版本待補的元件 MUST NOT 被對應。

#### Scenario: Version within a cycle
- **WHEN** PAN-OS 有分支 `10.2`、`11.1`、`11.2`，元件版本為 `11.1.6-h3`
- **THEN** 元件對應到分支 `11.1`

#### Scenario: Longest cycle wins
- **WHEN** 產品有分支 `9` 與 `9.16.1`，元件版本為 `9.16.1.2`
- **THEN** 元件對應到分支 `9.16.1`

#### Scenario: Cycle found through the latest version
- **WHEN** Windows Server 的分支 `2022` 最新版本為 `10.0.20348`，元件版本為 `10.0.20348.2700`
- **THEN** 元件對應到分支 `2022`

#### Scenario: No matching cycle
- **WHEN** 產品只有分支 `7` 與 `8`，元件版本為 `6.4`
- **THEN** 元件標為「無法對應版本分支（6.4）」，沒有 EoL 日期

### Requirement: Software end-of-life determination
每個使用中資產的清冊元件 SHALL 依下列順序決定軟體 EoL：元件的手動覆寫；產品目錄為該產品宣告的版本分支 EoL；endoflife.date 的版本分支 EoL。系統 SHALL 顯示採用的來源（手動覆寫的來源文字、目錄的來源網址或 endoflife.date）。endoflife.date 標示分支已 EoL 但沒有日期時 SHALL 視為已 EoL、日期未公布；分支沒有 EoL 日期且未標示 EoL 時 SHALL 顯示「未公布 EoL」。產品沒有任何 EoL 資料時 SHALL 顯示「無 EoL 資料」。主機套件元件與未辨識產品 MUST NOT 判定 EoL。

#### Scenario: EoL from endoflife.date
- **WHEN** `FW-EDGE` 的 PAN-OS 元件版本為 `10.2.9`，endoflife.date 資料中分支 `10.2` 的 EoL 為 2027-03-01
- **THEN** 該元件的軟體 EoL 為 2027-03-01，來源為 endoflife.date 分支 `10.2`

#### Scenario: Catalog entry for a product not covered
- **WHEN** 目錄為 Synology DSM 宣告分支 `7.1` 的 EoL 為 2026-06-01 與來源網址，`NAS-01` 的 DSM 版本為 `7.1.1-42962`
- **THEN** 該元件的軟體 EoL 為 2026-06-01，來源為目錄宣告的網址

#### Scenario: Component override wins
- **WHEN** `SRV-01` 的 ESXi 元件手動覆寫 EoL 為 2028-12-31、來源「延長支援合約」，endoflife.date 的分支 EoL 為 2027-10-11
- **THEN** 該元件的軟體 EoL 為 2028-12-31，來源顯示「延長支援合約（手動覆寫）」

#### Scenario: Pending version
- **WHEN** `NET-HW-003` 的 DSM 元件為版本待補
- **THEN** 該元件顯示「版本待補，無法判斷 EoL」，不列入 EoL 統計

### Requirement: Hardware end-of-life determination
每個使用中資產 SHALL 依下列順序決定硬體 EoL：資產的手動覆寫；以資產的廠牌與型號對應產品目錄的硬體型號清單。廠牌與型號比對 SHALL 不分大小寫、全形視同半形並忽略空白與 `-_./`，型號 SHALL 也比對別名。資產沒有廠牌或型號時 SHALL 顯示「未填廠牌或型號」；目錄沒有對應型號時 SHALL 顯示「目錄沒有此型號」；兩者都 MUST NOT 視為已 EoL。

#### Scenario: Model listed in the catalog
- **WHEN** `catalog.d` 宣告廠牌 `Aruba`、型號 `XYZ-100`、EoL 2026-10-31 與來源網址，`SW-CORE` 的廠牌為 `aruba`、型號為 `xyz100`
- **THEN** `SW-CORE` 的硬體 EoL 為 2026-10-31，來源為該網址

#### Scenario: Alias of a model
- **WHEN** 目錄的 `Aruba XYZ-100` 有別名 `XYZ-100F`，資產型號為 `XYZ-100F`
- **THEN** 資產對應到 `Aruba XYZ-100` 的 EoL

#### Scenario: Hardware override
- **WHEN** `SW-CORE` 對應到目錄的 EoL 2026-10-31，使用者手動覆寫硬體 EoL 為 2027-06-30、來源「原廠延長支援函」
- **THEN** `SW-CORE` 的硬體 EoL 為 2027-06-30，來源顯示「原廠延長支援函（手動覆寫）」

#### Scenario: Model not in the catalog
- **WHEN** `FW-EDGE` 的廠牌與型號在目錄中沒有對應
- **THEN** 硬體 EoL 顯示「目錄沒有此型號」，`FW-EDGE` 不標示為已 EoL

### Requirement: Lifecycle status
系統 SHALL 依本機日期把每個使用中資產的生命週期項目分類：硬體或任一軟體元件的 EoL 日期已到（含當天）或已 EoL 且日期未公布時為「已 EoL」；EoL 日期在提醒天數內時為「即將 EoL」；保固、維護合約或軟體授權的到期日已到時為「已到期」，在提醒天數內時為「即將到期」。提醒天數 SHALL 由設定檔 `[lifecycle] reminder_days` 設定，預設 90，允許 1–3650；超出範圍時設定驗證 SHALL 失敗。retired 資產 MUST NOT 列入任何生命週期狀態與提醒。

#### Scenario: EoL within the reminder window
- **WHEN** 今天為 2027-01-10，`FW-EDGE` 的 PAN-OS 元件 EoL 為 2027-03-01，`reminder_days` 為預設值
- **THEN** `FW-EDGE` 為「即將 EoL」，剩餘 50 天

#### Scenario: Warranty already expired
- **WHEN** 今天為 2027-01-10，`SRV-01` 的保固到期日為 2026-12-31
- **THEN** `SRV-01` 為保固「已到期」

#### Scenario: Shorter reminder window
- **WHEN** 設定 `reminder_days = 30`，今天為 2027-01-10，`SRV-01` 的維護合約到期日為 2027-03-01
- **THEN** `SRV-01` 沒有「即將到期」狀態

#### Scenario: Retired asset is not reminded
- **WHEN** retired 的 `NAS-02` 硬體 EoL 已過
- **THEN** `NAS-02` 不列入已 EoL 統計與任何提醒

#### Scenario: Reminder days out of range
- **WHEN** 設定檔寫 `reminder_days = 0`
- **THEN** 設定驗證失敗並說明允許的範圍為 1–3650

### Requirement: Lifecycle in asset show
`cvewatch asset show <asset_tag>` SHALL 列出資產的生命週期：硬體 EoL、每個清冊元件的軟體 EoL（含對應的版本分支）、保固、維護合約與軟體授權到期日，每一項 SHALL 顯示日期、狀態、剩餘或已過天數與來源；沒有資料的項目 SHALL 顯示原因（例如「未填」「目錄沒有此型號」「無 EoL 資料」）。

#### Scenario: Showing lifecycle items
- **WHEN** 今天為 2027-01-10，使用者執行 `cvewatch asset show FW-EDGE`，PAN-OS 元件 EoL 為 2027-03-01、保固未填
- **THEN** 輸出列出「PAN-OS 10.2.9（分支 10.2）EoL 2027-03-01 即將 EoL（剩 50 天），來源 endoflife.date」與「保固到期 未填」
