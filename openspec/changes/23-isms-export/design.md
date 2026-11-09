# Design

## Context

- ISMS 匯入（17、18）的讀取端：`isms/workbook.rs` 在前 10 列找同時含「資產編號」「資產類別」「資產描述」的列為表頭（欄名經 `normalize_header` 去除空白、換行與括號註記），兩欄「資產編號」依欄位順序為系統代碼與流水號，第二欄為空的列不是資產列；工作表優先「資訊資產清冊」。v2.1 的 16 欄表頭目前只是 `workbook.rs` 測試模組內的 `HEADERS` 常數。資產價值、防護等級、資產名稱欄不讀
- `isms/row.rs`：ISMS 編號 = 第一欄去除結尾 `-` ＋ `-` ＋ 第二欄；資產類別接受子類別代碼或名稱（`AssetClasses::resolve`）；法規遵循性的單一破折號或空白為未評定；是否含個人資料只接受 Y／N
- `isms/describe.rs` 的描述規則（讀回時決定推測出的產品）：
  1. NFKC 後依 `\n`、`\r`、`、`、`,`、`;`、`，` 拆行
  2. 每行 `strip_label`：第一個 `:` 之前若 ≤16 字、≤2 個詞、不含數字、冒號後不接 `/`，視為標籤並去除
  3. 去除 `(...)`、`[...]`、`【...】` 註記，再去除結尾數量（`*2`、` x2`、`×2`）
  4. 清理後為空的行略過；否則以 `Catalog::resolve` **完全比對**正規化後的別名（含 key 與 display_name），失敗時去除開頭中文字詞再試一次；仍失敗即為未辨識產品
  5. 同一列兩行比對到同一產品時合併並在報表警告
- 因此「產品 版本」（例如 `AOS-CX 1.2.3`）讀回時無法完全比對、會成為未辨識產品；而廠牌型號（例如 `Aruba XYZ-100`）若剛好是某產品的別名（`tests/fixtures/isms_catalog.d` 就把它宣告為 `hpe-aoscx` 的別名），會被推測成產品或與元件合併
- 一般清冊匯出 `assets/export.rs`：`inventory_items_by_asset`（只取 `kind = 'product' AND source = 'excel'` 的元件與 `asset_unresolved`，依建立順序）、`product_text`（顯示名稱能解析回同一產品才用，否則寫 key）、`personal_data_text`；寫檔用 `template::write_sheets`（只寫字串、表頭固定在第 1 列）與 `files::write_new`（`create_new`，不覆蓋）
- CLI `asset export` 的「此檔未加密」提示為 `commands/asset.rs` 的 `UNENCRYPTED_NOTICE`；GUI 的「匯出清冊」由 `screens/assets.rs` 以 `FileDialogs::save_xlsx` 取得路徑後直接呼叫 frontend `Workspace::export_inventory`（不經 companion），預設檔名由 `dated_xlsx_default` 產生
- 20-asset-identity-network 會在 `AssetRecord` 加入廠牌、型號、序號、財產編號、主機名稱、IP、管理介面欄位（20 的 proposal 命名為 `brand`、`model`，皆為選填純文字；另有 IP 存在 `asset_ip` 表。apply 時以 20 封存後的實際名稱為準）

## Goals / Non-Goals

**Goals:**

- 匯出檔可直接被現有的 ISMS 匯入讀回，且推測出的產品與原資產相同；不修改 ISMS 匯入的任何規則
- 讀回保證由程式自我檢查：匯出時用讀取端的同一組函式檢查每一列，不一致時警告而不是默默寫出
- 列資料的組成是純函式（不碰檔案），xlsx 版面另外一層，便於測試

**Non-Goals:**

- 不產生與機構官方範本逐格相同的檔案（格式、公式、選項工作表、文件編號）
- 不在讀取端新增「辨識匯出檔」的特殊規則或標記

## Decisions

### D1. CLI：`cvewatch asset export-isms <path.xlsx>`

新增子命令，不在 `asset export` 加 `--isms` 旗標：

