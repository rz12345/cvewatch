# Tasks

前提：

- 19-system-code-edit、20-asset-identity-network、21-component-roles 都已 archive。本 change 的 delta spec 全部為 ADDED，沒有 MODIFIED 區塊；apply 前仍先依 21 封存後的主 spec 重新核對：`asset-inventory`、`product-catalog`、`gui-console`、`tui-console`、`email-alerts` 沒有同名 requirement，且 ADDED 內容與 20、21 改過的 requirement（範本、匯出、缺欄保留、資產詳情、目錄格式）用語一致；若 20 已把範本／匯出寫成「列出所有工作表」的形式，改在本 change 補一個 MODIFIED 區塊（整段複製封存後的主 spec 再加入 `eol` 工作表與三個日期欄），並記入 design.md
- 依賴 20 的「廠牌」「型號」欄位：硬體 EoL 的比對（design D5）使用 20 實際的識別字與正規化方式；apply 前讀 20 封存後的 design，把 design D5、D6 中暫稱的 `brand`、`model` 與 migration 編號改成實際值
- 21 會在 `ProductDef` 新增角色欄位；本 change 的目錄欄位與合併規則（design D4）在 21 的版本上加入
- 測試不呼叫真實外部 API、不存取真實 Credential Manager；endoflife.date 一律用 `tests/fixtures/endoflife/` 與 wiremock。fixture、測試與文件只用虛構型號（例如 `Aruba XYZ-100`）與資產編號（FW-EDGE、SRV-01、SW-CORE、NAS-01）；真實型號只放使用者本機 `data/catalog.d/`。commit 前執行 `scripts/check-sensitive.ps1`
- 每完成一個任務群組：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test`，並勾選本檔

使用者已確認（2026-11-09）：

- 不建 Dell iDRAC9 的種子 EoL（其 EoL 依 PowerEdge 世代而非韌體分支訂定），由伺服器型號的硬體 EoL（本機 `catalog.d`）涵蓋；種子目錄的 EoL 宣告只建 Synology DSM
- endoflife.date 未涵蓋的其他產品本 change 不建種子，使用者需要時自行寫 `catalog.d`
- 依建議進行（apply 時記入 design.md）：軟體元件覆寫採「元件層級的日期＋來源」，清冊以 `eol` 工作表記錄（design D7）；已到期的保固／合約也標示「已到期」並納入「保固或合約到期」篩選

## 1. core：目錄格式

- [ ] 1.1 `catalog/file.rs`：`ProductDef` 新增 `endoflife: Option<String>`、`eol_cycles: Option<Vec<EolCycleDef { cycle, eol, source }>>`；`catalog/mod.rs` 的 `Product` 新增 `endoflife`、`eol_cycles`，`merge_into` 依 design D4（`""` 取消代稱、分支依 `cycle` 合併），載入時驗證代稱、日期與網址。完成條件：單元測試重現 spec 的 Declaring a slug locally、Disabling a seed slug、Malformed slug、Adding a cycle locally、Correcting a seed date、Missing source URL；`Catalog::fingerprint` 在只改這兩個欄位時不變
- [ ] 1.2 `[[hardware]]`：`HardwareDef`、`Catalog::hardware()` 與 `(vendor, model)`／別名索引（design D4、D5），相同型號後載入者覆寫，別名衝突拒絕載入。完成條件：單元測試重現 Declaring a model locally、Conflicting model alias；`Catalog::seed().hardware()` 為空（Seed has no models）；`catalog/mod.rs` 若超過 600 行，硬體部分拆到 `catalog/hardware.rs`
- [ ] 1.3 `cvewatch asset catalog show <key>` 輸出代稱與目錄宣告的分支表（design D4）。完成條件：`crates/cvewatch` 的輸出函式單元測試斷言有代稱、沒有代稱、有分支表三種情況
- [ ] 1.4 測試：`cargo test -p cvewatch-core --test catalog` 與 `cargo test -p cvewatch` 通過

## 2. core：資料模型與日期

- [ ] 2.1 migration（20 之後的下一號，預期 `0010_lifecycle.sql`）依 design D6 新增欄位與 `eol_product`、`eol_release` 表，並在 `migrate.rs` 登錄。完成條件：新增 `tests/db_migration_lifecycle.rs`，以前一版資料庫建立 `FW-EDGE`（2 筆命中）與 retired 的 `NAS-02`，升級後斷言三個日期與覆寫為 NULL、命中數不變（spec Upgrading keeps assets unchanged）
- [ ] 2.2 新檔 `assets/dates.rs`：`parse_date`（`YYYY-MM-DD`、`YYYY/MM/DD`，驗證日期存在）與錯誤訊息；`sheet.rs::cell_text` 的 `Data::DateTime` 轉為 `YYYY-MM-DD`（design D8），先以 grep 確認沒有其他欄位依賴舊輸出。完成條件：單元測試涵蓋 `2027/12/31`、`2027-1-5`、`2027-02-30`、`27/1/1`、空白，以及 `Data::DateTime` 儲存格轉為 `2027-12-31`
- [ ] 2.3 `model.rs`：`AssetRecord` 新增三個日期與 `hw_eol_override`，`Component` 新增 `eol_override`（`EolOverride { date, source }`）；`repo.rs` 讀寫，覆寫兩欄成對寫入與清除；元件軟刪除時清除其覆寫（design D6）。完成條件：`tests/assets_repo.rs` 新增案例斷言讀寫往返、移除元件後覆寫為 NULL、修改版本後覆寫保留（spec Override follows a version change）
- [ ] 2.4 測試：`cargo test -p cvewatch-core` 通過（同時修正因 `AssetRecord` 新欄位而需要補值的既有測試）

## 3. core：清冊、編修、範本與匯出

- [ ] 3.1 `columns.rs`：新增 `WarrantyEnd`、`SupportEnd`、`LicenseEnd`（欄名 `warranty_end`、`support_end`、`license_end`），加入 `ASSET_COLUMNS` 與 `KEEPABLE_FIELDS`；新增 `SheetKind::Eol`（`asset_tag`、`product`、`eol`、`source`）；`sheet.rs` 讀取選填的 `eol` 工作表。完成條件：單元測試斷言表頭解析、`eol` 工作表名稱不分大小寫、CSV 清冊的 `eol` 為 None
- [ ] 3.2 `record.rs`／`record_admin.rs`（或新檔 `record_lifecycle.rs`，維持各檔 < 800 行）解析三個日期；新檔 `validate_eol.rs` 驗證 `eol` 工作表（design D7）。完成條件：單元測試重現 spec 的 Recording warranty and support dates、Invalid date、Override without a source、Override for a product the asset does not have，以及同一元件兩列的錯誤
- [ ] 3.3 `plan.rs`／`plan_fields.rs`／`import.rs`：日期欄位變更列入欄位差異；覆寫的新增、修改、移除列入預覽；缺欄或缺工作表時保留既有值並給一則提醒（沿用 `kept_fields`）。完成條件：`tests/assets_import_lifecycle.rs` 重現 Importing overrides、Re-importing an inventory without lifecycle data、Overrides removed in the sheet
- [ ] 3.4 `export.rs`、`template.rs`：寫出三個日期欄與 `eol` 工作表；範本範例列依 design D7。完成條件：測試重現 Round trip with lifecycle data；範本有 `eol` 工作表，且範本本身以 `--dry-run` 匯入沒有錯誤
- [ ] 3.5 `edit.rs`：新增與編修資產時可設定或清除日期與覆寫，與其他欄位同一交易，不標記「比對待補」。完成條件：`tests/asset_edit.rs` 新增 Setting a support date on screen、Clearing a hardware override，並斷言 `match_stale` 未被設定
- [ ] 3.6 fixtures：清冊 fixture 產生器加入含三個日期欄與 `eol` 工作表的清冊（虛構資料）。完成條件：重新產生後 `git diff --stat tests/fixtures` 只列出預期檔案，`scripts/check-sensitive.ps1` 沒有命中
- [ ] 3.7 測試：`cargo test -p cvewatch-core` 通過

## 4. core：`eol` 同步來源

- [ ] 4.1 `endpoints.rs` 新增 `endoflife`（預設 `https://endoflife.date/api/v1`，`with_base` 為 `{base}/eol`）與 `product_url(slug)`；`config.rs` 的 `SourcesConfig` 新增 `eol_enabled`（預設 true）；`sync.rs` 的 `SOURCE_NAMES`、`SourceKind::Eol`、`plan` 排在 `osv` 之後（design D2）。完成條件：`sync.rs` 單元測試的預設順序以 `eol` 結尾、停用時略過但 `--source eol` 仍執行
- [ ] 4.2 錄製 fixture：以 `curl` 取得 `panos`、`windows-server`、`esxi` 的真實回應後裁剪（design D14），另做 `no-eol-date.json` 與 `not-found.html`。完成條件：`tests/fixtures/endoflife/` 每個檔案 < 20 KB、包含一個未知欄位，檔頭 `_note` 說明來源日期與改寫內容
- [ ] 4.3 新檔 `ingest/eol.rs`：回應的 serde 結構（只取 design D1 列出的欄位、忽略未知欄位）與轉換成 `eol_release` 列。完成條件：單元測試以 4.2 的 fixture 斷言分支名稱、`eolFrom` 為 null、`isEol`、`latest.name`、選填欄位缺席時為 None
- [ ] 4.4 `eol::sync`：選出代稱（design D2）、依序送出條件請求並間隔 `EOL_REQUEST_INTERVAL`、每個代稱各自交易、304／404／失敗處理與錯誤彙整（design D1），提交一律 `ctx.commit(tx, false)`。完成條件：`cargo build -p cvewatch-core` 通過，`eol.rs` < 400 行
- [ ] 4.5 測試：新增 `tests/eol_sync.rs`（wiremock）重現 spec 的 Refreshing EoL data only、endoflife.date unavailable（其他代稱已提交、來源失敗、結束碼 4）、Unknown product slug、Products without components are skipped，並涵蓋 304 帶 `If-None-Match`、301 轉址、不設定「比對待補」；`cargo test -p cvewatch-core --test eol_sync` 通過

