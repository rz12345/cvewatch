## ADDED Requirements

### Requirement: GUI asset identity input
資產編輯對話框 SHALL 有「識別與網路」區塊，提供廠牌、型號、序號、財產編號、主機名稱與管理介面位址的選填文字欄位，以及 IP 清單：每列有 IP 與用途兩個欄位和「移除」，另有「新增 IP」可加入空白列。完全空白的 IP 列 SHALL 在儲存時忽略。驗證錯誤 SHALL 以介面用語顯示在對應欄位或 IP 列旁，且不關閉對話框；財產編號已被其他資產使用時，錯誤 SHALL 在寫入前顯示在財產編號欄位旁。規則 SHALL 與 asset-inventory 相同。

#### Scenario: Adding addresses in the dialog
- **WHEN** 使用者編輯 `SRV-01`，按兩次「新增 IP」，分別填 `192.0.2.10`（用途「管理」）與 `2001:db8::10` 後儲存
- **THEN** 對話框關閉，資產詳情列出 `192.0.2.10（管理）` 與 `2001:db8::10`

#### Scenario: Invalid address on its row
- **WHEN** 使用者在第二個 IP 列填 `192.0.2.300` 後按「儲存」
- **THEN** 對話框不關閉，第二個 IP 列旁說明這不是有效的 IPv4 或 IPv6 位址，資料庫不變

#### Scenario: Property number in use
- **WHEN** 使用者編輯 `NAS-01`，把財產編號改為 retired 的 `NAS-02` 已使用的 `P-0002` 後按「儲存」
- **THEN** 對話框不關閉，財產編號欄位旁說明已被使用（已退役的 `NAS-02`），資料庫不變

#### Scenario: Management address without a scheme
- **WHEN** 管理介面位址填 `192.0.2.1` 後按「儲存」
- **THEN** 對話框不關閉，管理介面位址欄位旁說明需以 `http://` 或 `https://` 開頭

### Requirement: GUI asset identity detail
資產詳情 SHALL 顯示廠牌、型號、序號、財產編號、主機名稱、管理介面位址與 IP（每筆「位址（用途）」，依儲存順序），未填的欄位與沒有 IP 時 SHALL 顯示「-」。管理介面位址 SHALL 顯示成連結，點擊後 SHALL 以系統預設瀏覽器開啟（`http` 與 `https` 都可開啟），狀態列顯示結果；無法開啟瀏覽器時 SHALL 在狀態列說明原因。

#### Scenario: Identity in the detail pane
- **WHEN** 使用者選取廠牌「Aruba」、型號 `XYZ-100`、主機名稱 `fw-edge`、IP `192.0.2.1`（用途「管理」）、沒有序號與財產編號的 `FW-EDGE`
- **THEN** 詳情顯示廠牌「Aruba」、型號 `XYZ-100`、序號「-」、財產編號「-」、主機名稱 `fw-edge` 與 IP `192.0.2.1（管理）`

#### Scenario: Opening the management address
- **WHEN** `FW-EDGE` 的管理介面位址為 `http://192.0.2.1/`，使用者點擊詳情中的該連結
- **THEN** 系統以預設瀏覽器開啟 `http://192.0.2.1/`，狀態列顯示已開啟

#### Scenario: Asset without a management address
- **WHEN** 使用者選取沒有管理介面位址的 `NAS-01`
- **THEN** 詳情的管理介面位址顯示「-」，不顯示連結

### Requirement: GUI asset list search
資產清單上方 SHALL 有搜尋框，依資產編號、名稱、主機名稱、IP、序號與財產編號篩選：部分相符即列出，不分大小寫；查詢字是完整 IP 位址時，SHALL 也比對寫法不同但位址相同的 IP。搜尋 SHALL 與「顯示已退役」「只顯示版本待補」同時套用；沒有符合資產的資產說明組 MUST NOT 出現。搜尋中各組 SHALL 展開顯示，清除搜尋後 SHALL 恢復原本的收合狀態。沒有任何符合的資產時，清單 SHALL 說明沒有符合的資產。

#### Scenario: Searching by address
- **WHEN** `SRV-01` 有 IP `192.0.2.10`，使用者在搜尋框輸入 `192.0.2.1`
- **THEN** 清單列出 `SRV-01`，以及其他 IP、編號或名稱含 `192.0.2.1` 的資產

#### Scenario: Searching by property number with letter case
- **WHEN** `NAS-01` 的財產編號為 `P-0001`，使用者輸入 `p-00`
- **THEN** 清單列出 `NAS-01`

#### Scenario: Search combined with the retired filter
- **WHEN** retired 的 `NAS-02` 主機名稱為 `nas02`，使用者輸入 `nas0`，「顯示已退役」未勾選
- **THEN** 清單不列出 `NAS-02`；勾選「顯示已退役」後才以退役樣式列出

#### Scenario: Collapsed group while searching
- **WHEN** `NET` 組已收合，組內的 `FW-EDGE` 主機名稱為 `fw-edge`，使用者輸入 `fw-e`
- **THEN** `NET` 組展開並列出 `FW-EDGE`；清除搜尋後 `NET` 組恢復收合

#### Scenario: No matching asset
- **WHEN** 使用者輸入 `zzz`，沒有任何資產符合
- **THEN** 清單說明沒有符合「zzz」的資產

### Requirement: GUI CSV IP file
匯入 CSV 清冊時，匯入對話框 SHALL 除了元件檔之外，也可另選 IP 檔；選擇後 SHALL 重新產生預覽。匯入規則 SHALL 與 `cvewatch asset import --ips` 相同；沒有選擇 IP 檔時，既有資產的 IP SHALL 保留，預覽 SHALL 顯示對應的提醒。

#### Scenario: Choosing an IP file
- **WHEN** 使用者拖入 `assets.csv`，在匯入對話框按「選擇 IP 檔」並選擇 `ips.csv`
- **THEN** 對話框顯示 IP 檔的路徑並重新預覽，預覽列出 IP 有變更的資產；按「匯入」後才寫入
