# Design

## Context

- 同步來源：`ingest/sync.rs` 以固定順序 `cvelist → nvd → csaf:* → kev → epss → osv` 執行，`SOURCE_NAMES` 決定 `--source` 可用的名稱；每個來源自己開交易、經 `SyncCtx::commit(tx, data_changed)` 提交（`data_changed` 為真才標記「比對待補」），`runlog` 寫 `sync_run`，單一來源失敗只記錄、不中止，最後結束碼 4。端點集中在 `ingest/endpoints.rs`，測試以 `Endpoints::with_base` 指向 wiremock。游標存在 `sync_cursor`（`ingest/cursor.rs`，JSON）
- HTTP：`http.rs` 的 `HttpClient` 統一 User-Agent、逾時、429/5xx/連線錯誤的指數退避並遵守 `Retry-After`（超過 15 分鐘放棄）；reqwest 預設跟隨轉址、讀取 `HTTPS_PROXY`。KEV 已有 ETag／`If-Modified-Since` 條件請求的先例
- 產品目錄：`catalog/file.rs` 的 `CatalogFile { product, esxi_build }`、`ProductDef`（`deny_unknown_fields`）；`catalog/mod.rs` 載入「內建種子 → `catalog.d\*.toml`（檔名排序）」，同 key 有寫的欄位覆寫、`aliases` 合併；`esxi_build` 以後載入者為準。`Catalog::fingerprint` 只含影響比對的欄位。21-component-roles 會在 `ProductDef` 新增角色欄位
- 資產：`AssetRecord`（`assets/model.rs`）、清冊欄位 `assets/columns.rs`（`ASSET_COLUMNS`、`KEEPABLE_FIELDS`、`SheetKind`）、讀檔 `sheet.rs`（calamine，`cell_text` 目前把 `Data::DateTime` 直接 `to_string`）、驗證 `validate.rs`／`record.rs`／`record_admin.rs`、計畫 `plan.rs`／`plan_fields.rs`、寫入 `import.rs`、匯出 `export.rs`、範本 `template.rs`。缺欄保留既有值的機制為 `Inventory.kept_fields`（16 design D4）。20 會新增廠牌、型號等欄位與 `ips` 工作表（另一個選填工作表的先例）
- 摘要：`notify/digest.rs` 的 `DigestData` 與 `is_empty()`；18 的「版本待補」是「列出但不算新事項」的先例。Dashboard 數字來自 `matching/dashboard.rs`，GUI／TUI 經 frontend 快照讀取
- 畫面：frontend `data.rs` 的 `AssetListFilter { include_retired, pending_only, class }`；GUI `screens/asset_list.rs`（工具列、分組清單、`PENDING_MARK`）、`screens/assets.rs`／`detail.rs`、`screens/dashboard.rs`（`Jump::PendingVersions` 跳到資產畫面）、`dialogs/asset_edit.rs`／`asset_admin.rs`／`asset_components.rs`；TUI `ui/screens.rs` 的 `assets`、`asset_detail`、`dashboard`，`app.rs` 的 `on_assets_key`（目前沒有篩選，游標直接索引 `ws.data.assets`）。開啟網址有 `Workspace::open_url` 與 `Opener` 替身
- 時間：cvewatch 其他時間為 RFC 3339 UTC；EoL 與到期日是「日曆日」，與使用者所在時區有關

## Goals / Non-Goals

**Goals:**

- 生命週期判定是 core 的純函式（輸入：資產、元件、目錄、EoL 資料、今天、提醒天數），GUI、TUI、CLI 與摘要共用，結果可解釋（列出來源與原因）
- endoflife.date 同步與其他來源一樣可單獨執行、失敗隔離、可用 fixture＋wiremock 測試
- 版控只放虛構型號；種子 EoL 日期只來自查證過的原廠公告
- 不影響比對：EoL 資料與生命週期欄位的變更都不標記「比對待補」

**Non-Goals:**

- 不把 EoL 納入風險評分或優先序
- 不保存生命週期日期的變更歷史
- 不新增 crate 相依（日期用既有的 chrono）

## Decisions

### D1. endoflife.date API（2026-11-09 以官方文件與實際請求確認）

