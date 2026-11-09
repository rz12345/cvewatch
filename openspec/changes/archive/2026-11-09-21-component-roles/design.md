# Design

## Context

- 產品目錄由內建種子 `crates/core/catalog/seed.toml`（78 項）加 `<data_dir>\catalog.d\*.toml`（依檔名排序）合併而成。`file.rs` 的 `ProductDef` 以 `deny_unknown_fields` 反序列化；`mod.rs` 的 `merge_into` 對同 key 以「有寫的欄位覆寫、沒寫的沿用」合併，但合併後的條目保留**第一次出現時的來源名稱**，`build_product` 的錯誤（例如 `version_scheme` 不存在）因此指向該來源，而非實際寫錯的使用者檔
- `version_scheme` 的驗證慣例：`scheme_by_name` 完全比對，失敗時 `catalog_error(origin, "產品 `{key}` 的 version_scheme `{x}` 不存在；可用：…")`，目錄載入失敗
- `Catalog::fingerprint` 只含影響比對的欄位（key、版本格式、CPE、CNA、CSAF、關鍵字、ESXi build）；指紋改變時下次 match 全量重算。`sync_catalog` 把產品寫入 `catalog_product`，但畫面與 CLI 的顯示名稱都直接讀記憶體中的 `Catalog`
- 元件：`ComponentKind::Product { product_key }`（清冊）或 `ComponentKind::Package { purl }`（SBOM／dpkg／rpm）。編輯對話框的 `ComponentDraft.package` 為 `!is_inventory_product()`
- 顯示位置：GUI `screens/assets.rs` 的 `components_list`（標題「元件（N）」後平鋪 `component_line`，其下為版本歷史）；GUI `dialogs/asset_components.rs` 的 `components`（3 欄 `Grid`，欄位錯誤以列索引 `errors.component(i)` 對應，`focus_version` 也是列索引）；TUI `ui/screens.rs` 資產詳情的元件表格（元件、版本、來源、更新時間，最多 8 列）；CLI `asset_view.rs` 的 `show`（目前不載入目錄，元件以 `c.label()` 輸出）、`catalog_list`、`print_product`
- ISMS 子類別（`assets/classes.rs` 內建 v2.1）：SW01 應用系統、SW02 作業系統、SW03 資料庫管理系統、SW04 網站服務系統、SW05 應用工具軟體、SW06 資通安全系統、SW07 一般套裝軟體；名稱可由設定檔 `[asset_classes]` 覆寫

## Goals / Non-Goals

**Goals:**

- 角色的值、中文名稱與組順序只定義一次（core），GUI、TUI、CLI 共用同一個分組函式（frontend）
- 不需要 migration，不改變目錄指紋與比對結果
- 使用者檔寫錯 `role` 時，錯誤訊息指出實際寫錯的檔案

**Non-Goals:**

- 不把角色寫入 `catalog_product` 或任何資料表
- 不修正 `version_scheme` 等其他欄位在覆寫時指向原始來源的既有行為（只有 `role` 採逐來源驗證；見 Risks）
- 不改變元件在資料庫中的順序與清冊匯出的元件順序

## Decisions

### D1 角色與 ISMS 子類別的對應、中文名稱與組順序

`crates/core/src/catalog/role.rs` 新增 `ComponentRole`（`Copy`、`Ord`，列舉順序即顯示順序）：

| 順序 | 值 | 中文名稱 | 對應 ISMS 子類別 |
|---|---|---|---|
| 1 | `firmware` | 韌體 | —（實體設備的系統軟體，屬 HW 資產） |
| 2 | `bios` | BIOS | — |
| 3 | `bmc` | BMC | — |
| 4 | `os` | 作業系統 | SW02 作業系統 |
| 5 | `dbms` | 資料庫管理系統 | SW03 資料庫管理系統 |
| 6 | `web` | 網站服務系統 | SW04 網站服務系統 |
| 7 | `tool` | 應用工具軟體 | SW05 應用工具軟體 |
| 8 | `security` | 資通安全系統 | SW06 資通安全系統 |
| 9 | `suite` | 一般套裝軟體 | SW07 一般套裝軟體 |
| 10 | `app` | 應用程式元件 | —（執行環境、框架、CMS、函式庫；SW01 應用系統是自行或委外開發的系統，不是目錄產品） |
| 11 | `other` | 其他 | —（未宣告時的預設值） |