## 5. core：生命週期判定

- [ ] 5.1 新模組 `lifecycle/cycle.rs`：design D3 的 `tokens`、`token_eq`、規則一與規則二、ESXi build 轉換。完成條件：單元測試重現 spec 的 Version within a cycle、Longest cycle wins、Cycle found through the latest version、No matching cycle，以及 design D3 列出的每一個對應例（含 `1` 不等於 `10`、SQL Server `13.0-sp3` 不符合）
- [ ] 5.2 `lifecycle/hardware.rs`：design D5 的廠牌與型號比對。完成條件：單元測試重現 Model listed in the catalog、Alias of a model、Model not in the catalog，與只有型號沒有廠牌時為「未填廠牌或型號」
- [ ] 5.3 `lifecycle/evaluate.rs`、`model.rs`：`evaluate`、軟體與硬體的判定順序、`ItemState`／`Status`（design D9）；`config.rs` 新增 `[lifecycle] reminder_days`（預設 90，驗證 1–3650）並更新 `init` 的範例設定註解。完成條件：單元測試重現 EoL from endoflife.date、Catalog entry for a product not covered、Component override wins、Pending version、Hardware override、EoL within the reminder window、Warranty already expired、Shorter reminder window、Reminder days out of range；套件元件與未辨識產品不產生項目
- [ ] 5.4 `lifecycle/summary.rs`：`load_eol`、`summary`（只含 active 資產的已 EoL、即將 EoL、到期數）與摘要用的 `upcoming`／`reached`／新事項判定（design D12）。完成條件：單元測試重現 Retired asset is not reminded，並以固定日期斷言「期間內進入提醒天數」與「期間內到期」的邊界（剛好第 90 天、期間起點當天）
- [ ] 5.5 測試：`cargo test -p cvewatch-core` 通過；`lifecycle/` 各檔 < 400 行