官方文件 https://endoflife.date/docs/api/v1/ （Swagger UI，規格檔 `https://endoflife.date/docs/api/v1/openapi.yml`，API 版本 1.2.1）。

| 項目 | 內容 |
|---|---|
| Base URL | `https://endoflife.date/api/v1` |
| 使用端點 | `GET /products/{slug}`：產品全部資料，含所有版本分支。實測有無結尾 `/` 都回 200 |
| 不使用 | `/products`（摘要清單，只在查證種子代稱時手動使用）、`/products/full`（全部資料，過大）、`/products/{slug}/releases/{release}`、`/products/{slug}/releases/latest`（逐分支查詢會增加請求數） |
| 驗證 | 無（不需 API key，沒有秘密） |
| 回應 | `ProductResponse { schema_version, generated_at, last_modified, result: ProductDetails }`；`result.releases[]` 為 `ProductRelease` |
| `ProductRelease` 必有欄位 | `name`（分支名稱，例如 `22.04`、`11.1`、`2022`、`13.0-sp3`）、`codename`、`label`、`releaseDate`（date）、`isLts`、`ltsFrom`、`isEol`（bool）、`eolFrom`（date 或 null：日期未知）、`isMaintained`、`latest`（`{ name, date, link }` 或 null） |
| 選填欄位 | `isEoas`/`eoasFrom`（主動支援結束）、`isEoes`/`eoesFrom`（延伸支援結束）、`isDiscontinued`/`discontinuedFrom`（硬體用）、`custom`；產品沒有該階段時欄位**不出現** |
| 其他 `result` 欄位 | `name`、`label`、`aliases`、`category`、`tags`、`identifiers`、`labels`（各階段的名稱，例如 `eol` 為 "Security Support"）、`links.html`（產品頁）、`links.releasePolicy` |
| 相容性 | 文件明定：同一主版本 `/api/v1` 向後相容，用戶端須跟隨 301、容許新增欄位與新的列舉值；文件外的欄位不可依賴 → 解析時忽略未知欄位、只依賴上列欄位 |
| 狀態碼 | 200；301（產品改名，`Location` 為新網址，實測 `/products/pan-os` → `/products/panos/`）；304（條件請求未變更）；404（產品不存在，**回應是 HTML 錯誤頁而非 JSON**）；429（請求過多，可能帶 `Retry-After`）；託管於 Netlify，可能出現 502／503 |
| 速率限制 | 文件**沒有公布數值**，只說明可能回 429 與 `Retry-After` |
| 快取 | 回應帶 `ETag`、`Cache-Control: public,max-age=0,must-revalidate`；實測帶 `If-None-Match` 回 304（`If-Modified-Since` 也回 304，但回應沒有 `Last-Modified` header） |
| 編碼 | UTF-8 |

**請求策略**：每次 `eol` 執行對每個需要的代稱送一次 `GET /products/{slug}`，帶上次的 `If-None-Match`；請求依序送出、彼此間隔 `EOL_REQUEST_INTERVAL = 250 ms`（無公布限制下的禮貌間隔，代稱數量預期在 50 個以內，一次執行約十數秒以內）；429 與 5xx 由 `HttpClient` 的退避與 `Retry-After` 處理。

**失敗處理**：

- 304 → 保留本機資料，只更新 `fetched_at`
- 200 → 解析成功後，在該代稱自己的交易中整批取代 `eol_release` 的該代稱資料並寫入 ETag；解析失敗（非 JSON、缺必有欄位、日期格式不符）→ 該代稱失敗、資料不變
- 404 → 不算失敗：`SyncStats::note("endoflife.date 沒有產品 {slug}（目錄產品 {key}），請檢查 catalog.d 的 endoflife 設定")`，`eol_product.status = 'not_found'`，保留先前的分支資料（詳情顯示「endoflife.date 查無此產品」）。理由：種子或使用者打錯代稱時不應讓排程每天以結束碼 4 結束；提醒會出現在 `sync --history` 與 sync 結束摘要
- 連線錯誤、重試後的 429／5xx → 該代稱失敗，繼續處理其他代稱；全部處理完後只要有失敗的代稱，`eol` 來源回傳錯誤（訊息列出失敗的代稱），`sync_run` 記為失敗、整次 sync 結束碼 4。已成功的代稱已各自提交，不回滾（與「保留先前資料」一致：失敗的代稱資料不變）
- 轉址：reqwest 預設跟隨；不另外記錄新代稱（使用者可依產品頁更新目錄）