- 套件組（「套件」）不是目錄角色，由 frontend 在最後加上（D5）
- 中文名稱是固定常數，**不**隨設定檔 `[asset_classes]` 改名：角色是目錄的屬性，與使用者自訂的子類別表無關；本 change 也不做角色與子類別的關聯（proposal Out of scope）
- 硬體相關角色（韌體、BIOS、BMC）排在前面，接著依 SW02–SW07 的順序，最後是應用程式元件與其他：由底層到上層閱讀
- API：`ComponentRole::ALL`（顯示順序）、`as_str()`、`label()`、`parse(&str) -> Option<ComponentRole>`（完全比對小寫值，與 `scheme_by_name` 相同，不 trim、不分大小寫轉換）、`allowed() -> String`（錯誤訊息用，以「、」串接；code review 後由 `names()` 改名）、`Display`、`Default = Other`
- 替代方案：讓 `bios`／`bmc` 併入 `firmware`。使用者已確認要分開，伺服器常同時有兩者，分開顯示較清楚

### D2 目錄欄位、合併與驗證

- `ProductDef` 新增 `pub role: Option<String>`；`Product` 新增 `pub role: ComponentRole`
- `merge_into`：`role: def.role.or(existing.role.take())`（與 `version_scheme` 相同的覆寫規則）；多個使用者檔依檔名順序載入，最後寫的為準
- **逐來源驗證**：`from_sources` 在每份來源解析後、`merge_into` 之前，對每個有寫 `role` 的條目呼叫 `ComponentRole::parse`，失敗時回傳
  `catalog_error(&source.name, "產品 `{key}` 的 role `{value}` 不存在；可用：firmware、bios、bmc、os、dbms、web、tool、security、suite、app、other")`
  如此覆寫既有 key 時，錯誤也指向寫錯的 `catalog.d` 檔（`merge_into` 保留的是第一個來源名稱，若在 `build_product` 才驗證會誤指「內建種子目錄」）
- `build_product`：`role` 已驗證，`None` 時為 `ComponentRole::Other`
- 明確寫 `role = "other"` 是合法值，讓使用者可以把種子產品改回「其他」
- `fingerprint` **不**加入角色：角色只影響顯示（spec Role change does not rematch）
- `sync_catalog` 不變，`catalog_product` 不加欄位、不需要 migration
- `seed.toml` 檔頭的欄位說明加入 `role` 與可用值；每個 `[[product]]` 在 `version_scheme` 之後加 `role = "…"`

### D3 種子產品的角色（78 項）

依 `seed.toml` 區段排列。判斷原則：設備內建、隨設備更新的系統軟體為 `firmware`；可安裝在一般伺服器上的作業系統與 hypervisor 為 `os`；管理或網路服務軟體為 `tool`；執行環境、框架、CMS 與函式庫為 `app`。標 ※ 者為邊界判斷，使用者可在 `catalog.d` 覆寫。

