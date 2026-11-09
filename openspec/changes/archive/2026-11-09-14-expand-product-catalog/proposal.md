# Proposal

## Why

使用者實際維護的網通、儲存設備與網站軟體中，有一半以上的產品不在內建目錄裡（HPE Aruba、Cisco、MikroTik、Zyxel、RUCKUS、QSAN、CAYIN、Veeam、NAKIVO、nginx、MariaDB、Ruby on Rails 與前端函式庫），比對不到 CVE；同類別的其他常見品牌（Fortinet、Juniper、QNAP、Apache、MySQL、PHP 等）日後也可能出現在清冊中。目錄從 11 項擴充到約 78 項後，GUI 現行的平鋪下拉選單（尤其「未辨識產品 → 改為…」沒有搜尋）會變得難以使用，因此一併改善產品選擇方式。

## What Changes

- 內建目錄新增 67 項產品，各附 CPE、CNA、關鍵字與「廠牌＋產品名稱」寫法的別名：
  - 使用者環境的 18 項：AOS-CX（HPE Aruba Networking）、Cisco IOS、MikroTik RouterOS、RUCKUS Unleashed、RUCKUS SmartZone、PIXIS、QSAN SANOS、Veeam Backup & Replication、NAKIVO Backup & Replication、FreeRADIUS、nginx、MariaDB、Ruby、Ruby on Rails、CKEditor 4、Bootstrap、jQuery、Moment.js
  - 同類別常見品牌 49 項（依 cvelistV5 的 CVE 數與 CISA KEV 篩選）：
    - 防火牆／VPN：Fortinet FortiOS、FortiClient、Cisco ASA、Cisco Secure Firewall Threat Defense、SonicWall SonicOS、Sophos Firewall、WatchGuard Fireware、Check Point Gaia、Ivanti Connect Secure、F5 BIG-IP、NetScaler ADC
    - 交換器／路由：Cisco IOS XE、Cisco NX-OS、Juniper Junos OS、HPE AOS-Switch、Arista EOS
    - 無線：HPE ArubaOS（控制器／Instant）、UniFi Network
    - 儲存：QNAP QTS、QNAP QuTS hero、ASUSTOR ADM、TrueNAS、NetApp ONTAP
    - 備份：Acronis Cyber Protect、Commvault、Veritas NetBackup、Veritas Backup Exec
    - 網站伺服器與資料庫：Apache HTTP Server、Apache Tomcat、MySQL、PostgreSQL、Microsoft SQL Server、MongoDB、Redis
    - 執行環境／框架／CMS：PHP、Python、Node.js、Java SE、Django、Laravel、WordPress、Drupal
    - 前端函式庫：TinyMCE、Lodash、jQuery UI
    - 常用元件：OpenSSL、OpenSSH、Samba、BIND 9
- 版控中的內容（OpenSpec、種子目錄、測試、文件）只寫廠牌與產品名稱，不寫使用者的硬體型號：
  - 型號寫法對應到韌體產品的別名，以及 CVE 識別資料逐型號發布的產品（Zyxel 交換器韌體、CAYIN 媒體播放器），寫在使用者資料目錄的 `catalog.d`（不在版控）
  - 新增 commit 前的檢查腳本：以本機（不在版控）的敏感字詞清單比對暫存區內容
- 新增四種版本格式：
  - `cisco`：IOS `15.2(7)E8`、ASA `9.18(4)22`、NX-OS `10.2(3t)`，同一主次版本與 train 內可比較
  - `junos`：Junos `21.4R3-S5`、Ivanti `22.7R2.4`
  - `numeric`：數字分段，分隔可為 `.`、`-`、`_` 或 `build`，可有一個字母前綴（SonicOS、NetScaler、QNAP、Check Point、Acronis）
  - `zyxel`：`V主.次(型號代碼.修補)C數字`，同一型號代碼內可比較
- GUI「加入元件」與「未辨識產品 → 改為…」改用同一個產品選擇器：
  - 搜尋涵蓋產品名稱、別名、廠商，不分大小寫並忽略空白與 `-_./`；開頭相符者排前面
  - 結果超過上限時只顯示前幾筆並提示「再輸入文字縮小範圍」
  - 可用 ↑↓ 移動、Enter 選擇、Esc 關閉
  - 處理未辨識產品時，最上方列出與原文相似的目錄產品建議
- 15 為 15-scrub-model-names（清理既有型號）；原定 ISMS 對齊系列 14–18 依序順延為 16–20（16-isms-admin-fields … 20-isms-export）

## Capabilities

### New Capabilities

（無）

### Modified Capabilities

- `product-catalog`：新增「Seed catalog coverage」與「Seed catalog excludes hardware models」
- `vuln-matching`：「Vendor-specific version comparison」加入 Cisco、Juniper、數字分段與 Zyxel 版本格式
- `gui-console`：新增「GUI product picker」相關需求（搜尋與排序、筆數上限、鍵盤操作、未辨識建議）

## Out of scope

- 伺服器 BMC 的其他品牌（HPE iLO、Lenovo XClarity、Supermicro）、Microsoft IIS（CVE 歸在 Windows）、AngularJS、Vue.js、TP-Link（逐型號）
- 依元件角色分組或依資產子類別排序產品（屆時為 18-component-roles）
- 硬體本身的廠牌、型號欄位（屆時為 17-asset-identity-network）
- 硬體與軟體的 EoL 資料（屆時為 19-asset-lifecycle-eol）
- 清除既有版控內容中已出現的型號（15-scrub-model-names）；改寫 git 歷史
- 官網函式庫改走 SBOM／OSV 套件比對；本 change 以目錄產品處理
- 以 GUI 編輯產品目錄（仍只能改 `catalog.d`）
- TUI 的產品選擇（TUI 不編輯資產）

## Impact

- `crates/core/catalog/seed.toml`：新增產品與別名
- `crates/core/src/catalog/scheme.rs`、`crates/core/src/matching/version/`：新增 `cisco`、`junos`、`numeric`、`zyxel` 四種格式
- `crates/frontend/src/asset_form.rs`：產品搜尋改為正規化比對、排序與筆數上限
- `crates/gui/src/dialogs/asset_components.rs`（及新的選擇器模組）：兩處產品選單改用共用選擇器
- `scripts/`：新增 commit 前敏感字詞檢查腳本（字詞清單在不入版控的資料目錄）
- 使用者資料目錄 `catalog.d`：本機型號別名與逐型號產品（不入版控）
- 測試：`crates/core/tests/catalog.rs`、`version_compare.rs`、`crates/frontend/tests`、`crates/gui/tests/asset_edit.rs`；必要時新增 `tests/fixtures` 的 CVE 記錄樣本
- 文件：`docs/user-guide/assets.md`、`docs/user-guide/gui.md`、`docs/development.md`
- 既有資料：不需 migration；sync 會把新目錄同步到資料庫，之前未辨識的寫法在下次匯入時即可辨識