### D2. 同步來源 `eol`（`ingest/eol.rs`）

- `SOURCE_NAMES` 加入 `"eol"`，`SourceKind::Eol`，排在 `osv` 之後（最後執行：與 CVE 無關，失敗也不影響比對）
- 設定 `[sources] eol_enabled = true`（預設啟用）；`--source eol` 即使停用也執行（與 nvd／osv 相同）
- `Endpoints.endoflife = "https://endoflife.date/api/v1"`；`with_base` 為 `{base}/eol`；`product_url(slug) = {endoflife}/products/{slug}`（slug 已在目錄載入時驗證為 `[a-z0-9-]+`，不需要另外跳脫）
- 要查詢的代稱：`SELECT DISTINCT p.key FROM component c JOIN catalog_product p ... JOIN asset a ... WHERE a.status = 'active' AND c.removed_at IS NULL AND c.source = 'excel'`（含版本待補元件，便於補版本後立即可判定），再以 `ctx.catalog` 取得產品的代稱；沒有代稱的產品略過
- 游標：`sync_cursor('eol')` 的 JSON 為 `{ "<slug>": { "etag": "...", "fetched_at": "..." } }`
- `SyncStats`：`added` = 新出現的分支列數，`updated` = 內容變更的分支列數
- 提交一律 `ctx.commit(tx, false)`：EoL 不影響比對

### D3. 版本分支對應規則（`lifecycle/cycle.rs`）

```
tokens(s)       = s 以 . - _ 空白 ( ) / 切開、去掉空段、轉小寫
token_eq(c, v)  = c 為純數字 ? （v 開頭的連續數字存在，且數值等於 c）
                              : c 與 v 不分大小寫字串相等
is_prefix(cycle, version) = tokens(cycle) 的段數 ≤ tokens(version) 的段數，且逐段 token_eq
```

「c 為純數字時只比 v 開頭的數字」讓 `4a`（Cisco IOS XE `17.9.4a`）、`1p3`（ONTAP `9.16.1P3`）這類黏著尾碼的段落也能對到數字分支；spec 的「數字段以數值比較」即指此規則。

1. 前置：版本待補 → 不對應。`esxi` 版本格式的元件若只填 build（`build 24414501`），先以目錄 `esxi_build` 對照表換成名稱（`8.0 U3c`）；對照不到 → 「無法對應版本分支」
2. 規則一：在候選分支（D4 的目錄宣告分支，或 endoflife.date 的 `releases[].name`）中找 `is_prefix(name, version)` 者，取段數最多者；最多段數有兩個以上 → 不唯一
3. 規則二（規則一沒有符合時，只用於 endoflife.date 資料）：以 `latest.name` 做同樣的前綴比對（實例：Windows Server 分支 `2022` 的 `latest.name` 為 `10.0.20348`，cvewatch 的 Windows Server 版本為 OS build `10.0.20348.2700`）
4. 都沒有唯一結果 → 「無法對應版本分支（<版本>）」，不猜測；使用者可用元件覆寫（D7）

以 2026-11-09 取得的分支名稱驗證規則（版本為示意）：PAN-OS `11.1.4-h7` → `11.1`；ESXi `8.0 U3c` → `8.0`；PostgreSQL `16.4` → `16`；Ubuntu `22.04.4 LTS` → `22.04`；NetApp ONTAP `9.16.1P3` → `9.16.1`；Cisco IOS XE `17.9.4a` → `17.9`（`4a` 只在分支多一段時才需要比較）；Windows Server `10.0.20348.2700` → `2022`（規則二）；Microsoft SQL Server `13.0.6300.2` 對 `13.0-sp3` → 不符合（`sp3` 不是數字且不等於 `6300`），列為無法對應。