## 6. 種子目錄 EoL 資料

- [ ] 6.1 endoflife.date 代稱：對 design D15 列出的每個代稱手動執行 `curl -sS -o NUL -w "%{http_code}" https://endoflife.date/api/v1/products/<slug>`（只在此查證時呼叫真實 API，測試不呼叫），確認 200 且分支名稱與種子產品的版本寫法能依 design D3 對應；Java SE 決定對應 `oracle-jdk` 或不宣告；寫入 `seed.toml`，每個產品的註解寫查證日期。完成條件：design D15 的每個產品都在 `seed.toml` 有 `endoflife` 或註明不宣告的理由；單元測試斷言種子代稱都符合格式、`paloalto-panos` 的代稱為 `panos`（spec Seed products with slugs）
- [ ] 6.2 查證原廠公告並建立 `eol_cycles`：Synology DSM（https://www.synology.com/en-global/products/status 的 Software 分頁，及其連結的 DSM 生命週期公告）。每筆分支的 EoL 日期必須在公告原文中找得到，`source` 填公告網址，註解寫查證日期與公告標題；**不得以推測或第三方網站的日期填入**。不建 Dell iDRAC9 的種子（使用者已確認）。完成條件：`cvewatch asset catalog show synology-dsm` 列出分支、日期與網址（spec Seed product with catalog dates）；design D15 記錄每筆資料的查證日期與公告標題
- [ ] 6.3 測試：`cargo test -p cvewatch-core --test catalog` 通過，並斷言種子沒有 `[[hardware]]`、每筆 `eol_cycles` 有 `https` 來源；使用說明（README 或 `docs/`）的種子 EoL 涵蓋清單與 `seed.toml` 一致

