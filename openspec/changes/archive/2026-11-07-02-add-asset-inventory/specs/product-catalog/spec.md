# Spec Delta

## Purpose

把清冊中各種產品寫法正規化為單一產品識別，並提供比對所需的 CPE、CNA vendor/product 與版本格式，讓資產能對應到 CVE 資料。

## ADDED Requirements

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
每個目錄產品 SHALL 可定義零到多組 CPE（part、vendor、product）、零到多組 CNA vendor/product 寫法，以及零到多組 CSAF 公告中的 vendor/product 名稱。

#### Scenario: Product with CNA identifiers only
- **WHEN** 某產品沒有 CPE 但有 CNA vendor/product
- **THEN** 目錄載入成功，`asset catalog show` 顯示其 CPE 為空、CNA 識別一筆

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