| 區段 | role | 產品 key |
|---|---|---|
| 網路／資安 | `firmware` | `paloalto-panos`、`hpe-aoscx`、`hpe-aos-switch`、`hpe-arubaos`、`cisco-ios`、`cisco-iosxe`、`cisco-nxos`、`cisco-asa`、`cisco-ftd`、`juniper-junos`、`arista-eos`、`mikrotik-routeros`、`fortinet-fortios`、`sonicwall-sonicos`、`sophos-firewall`、`watchguard-fireware`、`checkpoint-gaia`、`ivanti-connect-secure`、`f5-bigip`、`citrix-netscaler`、`ruckus-unleashed`、`ruckus-smartzone`※（控制器可為實體或虛擬設備，仍視為設備系統軟體） |
| 網路／資安 | `security` | `fortinet-forticlient`（端點 VPN／安全用戶端）、`pixis`※（IP 管理與資產盤點，對應 SW06 描述的「資產盤點」） |
| 網路／資安 | `tool` | `ubiquiti-unifi-network`※（安裝在主機或控制器上的網路管理應用程式） |
| 虛擬化／伺服器 | `os` | `vmware-esxi`、`proxmox-ve`（SW02 描述含 VMware 等作業平台） |
| 虛擬化／伺服器 | `tool` | `vmware-vcenter`※（管理軟體；SW05 描述含「虛擬主機軟體」） |
| 虛擬化／伺服器 | `bmc` | `dell-idrac9` |
| 虛擬化／伺服器 | `bios` | `dell-poweredge-bios` |
| 儲存 | `firmware` | `synology-dsm`、`qnap-qts`、`qnap-quts-hero`、`asustor-adm`、`netapp-ontap`、`qsan-sanos` |
| 儲存 | `os` | `truenas`※（多安裝在自建伺服器上） |
| 備份 | `suite` | `veeam-backup-replication`、`nakivo-backup-replication`、`acronis-cyber-protect`、`commvault`、`veritas-netbackup`、`veritas-backup-exec`（SW07 描述含「備份軟體」） |
| 網站與資料庫 | `web` | `nginx`、`apache-httpd`、`apache-tomcat` |
| 網站與資料庫 | `dbms` | `mariadb`、`mysql`、`postgresql`、`mssql-server`、`mongodb`、`redis` |
| 執行環境與框架 | `app` | `ruby`、`rails`、`php`、`python`、`nodejs`、`java-se`、`django`、`laravel`、`wordpress`、`drupal` |
| 前端函式庫 | `app` | `ckeditor4`、`tinymce`、`bootstrap`、`jquery`、`jquery-ui`、`lodash`、`momentjs` |
| 系統元件 | `app` | `openssl`※（函式庫） |
| 系統元件 | `tool` | `openssh`、`samba`、`bind9`、`freeradius`※（主機上的網路服務，比照 SW05 的 FTP 軟體） |
| 作業系統 | `os` | `debian`、`ubuntu`、`rhel`、`windows-server` |

合計：`firmware` 28、`bios` 1、`bmc` 1、`os` 7、`dbms` 6、`web` 3、`tool` 6、`security` 2、`suite` 6、`app` 18，共 78 項，沒有 `other`。

### D4 frontend 分組：`component_groups.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComponentGroup { Role(ComponentRole), Packages }   // 衍生的 Ord 即顯示順序（套件最後）