數字段比較會讓分支 `1` 與版本段 `10` 不相等（`10` 的開頭數字為 10），不會誤對。

### D4. 目錄格式（`catalog/file.rs`、`catalog/mod.rs`）

```toml
[[product]]
key = "paloalto-panos"
endoflife = "panos"                 # endoflife.date 代稱；catalog.d 寫 "" 取消

[[product]]
key = "synology-dsm"
eol_cycles = [
  { cycle = "7.1", eol = "YYYY-MM-DD", source = "https://<原廠公告網址>" },
]

[[hardware]]                        # 只在 catalog.d；種子不得有
vendor = "Aruba"
vendor_aliases = ["HPE Aruba Networking"]
model = "XYZ-100"
aliases = ["XYZ-100F"]
eol = "2026-10-31"
source = "https://vendor.example/eol/xyz-100"
```

- `ProductDef` 新增 `endoflife: Option<String>`、`eol_cycles: Option<Vec<EolCycleDef>>`；`CatalogFile` 新增 `hardware: Vec<HardwareDef>`
- 合併：`endoflife` 有寫即覆寫（`""` → `Product.endoflife = None`）；`eol_cycles` **依 `cycle` 合併**（相同 cycle 以後者為準、新 cycle 加入），與 `aliases` 合併、其他欄位整個覆寫的既有規則不同，理由是使用者常只需修正或追加一個分支
- 驗證（載入失敗並指出產品 key／分支）：代稱 `[a-z0-9-]+`；`eol` 為 `YYYY-MM-DD`；`source` 為 `http://` 或 `https://` 開頭的網址；同一產品同一 cycle 在同一檔重複 → 錯誤
- 硬體：`(normalize_vendor(vendor), normalize(model))` 為鍵，後載入者覆寫；別名索引 `(vendor_norm, normalize(alias))` 屬於兩個不同型號 → 拒絕載入。`vendor_aliases` 讓清冊寫 `HPE Aruba Networking` 的資產也能對到。種子 `seed.toml` 不得有 `[[hardware]]`：單元測試斷言 `Catalog::seed().hardware().is_empty()`
- `Catalog::fingerprint` **不**加入這些欄位（不影響比對，不應提示 `sync --rescan`）
- `catalog_product` 表不新增欄位：EoL 判定在記憶體中以 `Catalog` 進行，GUI／TUI／CLI／摘要都已載入目錄
- `cvewatch asset catalog show <key>` 加印「endoflife.date：panos」（沒有時為「endoflife.date：無」）與目錄宣告的分支表（分支、EoL、來源）。不新增列出硬體型號的子命令：型號清單是使用者自己寫的 `catalog.d` 檔

### D5. 硬體型號比對（`lifecycle/hardware.rs`）

- 資產的廠牌、型號取自 20 新增的欄位（20 design 決定的識別字；下文暫稱 `brand`、`model`）
- 廠牌與型號都有值才比對：`normalize_vendor(brand)` 等於型號的 `vendor` 或任一 `vendor_aliases` 的正規化值，且 `normalize(model)` 等於型號或任一別名的正規化值
- 只有型號沒有廠牌 → 「未填廠牌或型號」（不嘗試只用型號比對，避免不同廠牌同名型號誤對）

### D6. 資料模型與 migration

Migration 編號為 20-asset-identity-network 實際使用的下一號（預期 `0010_lifecycle.sql`；19、21 不新增 migration）：