- 與既有的 `asset import-isms` 成對；`asset export` 的輸出要能以 `asset import` 讀回，ISMS 格式要以 `asset import-isms` 讀回，兩者的讀回指令、欄位與提示都不同，放在同一子命令的旗標下 `--help` 會混在一起
- 參數只有輸出路徑（與 `asset export`、`asset template` 相同）；處理放在 `commands/asset_isms.rs`（與 `import-isms` 同檔，約 170 行，加入後仍在 300 行內）
- 流程比照 `asset export`：先 `ensure_absent(path)`（避免開啟資料庫、要求金鑰後才失敗）→ 載入目錄與子類別 → `open_working_db`（未初始化時沿用既有錯誤）→ core `export_isms`
- 輸出：
  ```
  已匯出 3 個資產：D:\isms-out.xlsx
  略過 1 個沒有 ISMS 編號的資產：SRV-01
  警告：FW-EDGE 的「Zyxel ABCD x2」以 ISMS 匯入讀回時會成為「Zyxel ABCD」
  注意：此檔未加密，含資產清單，請妥善保管。
  可用 `cvewatch asset import-isms` 讀回；此檔由 cvewatch 產生，不是機構的 ISMS 範本，提交前請依範本核對。
  ```
  略過清單全部列出（不設上限，與 18 的報表相同）。結束碼：成功為 0；有略過或警告時仍為 0（不中止）

### D2. core 模組

| 檔案 | 內容 |
|---|---|
| `isms/export.rs` | `IsmsExportRow`（16 欄的值）、`IsmsExport { rows, skipped, warnings }`、`isms_export_rows(conn, classes, catalog) -> Result<IsmsExport>`（只讀）與 `export_isms(conn, classes, catalog, path, exported_on: NaiveDate) -> Result<IsmsExportSummary>` |
| `isms/description.rs` | 描述組成 `compose_description(identity, items, catalog) -> Composed { text, warnings }` 與讀回檢查（D5、D6） |
| `isms/export_write.rs` | xlsx 版面（D3），輸入 `&[IsmsExportRow]` 與匯出日期，回傳位元組；寫檔沿用 `files::write_new` |

- `workbook.rs`：測試中的 `HEADERS` 改為公開常數 `ISMS_V21_HEADERS: [&str; 16]`，讀取端測試與匯出共用同一份表頭
- `describe.rs`：分隔字元 `SEPARATORS` 改為 `pub(crate)`，另新增 `pub(crate) const NOTE_BRACKETS`（`()[]【】` 與全形括號），供 D5 的字元替換使用；規則本身不改
- `template.rs` 的 `already_exists` 改為 `pub(crate)`，目的檔已存在的訊息與其他匯出相同
- 不重用 `template::write_sheets`：它只寫字串、表頭固定在第 1 列，ISMS 版面需要文件資訊列、數字儲存格與換行格式

### D3. 版面

- 只有一個工作表 `ISMS_SHEET_NAME`（「資訊資產清冊」）
- 第 1–3 列（文件資訊，各只填 A、B 欄，不合併）：
  - 第 1 列：A「資訊資產清冊（cvewatch 匯出）」
  - 第 2 列：A「匯出日期」、B `2026-11-09` 格式的日期文字
  - 第 3 列：A「說明」、B「僅含有 ISMS 編號的使用中硬體與軟體資產；資產描述中括號內為版本或廠牌型號」
  - 三列都不含「資產編號」「資產類別」「資產描述」，所以 `find_header` 不會把它們誤認為表頭；不寫文件編號（內部文件編號不得出現在程式或檔案中）
- 第 4 列：`ISMS_V21_HEADERS`，粗體、換行（表頭本身含 `\n`）
- 第 5 列起：資料列。兩欄資產編號與文字欄以文字寫入；C/I/A、資產價值、法規遵循性（有值時）、防護等級（有值時）以數字寫入，方便使用者在 Excel 加總或篩選；資產描述欄設為自動換行
- 凍結窗格在第 5 列；欄寬固定（資產描述較寬），不設公式
- 不加「選項(勿刪)」工作表：讀取端不讀它（18 Out of scope），內容又與設定檔的子類別重複，加了反而讓人以為可以在那裡改範本值
- 不使用合併儲存格：讀取端雖然支援垂直合併，但每列都寫系統代碼最直接，也方便使用者排序

替代方案「表頭放第 1 列」：讀取端也讀得到，但與 ISMS 清冊的實際版面（文件資訊在上、表頭在第 4 列）不同，使用者把資料貼回官方範本時要另外對位，不採用。

### D4. 欄位對應