impl ComponentGroup { pub fn label(self) -> &'static str }  // 角色中文名稱或「套件」

pub const PACKAGES_LABEL: &str = "套件";

/// 目錄中已沒有的 key 歸入 Role(Other)。
pub fn group_of(catalog: &Catalog, key: &str, package: bool) -> ComponentGroup;

/// 依固定順序分組；組內維持輸入順序；空組不輸出。
pub fn group_components<'a>(catalog: &Catalog, components: &'a [Component])
    -> Vec<(ComponentGroup, Vec<&'a Component>)>;

/// 編輯草稿：回傳原索引，讓錯誤、焦點與移除仍以列索引對應。
pub fn group_drafts(catalog: &Catalog, drafts: &[ComponentDraft])
    -> Vec<(ComponentGroup, Vec<usize>)>;
```

- `Component` 以 `ComponentKind::Package` 判斷套件；`ComponentDraft` 以 `package` 旗標判斷（與現有唯讀規則一致）
- 兩個函式共用一個內部函式：先算每個項目的組，再以穩定排序（`sort_by_key`）依組排列後切段，不改變輸入切片（不可變）
- 放在 frontend 而非 core：套件組是顯示概念；CLI 已相依 frontend（`asset_form`），三個前端都能共用
- 替代方案：在 core 的 `Component` 加 `role()`。元件不持有目錄，需要每次傳 `Catalog`，且套件組不是目錄角色，放 core 會混入顯示規則

### D5 GUI

- 詳情 `components_list`：保留標題「元件（N）」；每組先以 `ui.label(RichText::new(group.label()).strong())` 顯示組名，元件與版本歷史以 `ui.indent` 縮排在組名下。產品 key 的滑鼠提示與版本歷史格式不變
- 編輯對話框 `components`：`Grid` 改為依 `group_drafts` 輸出，每組先插入一列組名（第一欄粗體、其餘兩欄空白），接著該組各列；`component_row`、`errors.component(i)`、`focus_version`、`dropped` 都沿用原索引 `i`，因此加入元件後的焦點流程與錯誤對應不變
- 加入元件仍是 `form.components.push`（加在最後），顯示時才依組排列；儲存邏輯與 `AssetForm` 不變
- 組名只是標籤，不加入 Tab 焦點順序
- 未辨識產品區塊不變（不分組）

### D6 TUI

- 元件表格欄位改為「角色」`Width::Fixed(14)`（最長「資料庫管理系統」7 個全形字＝14 欄）、「元件」`Flex(1)`、「版本」20、「來源」8、「更新時間」16；列的順序依 `group_components` 展開，每列第一欄為 `group.label()`
- 元件欄仍為 `c.label()`（產品 key／purl），不在本 change 改為顯示名稱（proposal Out of scope）
- 更新 `assets_pending__asset_detail_shows_components_findings_and_unresolved.snap`；新增一個多組元件的快照或斷言測試

### D7 CLI

- `asset show`：多載入一次目錄（`app.load_catalog()?`），「元件：」之下每組輸出 `  {組名}：`，組內元件 `    {label}  {version}（{source}）`、版本歷史 `        先前 …`（比原本多縮排兩格）；未辨識產品仍在最後以 `  未辨識：…` 輸出
- 輸出行由純函式 `component_lines(catalog, components, history) -> Vec<String>` 產生，便於單元測試
- `catalog_list`：在 `scheme` 欄之後加 `role` 欄（`{:<9}`，輸出 `as_str()` 的英文值，保持對齊並與 `catalog.d` 寫法一致）
- `print_product`：在「版本格式」之後輸出 `  角色：{label}（{as_str}）`

### D8 文件

- `docs/user-guide/assets.md`：在 `catalog.d` 一節加入 `role` 欄位、可用值與中文名稱表、覆寫範例（以 `example-fw` 等虛構 key 示範）、未宣告為「其他」、寫錯值會使目錄載入失敗；產品清單加註角色（或說明以 `asset catalog list` 查看）
- `docs/user-guide/gui.md`：資產詳情與編輯對話框的元件依角色分組、套件另成一組

## Risks / Trade-offs

- [舊版程式讀到含 `role` 的 `catalog.d`] `deny_unknown_fields` 會讓舊版載入失敗 → 只在回復舊版時發生；Migration Plan 說明回復前移除 `role` 行
- [邊界產品的角色見仁見智（D3 的 ※）] → 角色只影響顯示，使用者可在 `catalog.d` 覆寫；使用說明列出覆寫範例
- [`asset show` 改為需要載入目錄] `catalog.d` 有錯時 `asset show` 也會失敗 → 與 `asset import`、`match` 一致，錯誤訊息已指出檔案與產品；不另做降級顯示
- [只有 `role` 逐來源驗證，`version_scheme` 仍指向原始來源] 兩種錯誤的訊息來源不一致 → 屬既有問題，列入 Non-Goals；若要一併修正，另開 change
- [GUI 詳情與編輯對話框多了組名列，畫面變長] 單一元件的資產也會多一行組名 → 保持一致性；組名用粗體小標，不加框線
- [kittest 以文字查詢節點，組名可能與其他標籤同名（例如子類別下拉選單的「SW02 作業系統」）] → 組名不含代碼，與「代碼 名稱」格式不同；測試以組名 + 元件列的位置關係斷言

## Migration Plan

不需資料庫 migration。升級後第一次載入目錄即帶有角色；目錄指紋不變，不會觸發全量重新比對。回復舊版時，若 `catalog.d` 已寫 `role`，需先刪除該行（舊版不認得此欄位，會拒絕載入）。

## Apply 時的決定（/opsx-auto，依建議處理）

- **20 封存後的插入位置核對**：GUI 詳情的 `components_list` 仍在識別欄位之後、獨立成區塊；TUI 詳情的元件表格仍在資訊列（含 20 新增的識別兩行）之下。D5、D6 的插入位置不需調整
- **core 測試檔**：`crates/core/tests/catalog.rs` 已 479 行，角色相關測試（任務 1.3、2.3 與 6.3 的範例驗證）改放新檔 `crates/core/tests/catalog_roles.rs`，避免單檔過長；完成條件改以 `cargo test -p cvewatch-core --test catalog_roles` 驗證，`--test catalog` 仍全部通過
- **CLI 的 `component_lines`**：放在新模組 `crates/cvewatch/src/commands/component_view.rs`（`asset_view.rs` 已近 600 行），簽章多一個 `unresolved` 參數，讓「未辨識產品在所有組之後」也能以純函式測試
- **CLI 目錄測試**：新增 `crates/cvewatch/tests/asset_catalog_roles.rs`（`assert_cmd`、暫存資料目錄的 `catalog.d`，不開資料庫）；`asset catalog`（不帶子命令）也驗證角色欄
- **使用說明**：`assets.md` 新增「元件角色」一節，除了值與名稱，另以表格列出每個角色的內建產品（spec Seed catalog roles 要求與使用說明的產品清單一致）；範例 TOML 由 `catalog_roles.rs` 直接讀取使用說明的同一段驗證
- **`group_components` 判斷套件**：依 D4 以 `ComponentKind::Package` 判斷；草稿依 `package` 旗標（`!is_inventory_product()`）。目前只有 SBOM／dpkg／rpm 會產生套件元件，兩者一致
- **GUI 詳情的縮排 id**：`ui.indent` 以 `("component-group", 組名)` 為 id，避免與其他區塊衝突

## Code review 後的調整

- **D1 API**：`names()` 改為 `allowed() -> String`（以「、」串接，與 `string_enum!` 的 `allowed` 同名同義），並實作 `Display`；以編譯期斷言確保 `ALL` 列出每個值。不改用 `string_enum!`：其 `parse` 會 trim 並忽略大小寫，與 D1 的完全比對衝突，且缺 `Ord`、`Default`
- **D2 驗證**：`validate_role` 與 `build_product` 共用 `product_role`，值不在清單時一律回傳錯誤，不再以 `unwrap_or_default` 靜默改為「其他」
- **D4 分組**：`group_indices` 改以 `BTreeMap<ComponentGroup, Vec<usize>>` 收集（與 `groups.rs` 的資產說明分組相同做法），取代排序後切段
- **D5 編輯對話框**：元件 `Grid` 改為不加條紋（`striped(false)`）：組名列會打亂交替條紋，改以粗體組名區隔
- **D7 `asset show`**：目錄改在開啟資料庫與輸出任何內容前載入，`catalog.d` 有錯時直接失敗、不留下半份輸出（新增 `asset_show_fails_before_output_on_bad_role` 測試）；版本歷史改以新增的 `ComponentRepo::history_for_asset`（單一 JOIN 查詢）取得，不再逐元件查詢
- **未調整（取捨）**：
  - TUI 角色欄固定 14 欄會使 80 欄終端機的元件欄變窄：spec TUI component roles 要求每列第一欄顯示組名，縮窄會截斷「資料庫管理系統」，改用組名列則違反 spec；維持 D6
  - 種子測試寫死 78 項與各角色數量：任務 2.3 明定以 D3 表格逐項斷言數量，作為種子與 design 一致的守門；日後增修種子產品時一併更新 D3 與此測試

## 架構圖

架構圖：不需更新。apply 後以 `git diff HEAD` 確認：沒有 workspace `members` 或 `crates/*/Cargo.toml` 內部 `path =` 相依的變更、沒有新增或移除執行檔，也沒有新增外部系統或本機服務的互動；變更只在 core 目錄模組（`catalog/role.rs`）、frontend 顯示模組（`component_groups.rs`）、三個前端的畫面與 CLI 輸出（cli 已相依 frontend）。
