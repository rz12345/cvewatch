## MODIFIED Requirements

### Requirement: Alias resolution
系統 SHALL 將產品名稱以 NFKC、小寫並移除空白與 `-_./` 後，與目錄別名完全比對來決定產品。

#### Scenario: Different spellings of the same product
- **WHEN** 清冊中分別出現「PAN-OS」「Palo Alto PAN-OS」「panos」
- **THEN** 三者都對應到產品 `paloalto-panos`

### Requirement: User catalog extensions
系統 SHALL 載入內建種子目錄後，再載入資料目錄 `catalog.d` 下的 TOML 檔；相同 `key` 的產品 SHALL 以使用者檔中有寫的欄位覆寫、未寫的欄位沿用，`aliases` SHALL 與既有別名合併；新 `key` SHALL 新增。

#### Scenario: Adding an alias locally
- **WHEN** 使用者在 `catalog.d\local.toml` 為 `vmware-esxi` 增加別名 `xyz-200 esxi`
- **THEN** 重匯後清冊中的「XYZ-200 ESXi」被辨識為 `vmware-esxi`

#### Scenario: Conflicting alias
- **WHEN** 兩個不同產品宣告相同別名
- **THEN** 系統拒絕載入目錄，錯誤訊息指出衝突的別名與兩個產品 key