| 欄 | 值 | 讀回時 |
|---|---|---|
| 資產編號（一） | ISMS 編號第一個 `-` 之前＋`-`（`NET-`） | 去除結尾 `-` 後組回 |
| 資產編號（二） | 第一個 `-` 之後（`HW-001`），保留原寫法 | 同上 |
| 資產類別 | 子類別代碼（`HW02`） | `resolve` 接受代碼；寫代碼不受子類別改名影響 |
| 資產說明 | 資產說明名稱 | 更新資產說明名稱（相同即不變） |
| 資產名稱 | 子類別名稱（16 的決定）；子類別已不在設定中時空白並警告 | 不讀 |
| 資產描述 | D5 | 推測元件 |
| 權責單位、保管人、使用單位 | 原值，未填空白 | 原值 |
| 機密性、完整性、可用性 | 數字 | 數字文字 `2` |
| 資產價值 | `cia.value()` | 不讀 |
| 法規遵循性 | 有值為數字；未評定為 `-` | `-` 為未評定 |
| 資通系統防護等級 | `protection_level()`；未評定為 `-` | 不讀 |
| 是否含個人資料 | `personal_data_text`（Y／N） | Y／N |

- 排序：ISMS 編號轉小寫後排序（不分大小寫），相同系統代碼自然排在一起
- 列的來源：`AssetRepo::list(&AssetFilter::default())`（只有 active），`isms_id` 為 None 的放入 `skipped`（依 asset_tag 排序）
- cvewatch 的資產類別只有 HW／SW 大類，所以「只匯出 HW／SW」不需要另外過濾

### D5. 資產描述與讀回

一行一項，以 `\n` 連接：

1. 廠牌型號行（`brand` 或 `model` 有值時；只有其一時只寫該值）：`廠牌型號：【<廠牌> <型號>】`
   - 讀回時：`strip_label` 把「廠牌型號」視為標籤（4 字、1 個詞、無數字）去除，剩下的 `【…】` 被註記規則整段去除，清理後為空 → 整行略過；新資產的名稱（描述第一個非空的行）因此取第一個元件行
   - 替代方案「直接寫 `Aruba XYZ-100`」：型號剛好是目錄別名時會被推測成產品（或與元件合併、產生合併警告），不是別名時成為未辨識產品，兩者都違反讀回保證，不採用
   - 替代方案「只加標籤 `廠牌型號：Aruba XYZ-100`」：標籤被去除後仍以 `Aruba XYZ-100` 比對，結果同上，不採用
   - 替代方案「寫在資產名稱欄」：16 已決定該欄為子類別名稱，不採用
   - 替代方案「在讀取端新增忽略標記的規則」：會改變 ISMS 匯入對機構原始清冊的推測行為（Out of scope），不採用
2. 元件行（依建立順序，`inventory_items_by_asset` 的結果；主機套件不在其中）：
   - 目錄產品：`product_text(catalog, key)`（顯示名稱；無法解析回同一產品時為 key）
   - 未辨識產品：原文
   - 有版本時接 `（<版本>）`；版本待補（`is_pending_version`）只寫產品
   - 讀回時：括號內的版本被註記規則去除，剩下的顯示名稱以完全比對解析回同一產品
   - 替代方案「`產品 版本`（空白分隔）」：別名是完全比對，`AOS-CX 1.2.3` 不會辨識為 AOS-CX，讀回全部成為未辨識產品，不採用（見 Open Questions）
   - 替代方案「讀取端去除結尾的版本再比對」：機構清冊中的 `Windows Server 2019` 這類名稱會被改變推測結果，不採用
3. 字元替換：廠牌、型號與版本（先 NFKC）中的 `SEPARATORS`、`:`、`NOTE_BRACKETS` 一律換成空白並合併連續空白，避免版本 `1.0, build 5` 被拆成兩行、型號內的 `】` 提早結束註記、或冒號被當成標籤。產品文字（顯示名稱、key、未辨識原文）不替換，由 D6 檢查
4. 不寫入主機名稱、IP、序號、財產編號、管理介面與 22 的到期欄位；測試以「匯出檔任何儲存格都不含主機名稱與 IP」斷言

### D6. 讀回檢查（自我驗證）

`compose_description` 組好描述後，立刻以 `describe::describe(&text, catalog)` 讀一次並比較：

- 預期：目錄產品 key 的集合 = 資產元件的 key 集合；未辨識項目的 `normalize(text)` 集合 = 資產未辨識產品 `normalize(product)` 的集合；沒有 `lines.len() > 1` 的合併
- 目錄產品的顯示名稱讀回不是同一 key（例如顯示名稱含括號或分隔字元）時，該行改寫 key 再檢查一次；仍不一致才警告
- 不一致時的警告以資產編號與原文說明讀回結果，例如「FW-EDGE 的「Zyxel ABCD x2」以 ISMS 匯入讀回時會成為「Zyxel ABCD」」、「SRV-01 的未辨識產品「Veeam B&R」讀回時會辨識為 veeam-backup-replication」；匯出不中止，該行照原樣寫出
- 描述沒有任何元件行（只有廠牌型號或完全空白）時警告「資產描述沒有元件行，以 ISMS 匯入建立新資產時會被略過」
- 檢查使用與匯入相同的 `Catalog`（CLI 的 `app.load_catalog()`、GUI 的 `settings.catalog`：內建種子＋ catalog.d），所以只要讀回也用同一份目錄，檢查結果就是實際讀回的結果

