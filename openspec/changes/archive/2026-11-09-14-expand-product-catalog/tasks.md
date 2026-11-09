# Tasks

## 1. commit 前敏感字詞檢查

- [x] 1.1 新增 `scripts/check-sensitive.ps1`（design D8）：讀 `data\sensitive-patterns.txt`，比對 `git diff --cached` 新增的行，命中時列出檔名與行號並以非 0 結束；清單不存在時提示並以非 0 結束；完成條件：腳本內不含任何型號字詞
- [x] 1.2 在本機建立 `data\sensitive-patterns.txt`（依使用者的來源檔列出型號、資產編號等，不入版控），並確認 `git check-ignore` 涵蓋它；完成條件：`git status` 不出現該檔
- [x] 1.3 測試：以暫存一行含清單字詞的暫存檔（測後還原）手動執行腳本，確認命中時非 0、未命中時為 0、清單不存在時非 0；在 `docs/development.md` 記錄用法與掛 pre-commit hook 的方式

## 2. 版本格式 cisco、junos、numeric、zyxel

- [x] 2.1 在 `matching/version/` 新增 `cisco.rs`（design D4：IOS、ASA、NX-OS 寫法，同主次版本與 train 才可比較），並加入 `Version` 列舉與 `Schemes`；完成條件：單元測試涵蓋 spec 的「Cisco ordering within a train」「Cisco versions from different trains」「Cisco ASA interim releases」
- [x] 2.2 新增 `junos.rs`（design D4b：`R`／`-S`、Ivanti 小數 release、跨主次版本可比較、特殊類型只與同類型比較）；完成條件：單元測試涵蓋「Juniper service releases」「Juniper versions from different lines」
- [x] 2.3 新增 `numeric.rs`（design D4c：字母前綴、`.`／`-`／`_`／`build` 分隔）；完成條件：單元測試涵蓋「Numeric segments with build numbers」與前綴不同為 None
- [x] 2.4 新增 `zyxel.rs`（解析 `V2.80(ABCD.1)C0`、型號代碼不同為 None；測試只用虛構代碼）；完成條件：單元測試涵蓋「Zyxel firmware ordering」「Zyxel firmware for another model」
- [x] 2.5 在 `catalog/scheme.rs` 註冊四種格式的寫法驗證，更新 `seed.toml` 頂端的格式說明；完成條件：`scheme_names()` 含四者，無法解析的寫法產生匯入警告
- [x] 2.6 測試：`crates/core/tests/version_compare.rs` 加入四種格式的排序與未知案例，`matching_rules.rs` 加 Cisco 跨 train 為未知、Junos 跨版本線區間判定的 CNA 案例；`cargo test -p cvewatch-core` 通過

## 3. 種子目錄擴充

- [x] 3.1 補完 design.md D2 第一張表中網通與儲存產品（AOS-CX、Cisco IOS、RouterOS、RUCKUS Unleashed／SmartZone、PIXIS、QSAN）標「待補」或「待確認」的識別資料，並解決 QSAN 的 Open Question；完成條件：這幾列不再有「待補」「待確認」
- [x] 3.2 補完 Veeam、NAKIVO、FreeRADIUS、nginx、MariaDB、Ruby、Rails（含元件 gem 的 CPE 與 CNA product）、CKEditor 4、Bootstrap、jQuery、Moment.js 的識別資料；完成條件同 3.1
- [x] 3.3 把第一張表的 18 項寫入 `seed.toml`（分「網路／資安」「儲存」「備份」「網站與資料庫」「執行環境與框架」「前端函式庫」「系統元件」區段），別名只用廠牌＋產品寫法，並為既有產品補 D2 所列別名；完成條件：目錄載入成功，`asset catalog show pixis` 顯示只有關鍵字
- [x] 3.4 防火牆／VPN 11 項（D2 第二張表）：補完「待補」的 CPE（FTD、Sophos、BIG-IP 模組、NetScaler Gateway），確認 FortiClient 的 CNA product 寫法，寫入 `seed.toml`；完成條件：這幾列不再有「待補」，目錄載入成功
- [x] 3.5 交換器／路由與無線 7 項：補完 AOS-Switch 的 CPE，寫入 `seed.toml`；完成條件同 3.4
- [x] 3.6 儲存與備份 9 項：補完 QuTS hero、TrueNAS、ONTAP 的 CPE，確認 Veritas 兩項的 CNA 寫法，寫入 `seed.toml`；完成條件同 3.4
- [x] 3.7 網站伺服器與資料庫 7 項：補完 SQL Server 各年版 CPE，寫入 `seed.toml`；完成條件同 3.4
- [x] 3.8 執行環境／框架／CMS 8 項：補完 Java SE 的 CPE 與近年 CNA 寫法，寫入 `seed.toml`；完成條件同 3.4
- [x] 3.9 前端函式庫與常用元件 7 項寫入 `seed.toml`；完成條件：`cvewatch asset catalog` 在沒有 `catalog.d` 時列出 78 項，`scripts/check-sensitive.ps1` 對暫存的 `seed.toml` 無命中
- [x] 3.10 在 `tests/fixtures` 加入代表性 CVE 記錄樣本（AOS-CX、Cisco IOS、Cisco ASA、Junos、FortiOS、QNAP QTS、Veeam、jQuery、Ruby 的 `n/a / n/a` 記錄各一筆，取自 cvelistV5 並註明來源 CVE 編號；只選不含使用者型號的記錄）；完成條件：fixture 可被既有 cvelist 測試載入，`check-sensitive.ps1` 無命中
- [x] 3.11 在本機 `data\catalog.d\site.toml` 依使用者來源檔寫入型號別名（對應 AOS-CX、Cisco IOS、RouterOS、RUCKUS、QSAN、PAN-OS、DSM 等）與逐型號產品（Zyxel 交換器韌體、CAYIN 媒體播放器）；不入版控；完成條件：目錄載入無別名衝突，來源檔中的設備寫法都能辨識，`git status` 不出現該檔
- [x] 3.12 測試：`crates/core/tests/catalog.rs` 加入 spec 的「Network firmware and appliances」「Storage and backup products」「Server software and libraries」「Brand and product spellings」「Product with keywords only」「Model spelling without a local alias」「Model alias declared locally」「Per-model product declared locally」（只用虛構型號），以及種子目錄無別名衝突、每項至少一組 CPE／CNA／關鍵字；`matching_e2e.rs` 以 3.10 的 fixture 驗證各產品可命中、Ruby 的 `n/a / n/a` 記錄以「僅關鍵字相符」出現；以既有 cvelistV5 fixture 跑比對確認沒有新增誤命中；`cargo test` 通過

