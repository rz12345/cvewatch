## ADDED Requirements

### Requirement: GUI product picker
資產對話框的「加入元件」與未辨識產品的「改為…」SHALL 使用同一種產品選擇器：含搜尋框與結果清單，每筆以「顯示名稱（廠商）」呈現。搜尋 SHALL 比對顯示名稱、別名與廠商，不分大小寫並忽略空白與 `-_./`；顯示名稱或別名以搜尋字開頭者 SHALL 排在只有中間相符者之前，同級依顯示名稱排序。搜尋框為空時 SHALL 依顯示名稱列出全部產品。

#### Scenario: Searching by vendor
- **WHEN** 使用者在「加入元件」輸入 `aruba`
- **THEN** 清單列出 AOS-CX（HPE Aruba Networking）

#### Scenario: Search ignores separators
- **WHEN** 使用者輸入 `aoscx`
- **THEN** 清單列出 AOS-CX

#### Scenario: Prefix matches first
- **WHEN** 使用者輸入 `ruby`
- **THEN** Ruby 排在 Ruby on Rails 之前，兩者都排在只有中間相符的產品之前

### Requirement: GUI product picker result limit
產品選擇器的結果超過 12 筆時 SHALL 只顯示前 12 筆，並 SHALL 在清單末端顯示其餘筆數與「再輸入文字縮小範圍」的提示；沒有相符產品時 SHALL 顯示「沒有相符的產品」。

#### Scenario: Too many results
- **WHEN** 目錄有 30 項產品，使用者開啟「加入元件」且未輸入搜尋字
- **THEN** 清單顯示 12 筆，末端顯示「還有 18 項，再輸入文字縮小範圍」

#### Scenario: No match
- **WHEN** 使用者輸入 `zzz`
- **THEN** 清單顯示「沒有相符的產品」

### Requirement: GUI product picker keyboard operation
產品選擇器開啟時輸入焦點 SHALL 位於搜尋框，第一筆結果 SHALL 為反白；↑、↓ SHALL 移動反白，Enter SHALL 選擇反白項目，Esc SHALL 關閉選擇器且不變更元件。搜尋字改變時反白 SHALL 回到第一筆。

#### Scenario: Picking with the keyboard
- **WHEN** 使用者開啟「加入元件」，輸入 `cisco ios` 後按 Enter
- **THEN** 加入 Cisco IOS 元件，焦點移到該元件的版本欄

#### Scenario: Escape leaves components unchanged
- **WHEN** 使用者開啟「加入元件」，按 ↓ 後按 Esc
- **THEN** 選擇器關閉，元件清單不變

### Requirement: GUI unresolved product suggestions
未辨識產品的「改為…」選擇器 SHALL 在搜尋結果之上列出「保留」，以及與原文相似的目錄產品建議（與匯入報表相同的最多 3 個）；沒有建議時 SHALL 只列「保留」與搜尋結果。選擇產品後該列處置 SHALL 顯示「改為 <顯示名稱>」，儲存時沿用原版本字串。

#### Scenario: Suggestion for a typo
- **WHEN** 未辨識產品原文為「Synolgy DSM」，使用者開啟其「改為…」
- **THEN** 選擇器最上方列出建議 DiskStation Manager；選擇後處置顯示「改為 DiskStation Manager」

#### Scenario: Searching beyond suggestions
- **WHEN** 未辨識產品原文為「核心交換器」，使用者在其「改為…」輸入 `cx`
- **THEN** 搜尋結果列出 AOS-CX，可直接選擇