### D7. 回傳與提示

```rust
pub struct IsmsExportSummary {
    pub exported: usize,
    pub skipped: Vec<String>,   // 沒有 ISMS 編號的 active 資產編號
    pub warnings: Vec<String>,  // 子類別未定義、讀回不一致、沒有元件行
}
```

- CLI 依 D1 印出，最後印 `UNENCRYPTED_NOTICE` 與讀回提示
- frontend `Workspace::export_isms(&self, path) -> Report`：沒有略過與警告時 `Report::info`「已匯出 N 個資產至 <路徑>（此檔未加密，含資產清單）」；有時 `Report::warning`，訊息後接「；略過沒有 ISMS 編號的資產：SRV-01、…；<警告>」。失敗為 `Report::failure("匯出 ISMS 清冊失敗", e)`
- `tracing::info!` 只記錄路徑與數量（匯出數、略過數、警告數），不記錄保管人等欄位與資產描述

### D8. 匯出日期

core 不取時間，`export_isms` 接收 `exported_on: NaiveDate`：CLI 傳 `chrono::Local::now().date_naive()`（與 `schedule` 相同），frontend 以 `services.tz` 與 `services.now()` 取當天日期（與 `dated_xlsx_default` 相同），測試傳固定日期。

### D9. GUI

- `asset_list.rs` 工具列：在 `ISMS_IMPORT_BUTTON` 之後加 `ISMS_EXPORT_BUTTON = "匯出 ISMS 清冊"`，滑鼠提示「只匯出有 ISMS 編號的使用中資產；可再以「從 ISMS 清冊匯入」讀回」；不以 `companion` 啟用與否控制（匯出在行程內呼叫 core，與「匯出清冊」相同）；更新模組開頭的工具列說明註解
- `screens/assets.rs`：`export_isms()`，比照 `export_inventory()`：`self.dialogs.save_xlsx("匯出 ISMS 清冊", &dir, &name)`，取消時 `Report::info("已取消匯出 ISMS 清冊")`
- frontend：`isms_export_default()` = `dated_xlsx_default("cvewatch-isms")`
- 不修改 gui-console 的 GUI asset presentation（其工具列描述只列一般清冊操作，18 新增「從 ISMS 清冊匯入」時也是另立 requirement），本 change 以 ADDED 的 GUI ISMS export 描述按鈕位置

### D10. 沒有可匯出的資產

有 ISMS 編號的 active 資產為 0 時仍寫出只有文件資訊與表頭的檔案，摘要為「已匯出 0 個資產」並列出略過的資產：與一般清冊匯出的行為一致，也讓使用者拿到可填的空白版面。

## Risks / Trade-offs

- [資產描述的括號寫法與機構清冊的習慣不同（版本在括號內、廠牌型號以【】包住）] → 文件資訊第 3 列與 `docs/user-guide/assets.md` 說明；這是不改讀取端又能保證讀回的最小代價。使用者若要改成機構習慣的寫法，需理解讀回時推測結果可能不同
- [讀取端規則日後修改，匯出寫法不再能讀回] → D6 以讀取端的同一組函式檢查，規則一變就會出現警告；round-trip 整合測試在 CI 中失敗
- [匯出檔含保管人姓名且未加密] → 完成時提示此檔未加密；日誌不記錄欄位內容；測試與 fixture 只用虛構姓名
- [使用者把匯出檔當作機構正式清冊提交] → 第 1 列標題標示「cvewatch 匯出」，CLI 提示提交前依範本核對
- [GUI 通知中略過清單很長] → 單人工具的資產數量有限；CLI 有完整清單可對照
- [同一資料庫讀回時資產說明名稱衝突] → 匯出的名稱就是資料庫中的名稱，不會衝突；若匯出後在別處改了名稱，依 18 的規則中止匯入

## Migration Plan

不需要資料庫 migration 或設定變更；直接更換執行檔。回復時換回舊版即可，已匯出的檔案不受影響。

## Open Questions

（無）使用者 2026-11-09 確認：描述行採「產品（版本）」、廠牌型號行採「廠牌型號：【…】」以保證讀回；未評定寫 `-`、未辨識產品寫進描述依本 design 進行