```sql
ALTER TABLE asset ADD COLUMN warranty_end TEXT;       -- YYYY-MM-DD
ALTER TABLE asset ADD COLUMN support_end TEXT;
ALTER TABLE asset ADD COLUMN license_end TEXT;
ALTER TABLE asset ADD COLUMN hw_eol_override TEXT;    -- YYYY-MM-DD，與 hw_eol_source 同時有值或同時為 NULL
ALTER TABLE asset ADD COLUMN hw_eol_source TEXT;
ALTER TABLE component ADD COLUMN eol_override TEXT;
ALTER TABLE component ADD COLUMN eol_override_source TEXT;

CREATE TABLE eol_product (
    slug TEXT PRIMARY KEY,
    label TEXT,
    html_link TEXT,
    status TEXT NOT NULL CHECK (status IN ('ok', 'not_found')),
    last_modified TEXT,          -- API 回應的 last_modified
    fetched_at TEXT NOT NULL
);
CREATE TABLE eol_release (
    slug TEXT NOT NULL REFERENCES eol_product (slug) ON DELETE CASCADE,
    name TEXT NOT NULL,          -- 分支名稱
    label TEXT,
    release_date TEXT,
    is_eol INTEGER NOT NULL CHECK (is_eol IN (0, 1)),
    eol_from TEXT,               -- NULL：日期未知
    eoas_from TEXT,
    eoes_from TEXT,
    latest_name TEXT,
    position INTEGER NOT NULL,   -- API 回應中的順序（新到舊），顯示用
    PRIMARY KEY (slug, name)
);
```

- 不以 CHECK 驗證日期格式與「覆寫日期與來源成對」：由 core 的 repo 寫入函式保證（兩欄一起寫、一起清除），並以 repo 測試確認；不重建 `asset`、`component` 表
- 元件以軟刪除（`removed_at`）移除時，同一交易內把 `eol_override`、`eol_override_source` 設為 NULL（spec：元件被移除時覆寫一併移除）；元件之後恢復時沒有覆寫，需要時重新設定。版本修改只動 `version_raw`／`version_norm`，覆寫保留
- `AssetRecord` 新增 `warranty_end`、`support_end`、`license_end: Option<NaiveDate>` 與 `hw_eol_override: Option<EolOverride { date, source }>`；`Component` 新增 `eol_override: Option<EolOverride>`
- 日期型別：core 內部用 `chrono::NaiveDate`，DB 與清冊為 `YYYY-MM-DD` 文字

### D7. 覆寫的位置：清冊 `eol` 工作表（自行決定）

- 軟體覆寫以「清冊元件」為單位（同一資產同一產品只有一個元件，見 Editing components），存在 `component` 列上；硬體覆寫存在 `asset` 列上
- 清冊用獨立的選填工作表 `eol`（`asset_tag`、`product`、`eol`、`source`），不在 `assets` 工作表加 4 欄：覆寫很少用，`assets` 工作表加上 20 的欄位後已很寬；主要元件寫在 `assets` 列、其餘在 `components`，若把元件覆寫放在兩個工作表會很難讀。與 20 的 `ips` 工作表一樣是「一列一筆、以 asset_tag 關聯」
- `SheetKind::Eol`；CSV 清冊沒有此工作表（視同缺工作表 → 保留）
- 驗證（錯誤彙整後整批中止）：`asset_tag` 必須是同一份清冊中的資產；`product` 空白 = 硬體，否則以目錄解析且該資產在此清冊中必須有該產品的清冊元件；同一資產的硬體或同一元件出現兩列 → 錯誤；`eol` 依 D8 解析；`source` 不可空白
- 有 `eol` 工作表時：清冊中各資產的覆寫以檔案為準（檔案沒有的覆寫移除，預覽列出「覆寫移除」）；清冊沒有列出的資產不受影響（與資產本身的規則相同，資產不在清冊中時另依 Retiring assets 處理）
- 範本：`eol` 工作表含表頭與一列虛構範例，asset_tag 與產品沿用 `template.rs` 既有範例列（`SRV-01`、`ESXi`、`2030-12-31`、「範例：延長支援合約」），讓範本本身可以 `--dry-run` 匯入；`assets` 範例列的 `SRV-01` 加上三個日期的範例值
- 匯出：寫出 active 資產的所有硬體與清冊元件覆寫（產品寫目錄顯示名稱，匯入時以別名解析回來）

### D8. 日期解析（`assets/dates.rs`）

- 接受 `YYYY-MM-DD`、`YYYY/MM/DD`（月日可為一位數），以 `NaiveDate::from_ymd_opt` 驗證存在；其他寫法（含 `2027-02-30`、`27/1/1`、民國年）→ 欄位錯誤「不是有效日期（請用 YYYY-MM-DD）」
- `sheet.rs::cell_text`：`Data::DateTime(dt)` 改為以 calamine 的 `as_datetime()` 轉成 `YYYY-MM-DD`（只有日期部分；時間部分非 0 時仍只取日期）。目前沒有任何欄位依賴舊的 `to_string` 輸出（任務 3.1 以 grep 與測試確認）；`Data::DateTimeIso` 取前 10 字元
- 匯出寫成文字 `YYYY-MM-DD`（不用 Excel 日期格式，避免地區設定改變顯示）