## 7. 每日摘要與 CLI

- [ ] 7.1 `notify/digest.rs`：`DigestData.lifecycle`、`collect` 新增 `catalog` 與 `window` 參數、`is_empty()` 的新事項條件；`notify/content.rs` 新增「生命週期提醒」段落（design D12）；CLI `notify digest` 傳入目錄與設定。完成條件：`tests/notify_email.rs` 新增案例重現 spec 的 A support contract enters the reminder window、Reminder already sent before、Hardware reached end of life during the period；沒有任何項目時段落不出現
- [ ] 7.2 CLI `asset show` 新增生命週期段落（design D13，純函式）。完成條件：`commands/asset_view.rs` 單元測試重現 spec 的 Showing lifecycle items，並涵蓋「未填廠牌或型號」「無 EoL 資料」「版本待補，無法判斷 EoL」
- [ ] 7.3 `sync` 結束摘要列出 `eol` 的提醒（404 代稱）；`cvewatch sync --help` 的來源清單含 `eol`。完成條件：`crates/cvewatch/tests` 斷言 `sync --help` 含 `eol`；摘要輸出函式單元測試斷言提醒文字
- [ ] 7.4 測試：`cargo test -p cvewatch-core` 與 `cargo test -p cvewatch` 通過

## 8. frontend

- [ ] 8.1 `Settings` 新增 `reminder_days`；`data.rs` 快照載入 `EolData`，`AssetEntry.lifecycle`，跨日重算；`AssetListFilter.lifecycle`（design D11）。完成條件：單元測試斷言三種篩選的結果、可與 `pending_only`、`include_retired` 同時使用、retired 資產不符合任何生命週期篩選
- [ ] 8.2 `asset_form/lifecycle.rs`：三個日期與硬體覆寫、元件覆寫的表單欄位、解析與欄位錯誤；`Workspace` 儲存時一併寫入。完成條件：`crates/frontend/tests/asset_edit.rs` 新增案例：`2027/12/31` 儲存為 2027-12-31、`2027-13-01` 回傳該欄位的錯誤且資料不變、覆寫缺來源時回傳錯誤
- [ ] 8.3 測試：`cargo test -p cvewatch-frontend` 通過

## 9. GUI

