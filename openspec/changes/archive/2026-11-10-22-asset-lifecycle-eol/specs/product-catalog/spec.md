## ADDED Requirements

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