### D9. 生命週期判定（`crates/core/src/lifecycle/`）

```rust
pub enum ItemKind { Hardware, Software { component_id: i64 }, Warranty, Support, License }
pub enum Source { Override(String), Catalog(String /* url */), EndOfLifeDate { slug: String, cycle: String } , AssetField }
pub enum ItemState {
    Dated { date: NaiveDate, source: Source },        // 有日期
    EolUndated { source: Source },                     // endoflife.date isEol=true、eolFrom=null
    NotAnnounced { cycle: String },                    // isEol=false、eolFrom=null
    Missing(MissingReason),                            // 未填、未填廠牌或型號、目錄沒有此型號、無法對應版本分支、版本待補、無 EoL 資料、endoflife.date 查無此產品、尚未同步
}
pub enum Status { Past, Soon { days_left: i64 }, Later, None }
pub struct LifecycleItem { pub kind: ItemKind, pub label: String, pub state: ItemState, pub status: Status }
pub struct AssetLifecycle { pub items: Vec<LifecycleItem> }   // flags(): eol, eol_soon, expiring, expired

pub fn evaluate(asset: &Asset, components: &[Component], catalog: &Catalog, eol: &EolData, today: NaiveDate, window: u32) -> AssetLifecycle;
pub fn load_eol(conn: &Connection) -> Result<EolData, CoreError>;   // eol_product + eol_release
pub fn summary(conn, catalog, today, window) -> Result<LifecycleSummary, CoreError>; // Dashboard 與摘要
```

- 軟體判定順序（spec Software end-of-life determination）：元件覆寫 → 目錄 `eol_cycles`（D3 規則一）→ endoflife.date（D3 規則一、二）。目錄有 `eol_cycles` 但版本不在其中、且產品也有代稱時，再試 endoflife.date
- 狀態：`days_left = date - today`；`days_left <= 0` → `Past`；`<= window` → `Soon`；其餘 `Later`；`EolUndated` → `Past`；其他 → `None`
- 「今天」= `chrono::Local::now().date_naive()`（使用者時區）；函式本身接收 `today`，測試注入固定日期
- retired 資產：`evaluate` 仍可計算（詳情可看），但清單標示、篩選、Dashboard、摘要都只用 active 資產
- 提醒天數：`Config.lifecycle.reminder_days`（D10）

### D10. 設定

```toml
[lifecycle]
reminder_days = 90      # 1–3650；EoL 與保固／合約／授權到期都用同一個值

[sources]
eol_enabled = true
```

- `config.rs` 新增 `LifecycleConfig { reminder_days: u32 }`（`serde(default)`），`validate` 檢查範圍；範例設定檔（`init` 產生）加註解
- 不分別設定 EoL 與到期的天數（使用者只要求一個可調的提前天數）

### D11. frontend、GUI、TUI