- [ ] 9.1 `screens/asset_list.rs`：名稱後的生命週期標示（常數與警示色）、工具列的生命週期篩選下拉選單（design D11）。完成條件：kittest 重現 spec 的 Marker in the list、Filtering expiring contracts
- [ ] 9.2 新檔 `screens/lifecycle_view.rs`：詳情的「生命週期」區塊、來源網址以 `Workspace::open_url` 開啟。完成條件：kittest 重現 Opening the source of an EoL date（`FakeOpener` 收到網址）與 Unmapped version in the detail pane
- [ ] 9.3 新檔 `dialogs/asset_lifecycle.rs`：資產對話框的生命週期區塊；`asset_components.rs` 的元件列加入「EoL 覆寫…」。完成條件：kittest 重現 Entering a warranty date、Invalid date in the dialog、Component override in the dialog；`asset_edit.rs`、`asset_components.rs` 仍 < 800 行
- [ ] 9.4 `screens/dashboard.rs`：生命週期數字列與 `Jump::Lifecycle`；三者為 0 時不顯示。完成條件：kittest 重現 Jumping to EoL assets，並斷言全為 0 時沒有此列
- [ ] 9.5 測試：`cargo test -p cvewatch-gui` 全部通過（手動目視檢查移到 Workflow follow-up）

## 10. TUI

- [ ] 10.1 `ui/screens.rs` 的資產表格新增「週期」欄；`app.rs` 新增 `assets_lifecycle_filter`、`l` 鍵循環切換、`visible_assets()` 與標題（design D11）。完成條件：畫面測試重現 spec 的 Filtering EoL assets in the console，並斷言篩選後 Enter 開啟的是篩選清單中的資產
- [ ] 10.2 `asset_detail` 新增生命週期段落；`dashboard` 新增數字行（全為 0 時不顯示）；`?` 說明加入 `l`。完成條件：畫面測試重現 Lifecycle in the asset detail、Lifecycle counts on the dashboard
- [ ] 10.3 測試：`cargo test -p cvewatch-tui` 通過

## 11. 文件與整合驗證

- [ ] 11.1 README／`docs/`：三個日期欄與 `eol` 工作表、`[lifecycle] reminder_days`、`[sources] eol_enabled`、`cvewatch sync --source eol`、目錄的 `endoflife`／`eol_cycles`／`[[hardware]]` 寫法（只用虛構型號 `Aruba XYZ-100` 示範）、版本分支對應規則摘要與「無法對應」時改用元件覆寫。完成條件：文件中的欄名與 `cvewatch asset template` 產出的表頭一致；`scripts/check-sensitive.ps1` 沒有命中
- [ ] 11.2 全專案檢查：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`openspec validate 22-asset-lifecycle-eol --strict`。完成條件：四個指令皆成功

## Workflow follow-up

- 使用者手動啟動 `cvewatchw.exe`，確認資產清單的生命週期標示、篩選、詳情區塊與對話框的生命週期區塊中文顯示正常、版面沒有被截斷
- 使用者把本機真實硬體型號的 EoL 寫入 `data/catalog.d/`（不入版控），執行一次 `cvewatch sync --source eol` 確認結果
- 依 `/opsx-auto` 流程判斷架構變更：新增外部整合 endoflife.date 與 core 模組 `lifecycle/`，需以 Archify 更新 `docs/architecture/` 並另行 commit
- archive 時同步主 spec：新增 `asset-lifecycle`（Software end-of-life sync、Software release cycle mapping、Software end-of-life determination、Hardware end-of-life determination、Lifecycle status、Lifecycle in asset show）；`asset-inventory` 新增 Lifecycle date fields、EoL overrides、Lifecycle columns and EoL sheet、Editing lifecycle fields；`product-catalog` 新增 endoflife.date product declaration、Catalog software end-of-life dates、Hardware model lifecycle catalog、Seed end-of-life coverage；`gui-console` 新增 GUI lifecycle fields input、GUI lifecycle markers and filter、GUI lifecycle in asset detail、GUI dashboard lifecycle reminder；`tui-console` 新增 Lifecycle in the console；`email-alerts` 新增 Lifecycle reminders in the daily digest
- archive 後執行 `openspec validate --all --strict` 確認通過
- 之後 propose／apply 23-isms-export