## 4. 產品搜尋（frontend）

- [x] 4.1 改寫 `search_products`：query 與顯示名稱、別名、廠商、key 都經 `catalog::normalize` 比對，依「開頭相符／中間相符」兩級與顯示名稱排序；完成條件：`aoscx` 找到 AOS-CX、`ruby` 的順序為 Ruby、Ruby on Rails
- [x] 4.2 新增 `PICKER_LIMIT = 12` 與回傳前 N 筆加其餘筆數的函式，以及未辨識模式的候選組合（建議在前、搜尋結果去除重複）；完成條件：單元測試涵蓋 30 項時剩 18 項、78 項時剩 66 項、建議不重複出現
- [x] 4.3 測試：`crates/frontend/tests` 加入 spec「Searching by vendor」「Search ignores separators」「Prefix matches first」「Too many results」「No match」的對應測試；`cargo test -p cvewatch-frontend` 通過

## 5. GUI 產品選擇器

- [x] 5.1 新增 `dialogs/product_picker.rs`（按鈕＋Popup、搜尋框、反白、↑↓／Enter／Esc、筆數上限提示、「沒有相符的產品」），狀態從 `AssetEdit.search`／`picker_open` 移入；完成條件：`cargo clippy --all-targets -- -D warnings` 通過
- [x] 5.2 「加入元件」改用選擇器，保留開啟時焦點在搜尋框、加入後焦點到版本欄的行為；完成條件：既有測試 `asset_edit_keyboard_flow_when_adding_a_component` 仍通過
- [x] 5.3 未辨識產品的「改為…」改用選擇器的未辨識模式（「保留」、建議、搜尋結果），選擇後處置顯示「改為 <顯示名稱>」；完成條件：既有測試 `asset_edit_mapping_an_unresolved_product` 仍通過
- [x] 5.4 調整既有 GUI 測試在 12 筆上限下的選擇方式（先輸入搜尋字）；完成條件：`cargo test -p cvewatch-gui` 全部通過
- [x] 5.5 測試：`crates/gui/tests/asset_edit.rs` 加入 spec「Picking with the keyboard」「Escape leaves components unchanged」「Too many results」「Suggestion for a typo」「Searching beyond suggestions」；`cargo test` 通過

## 6. 文件與整體驗證

- [x] 6.1 更新 `docs/user-guide/assets.md`：新增產品清單、在 `catalog.d` 為硬體型號加別名與宣告逐型號產品的寫法（以虛構型號示範，並提醒備份 `catalog.d`）、函式庫與 SBOM 擇一、發行版套件建議用 `import-packages`、升級時與 `catalog.d` 別名衝突的處理；`docs/user-guide/gui.md`：選擇器的搜尋、上限、鍵盤與未辨識建議；完成條件：文件中的產品名稱與 `asset catalog` 輸出一致
- [x] 6.2 驗證：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 全部通過，`openspec validate 14-expand-product-catalog --strict` 通過，暫存全部變更後 `scripts/check-sensitive.ps1` 無命中

## Workflow follow-up

- 依 `/opsx-auto` 流程：code-review 一輪、判斷架構變更（本 change 預期無 crate 結構或外部整合變更）、每次 commit 前執行 `scripts/check-sensitive.ps1`、archive 並同步主 spec
- archive 後實作 15-scrub-model-names，再 propose 16-isms-admin-fields