- frontend `Settings` 新增 `reminder_days`；快照（`data.rs`）載入 `EolData`，每個 `AssetEntry` 新增 `lifecycle: AssetLifecycle`（載入時以 `today` 計算一次；跨日時由既有的重新載入更新，GUI 每次切到資產或 Dashboard 畫面時檢查日期變了就重算）
- `AssetListFilter` 新增 `lifecycle: Option<LifecycleFilter>`（`Eol`、`EolSoon`、`Expiry`；`Expiry` 含即將到期與已到期）
- GUI 清單：名稱後依序附加「（已 EoL）」「（即將 EoL）」「（已到期）」「（即將到期）」標示（常數放 `asset_list.rs`，與 `PENDING_MARK` 相同作法），已 EoL／已到期用 `ALERT` 色；工具列在「只顯示版本待補」後加下拉選單「生命週期：全部／已 EoL／即將 EoL／保固或合約到期」
- GUI 詳情：`detail.rs` 新增「生命週期」區塊（`lifecycle_view.rs`，避免 `detail.rs`／`assets.rs` 過長）；網址來源以 `hyperlink` 樣式，點選呼叫 `Workspace::open_url`
- GUI 對話框：`dialogs/asset_lifecycle.rs` 新區塊（三個日期與硬體覆寫）；元件列加「EoL 覆寫…」小對話框（日期＋來源，可清除）；表單層 `frontend/src/asset_form/lifecycle.rs` 做解析與欄位錯誤
- GUI Dashboard：版本待補提示之後新增一列「已 EoL N｜即將 EoL N｜保固或合約到期 N（提醒 90 天）」，各數字可點；`Jump::Lifecycle(LifecycleFilter)` 設定篩選、清除 `assets_pending_only` 與 `assets_show_retired` 後切到資產畫面
- TUI：資產表格新增「週期」欄（寬 8，顯示最嚴重的一個狀態：已 EoL > 已到期 > 即將 EoL > 即將到期，使用 `glyphs` 的警示字元）；`App` 新增 `assets_lifecycle_filter`，`on_assets_key` 處理 `l` 循環切換，清單與游標改用篩選後的索引（`visible_assets()`）；標題「資產（生命週期：已 EoL）」；`asset_detail` 新增生命週期段落；Dashboard 在版本待補行之後加一行；`?` 說明加入 `l`

### D12. 每日摘要

- `DigestData` 新增 `lifecycle: LifecycleDigest { upcoming: Vec<Entry>, reached: Vec<Entry>, eol_assets: usize, expired_assets: usize }`
  - `upcoming`：active 資產中 `Status::Soon` 的項目（依日期排序）
  - `reached`：日期落在 `(since_local_date, until_local_date]` 的項目（期間內 EoL 或到期）
  - 「新事項」：`reached` 不為空，或任一 `upcoming` 項目的 `date - window` 落在 `(since_local_date, until_local_date]`（期間內進入提醒天數）。純日期計算，不需記錄已提醒狀態；筆電離線多天後的摘要也能涵蓋期間內所有進入與到期的項目
- `is_empty()` 加入上述「新事項」條件；其他生命週期內容照列但不影響「無新事項」字樣
- `notify/content.rs`：段落「生命週期提醒（提前 90 天）」，每列「SRV-01 維護合約到期 2027-04-10（剩 90 天）」；`reached` 列「已於 2027-01-10 到期」；最後一行「目前已 EoL 2 個資產、已到期 1 個資產」
- `collect` 需要目錄：簽章新增 `catalog: &Catalog` 與 `window: u32`，CLI `notify digest` 由 runtime 傳入

### D13. CLI

- `asset show`：在 ISMS 管理欄位之後輸出「生命週期（提醒 90 天）」段落，每項一行（spec 的格式）；輸出函式為純函式便於測試
- `sync`：`--source eol` 由 `SOURCE_NAMES` 自動支援；`sync` 結束摘要列出 `eol` 的提醒（404 代稱）
- 不新增獨立的 `cvewatch lifecycle` 子命令（Out of scope 外的取捨：畫面與摘要已涵蓋；需要時另開 change）

### D14. 測試資料

- `tests/fixtures/endoflife/`：以真實 API 回應**裁剪**後錄製（只保留 2–4 個分支與 spec 用到的欄位、加上一個未知欄位以測試相容），檔案：`panos.json`、`windows-server.json`（測規則二）、`esxi.json`、`no-eol-date.json`（`eolFrom` null 的分支）、`not-found.html`（404 本文）。fixture 中的日期改寫為測試需要的虛構日期並在檔頭以 `"_note"` 欄說明（未知欄位會被忽略）
- wiremock：200 + `ETag`、304（`If-None-Match` 相符）、404 HTML、503（重試後失敗）、301 → 新路徑
- 虛構硬體：`tests/fixtures/lifecycle_catalog.d/hardware.toml` 只有 `Aruba XYZ-100`（別名 `XYZ-100F`）與 `Aruba XYZ-200`；資產編號用 FW-EDGE、SRV-01、SW-CORE、NAS-01
- commit 前執行 `scripts/check-sensitive.ps1`

### D15. 種子 EoL 資料的來源（只列查證起點，不在規劃階段寫日期）

- endoflife.date 代稱（2026-11-09 以 `GET /api/v1/products` 確認存在）：PAN-OS `panos`、Cisco IOS XE `cisco-ios-xe`、RouterOS `routeros`、FortiOS `fortios`、ESXi `esxi`、vCenter `vcenter`、Proxmox VE `proxmox-ve`、TrueNAS `truenas`、NetApp ONTAP `netapp-ontap`、Veeam `veeam-backup-and-replication`、Commvault `commvault`、nginx `nginx`、Apache HTTP Server `apache-http-server`、Tomcat `tomcat`、MariaDB `mariadb`、MySQL `mysql`、PostgreSQL `postgresql`、SQL Server `mssqlserver`、MongoDB `mongodb`、Redis `redis`、Ruby `ruby`、Rails `rails`、PHP `php`、Python `python`、Node.js `nodejs`、Django `django`、Laravel `laravel`、WordPress `wordpress`、Drupal `drupal`、CKEditor 4 `ckeditor`（分支 `4`）、Bootstrap `bootstrap`、jQuery `jquery`、jQuery UI `jquery-ui`、OpenSSL `openssl`、Samba `samba`、BIND 9 `bind-9`、Debian `debian`、Ubuntu `ubuntu`、RHEL `rhel`、Windows Server `windows-server`；Java SE 對應 `oracle-jdk` 或 OpenJDK 系列需在任務中決定（種子 `java-se` 的 CPE 為 Oracle）
- endoflife.date 未涵蓋、需查原廠公告：Synology DSM（Synology「Product Support Status」頁 https://www.synology.com/en-global/products/status 的 Software 分頁）、Dell iDRAC9 **不建種子**（使用者 2026-11-09 確認）：Dell KB 000178016（https://www.dell.com/support/kbdoc/en-us/000178016/ ）的 End of Feature Development／End of Software Maintenance 依 PowerEdge 世代訂定，無法以韌體分支表達，改由伺服器型號的硬體 EoL（本機 `catalog.d`）涵蓋
- 其他未涵蓋的種子產品（AOS-CX、ArubaOS、AOS-Switch、Cisco IOS／NX-OS／ASA／FTD、Junos、Arista EOS、SonicOS、Sophos、WatchGuard、Check Point、Ivanti、F5、NetScaler、RUCKUS、UniFi、QNAP、ASUSTOR、QSAN、NAKIVO、Acronis、Veritas、PIXIS、PowerEdge BIOS 等）**本 change 不建種子 EoL**，使用者需要時在 `catalog.d` 自行宣告；任務 6.3 只列出查證起點供參考

## Risks / Trade-offs

- [endoflife.date 改版或資料錯誤] 社群維護的資料可能延遲或與原廠不一致 → 詳情顯示來源（分支與 endoflife.date 產品頁），`catalog.d` 的分支宣告與元件覆寫可修正；解析只依賴文件列出的欄位
- [版本分支對應錯誤] 規則以字串前綴為主，可能遇到命名不一致的產品（SQL Server 的 `13.0-sp3`）→ 不唯一或不符合時一律「無法對應」而不猜，畫面明確顯示；Windows Server 依賴規則二
- [iDRAC9 的 EoL 依伺服器世代而非韌體分支] Dell 的公告以 PowerEdge 世代訂日期，同一 iDRAC9 韌體分支可能對應不同世代 → 使用者已確認不建 iDRAC9 種子，由硬體型號（伺服器機型）的 EoL 涵蓋
- [提醒天數只有一個] EoL 規劃通常需要比合約續約更早 → 先依使用者要求只做一個值；之後可拆成兩個設定
- [日期以本機時區計算] 筆電時區錯誤會差一天 → 可接受；測試注入固定日期
- [`cell_text` 日期格式改變] 若其他欄位曾依賴 Excel 日期儲存格的舊字串 → 任務 3.1 確認並以測試涵蓋
- [不影響比對] EoL 資料變更不重新比對，比對結果與 EoL 狀態是兩件事；畫面上分開呈現
