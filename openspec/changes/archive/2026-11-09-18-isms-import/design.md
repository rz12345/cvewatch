# Design

## Context

- 17 的 ISMS 轉換已拆成純函式：`assets/isms/` 的 `workbook`／`workbook_read`（讀取）、`row`（單列驗證與略過原因）、`describe`（描述比對）、`draft`（`build_draft(&IsmsBook, &DraftContext) -> Draft`，含沿用既有資產與 `systems`）、`report`，最後由 `output.rs` 寫出 xlsx。`Draft` 的每個資產已帶有完整的 `AssetRecord` 與清冊項目（`InventoryItem`；既有資產含原有元件與版本，新產品版本為空字串）。
- 清冊匯入的寫入路徑是 `import_inventory(conn, catalog, &Inventory, ImportOptions, now)`：同一交易內 `plan_import` → 寫資產說明 → `apply_asset`（含版本歷史 `component_history`）→ 可選的 prune。`validate.rs` 讀列時把 `version` 當必填（`r.required(StandardField::Version, …)`），所以空白版本整批中止。
- 資料表 `component.version_raw TEXT NOT NULL`、`version_norm TEXT`（可為 NULL）；`asset_unresolved.version_raw TEXT NOT NULL`。現有資料庫中不會有空字串版本。
- 比對：`matching/load.rs` 從 view `active_component` 載入元件；`findings::load_existing` 只載入這些元件的命中；`write_conflicts` 只清除這些元件的來源矛盾。所以沒被載入的元件，它的命中與來源矛盾都不會被動到。`active_component` 也被 `ingest/relevance.rs`、`epss.rs`、`osv`、`show.rs`、`query.rs`、`dashboard.rs` 使用。
- GUI 的清冊匯入經由 companion：`gui/src/dialogs/import.rs` 以 `asset import <file> … --dry-run` 取得預覽文字，確認後去掉 `--dry-run` 再執行同一組參數（`companion.rs` 的 `ImportArgs::to_args`）。ISMS 草稿則是 `frontend/workspace/isms.rs` 直接呼叫 core（只讀資料庫）。

## Goals / Non-Goals

**Goals:**
- ISMS 匯入與一般清冊匯入共用同一條寫入路徑（同一個 `import_inventory` 交易、同一套版本歷史與比對觸發），ISMS 匯入只是另一種「讀入 `Inventory`」的方式
- 「版本待補」只有一個表示法，所有讀取端（比對、Dashboard、摘要、畫面、匯出）都用同一個判斷函式
- 待補元件不會被當成「版本未知」而產生 probable 命中

**Non-Goals:**
- 不新增資料表欄位或 migration
- 一般清冊匯入的預覽格式不改（ISMS 匯入另外多印 ISMS 報表）

## Decisions

### D1. 版本待補 ＝ `version_raw` 為空字串

待補元件與待補的未辨識產品以 `version_raw = ''`、`version_norm = NULL` 表示。core 提供 `is_pending_version(&str) -> bool`（去除空白後為空），供 `Component`、`UnresolvedEntry`、比對載入、計數查詢與畫面共用；SQL 端統一寫 `trim(version_raw) = ''`。

- 替代方案「新增 `version_pending` 欄位」：兩個欄位可能互相矛盾（有版本卻標為待補），還要 migration，不採用。
- 替代方案「寫入 `待補` 等特殊字串」：會被版本解析當成無法解析的版本（probable），匯出時也會寫出這段文字，再匯入就變成版本，不採用。
- 主機套件元件由套件清單解析器建立，版本一律非空；`packages` 匯入時若遇到空白版本，維持現有的錯誤處理，不轉為待補。

### D2. 比對只在載入時排除待補元件，不改 `active_component` view

`matching/load.rs` 的元件查詢加上 `AND NOT (c.kind = 'product' AND trim(c.version_raw) = '')`。因為命中與來源矛盾的更新都限定在已載入的元件（見 Context），待補元件的既有命中自然保留、不更新，填上版本後下一次比對沿用原命中，與「元件從清冊消失」的行為一致。

- 不改 view：`ingest/relevance.rs` 依 active 元件判斷哪些 CVE 與資產有關。待補的產品仍應視為有關，CVE 資料才會在本機；版本填上時可以立刻比對，不必等下一次全量同步。
- Dashboard 的來源矛盾數改為排除待補元件（舊的 `match_conflict` 列在元件待補期間不會被清除）。

### D3. 一般清冊匯入：version 改為選填

- `validate.rs`：`assets` 列與 `components` 列的 `version` 改用非必填讀取；`product` 仍為必填。
- `plan.rs`：`resolve_version` 遇到待補版本時，不解析、不產生版本格式警告。版本由非空變成空白時，`VersionChange.to` 為空字串，預覽以「待補」呈現（`11.1.6 → 待補`）；由空白變成非空時，預覽呈現 `待補 → 7.2.2-72806`。
- `ImportPlan` 新增 `pending_components()`：匯入後檔案中版本待補的目錄產品元件數。CLI 預覽、完成摘要與 GUI 預覽都列出這個數字。
- 版本歷史：沿用「版本變更時寫入舊版本」的邏輯，但舊版本為空白時不寫入（`apply_asset` 與 `edit.rs` 共用同一個判斷）。所以 `11.1.6 → 待補` 會留下 `11.1.6`，`待補 → x` 不留紀錄。
- `affects_matching()` 不需要修改：待補的版本變更本來就算版本變更，會觸發比對；比對時被排除，等於停止更新。

### D4. ISMS 匯入：轉換結果 → `Inventory` → 既有的匯入交易

- 新增 `assets/isms/convert.rs`（取代 `output.rs`）：`to_inventory(&Conversion) -> Inventory`，把每個轉換資產的 `AssetRecord` 與清冊項目組成和讀檔相同的 `IncomingAsset`／`IncomingComponent`，`systems` 組成 `systems` 工作表的內容。之後交給 `import_inventory`，`ImportOptions { prune: false, dry_run }`。
- 命名：草稿不存在了，所以 `draft.rs` → `conversion.rs`、`Draft` → `Conversion`、`DraftContext` → `ConversionContext`、`DraftReport` → `IsmsReport`。改名一次做完，避免「草稿」一詞留在程式中造成誤解。
- `build` 的行為延續 17，只調整：
  - 新資產的描述沒有任何項目時，改為 `SkipReason`（原本是報表的 `no_components`），避免整批因「至少一個元件」而中止
  - 新資產的曝露度直接用預設值 internal（不再需要 `blank_exposure`）
- 既有資產：`Conversion` 已帶入原有元件與版本，所以 `plan_import` 看到的只有 ISMS 欄位變更與新增的待補項目，既有元件不會被移除。`ISMS 編號 → 既有資產` 的判斷仍由 `ConversionContext::load` 讀資料庫完成。這一步在 `import_inventory` 的交易之外，但 CLI 是單一程序；GUI 由 companion 執行，同時只會有一個匯入，所以可以接受。
- 系統名稱衝突沿用 `plan_import` 的錯誤，整批中止。不再先寫報表提醒「匯入前須修改」。
- 資料庫未初始化：`import-isms` 使用與 `asset import` 相同的開啟資料庫流程，未初始化時以相同的錯誤結束。

### D5. ISMS 欄位差異：`AssetUpdate` 帶出新舊值

`AssetUpdate.changed_fields: Vec<&'static str>` 改為 `Vec<FieldChange { field, from, to }>`，值以畫面用語呈現（類別「代碼 名稱」、未填為「-」、法規遵循性「未評定」）。一般清冊匯入的預覽仍只印欄位名稱，維持原格式；ISMS 匯入預覽印出 `機密性 2 → 3`、`保管人 王小明 → 李小華`。保管人等姓名只出現在終端機與 GUI 預覽，不寫入日誌（`tracing` 只記錄數量）。

### D6. ISMS 匯入報表與 CLI 輸出

`IsmsReport` 保留 17 的內容（依大類略過、略過的列、合併的產品、未辨識的描述行、資產說明名稱警告、不在清冊中的資產），移除 `sheet_rows`、`REPORT_SHEET` 與 `--prune` 提醒，新增「新增的版本待補元件數」。CLI `asset import-isms` 先印 ISMS 報表，再印與 `asset import` 相同的匯入預覽與摘要（共用 `commands/asset.rs` 的 `print_plan` 與 `asset_view.rs` 的 `print_plan_details`），最後提醒「版本待補的元件未納入比對，請在畫面補上版本」。結束碼與 `asset import` 相同。

### D7. 待補計數的單一來源

core 新增 `assets::pending::pending_counts(conn) -> PendingCounts { components, assets }`，只計算 active 資產的目錄產品元件（未辨識產品另有「未辨識」標示，不重複計算）。使用端：

| 使用端 | 呈現 |
|---|---|
| `matching::dashboard::Dashboard` | 新增 `pending_versions` 欄位，GUI 與 TUI 共用 |
| `cvewatch match`、`cvewatch sync` | 結束時 `components > 0` 才印「版本待補 N 個元件（M 個資產），未納入比對」；sync 略過比對時也印 |
| `notify::digest::DigestData` | 寄出時查詢；不影響「無新事項」的判斷 |

文字由 core 的 `PendingCounts::summary()` 產生，四個地方的寫法一致。

### D8. 畫面

- frontend：`display` 新增 `PENDING_VERSION = "版本待補"`。元件版本顯示、資產清單標示與詳情的「（未納入比對）」都用這個常數。資產篩選新增 `pending_only`，Dashboard 點選時設定這個篩選。
- `asset_form`：元件的原版本為空白、送出時也是空白 → 視為未變更，可以儲存；原版本非空、送出空白 → `必填`；新加入的元件版本為空白 → `必填`。core `edit.rs` 也做相同檢查，CLI 編修因此一致。
- GUI：清冊檔案組移除「由 ISMS 清冊產生草稿」，新增「從 ISMS 清冊匯入」。`companion.rs` 的匯入參數加上 `ImportKind::{Inventory, Isms}`，`Isms` 產生 `asset import-isms <file> [--dry-run]`。`dialogs/import.rs` 沿用預覽 → 確認流程；`Isms` 時不顯示元件檔與退役勾選，覆寫提醒改為「只更新 ISMS 欄位並加入版本待補元件，不會移除元件」。`FileDialogs` 保留 `open_isms_workbook`，移除草稿另存對話框。
- TUI：Dashboard 加一行計數；資產清單在有待補元件的資產旁加標示（沿用 `glyphs.rs` 的寬度安全字元），元件版本顯示「版本待補」。

### D9. 移除的程式與文件

- core：`isms/output.rs`、`generate_draft`、`write_draft`、`isms_report` 工作表相關常數與測試。`export.rs` 抽出的共用函式仍由清冊匯出使用，保留。
- CLI：`commands/asset_isms.rs` 改為 `import-isms` 的處理；`cli.rs` 移除 `IsmsDraft` 子指令。
- frontend：`workspace/isms.rs` 的草稿函式與 `dated_xlsx_default("cvewatch-isms-draft")` 用法。
- 測試與 fixture：`tests/fixtures` 的 ISMS 清冊（虛構資料）保留，`tests/isms_draft.rs` 改寫為 `tests/isms_import.rs`。
- 文件：`docs/user-guide/assets.md` 的「由 ISMS 清冊建立資產」改寫為匯入，並新增「版本待補」一節；`gui.md` 的按鈕說明。

## Risks / Trade-offs

- [待補元件未被監控，使用者誤以為已受保護] → Dashboard（GUI、TUI）、每日摘要、`match`／`sync` 摘要、資產清單與詳情都標示，文字一律說明「未納入比對」
- [一般匯入漏填版本不再被擋下] → 預覽與完成摘要列出待補數；GUI 預覽在待補數大於 0 時以警告樣式顯示
- [重匯的檔案版本空白，蓋掉已知版本並停止比對] → 預覽列為版本變更 `x → 待補`，版本歷史保留舊版本，可以從歷史找回並填回
- [ISMS 匯入覆寫在 cvewatch 手動修改過的 ISMS 欄位] → 預覽逐欄列出新舊值；ISMS 清冊是這些欄位的權威來源
- [移除 `asset isms-draft` 是破壞性變更] → 單人使用的工具，沒有已知的自動化腳本依賴；使用說明列出替代指令
- [一列多台設備無法再在草稿中拆開] → 合併警告改為提醒在畫面另外新增資產
- [降版到舊版程式時，空字串版本會被當成無法解析的版本而產生大量 probable 命中] → 不提供降版支援；在 `docs/user-guide/assets.md` 的「版本待補」一節註明

## Migration Plan

不需要資料庫 migration：現有資料庫沒有空字串版本，升級後行為不變。部署方式是直接更換執行檔。回復方式：換回舊版執行檔前，先把所有待補元件的版本補上（或在舊版中刪除這些元件），否則會出現上述 probable 命中。

## 實作紀錄（apply 時依建議做法決定）

- **D1 補充：判斷集中**：SQL 條件寫成 `assets::pending::PENDING_COMPONENT_SQL`（`c.kind = 'product' AND trim(c.version_raw) = ''`），比對載入、Dashboard 的來源矛盾數、來源矛盾清單與 `pending_counts` 共用；Rust 端一律用 `is_pending_version`。
- **D2 補充：來源矛盾清單**：`cvewatch finding conflicts`（`query::conflicts`）也排除版本待補元件的舊紀錄，與 Dashboard 的數字一致。
- **D2 補充：等待區手動套用**：`pending resolve --apply` 使用比對載入的元件，所以不能套用到版本待補的元件（不建立 probable 命中，符合 Pending versions are not matched）。
- **D3 補充：版本解析**：`resolve_version` 遇到待補版本回傳 `(None, None)`，清冊匯入、畫面編修與未辨識產品改為目錄產品共用，待補不會產生版本格式警告。
- **D3 補充：版本歷史**：「舊版本為空白時不寫歷史」放在 `ComponentRepo::change_version`，清冊匯入、畫面編修與套件匯入共用同一個判斷（套件版本一律非空，行為不變）。
- **D3 補充：`version` 欄**：只有儲存格可以空白；工作表仍須有 `version` 欄（`inventory_missing_column.xlsx` 的「缺少 version 欄」照舊中止），最保守、不改變表頭規則。
- **D3 補充：fixture**：`inventory_bad_rows.xlsx` 第 12 列改為缺少 product（原為缺少 version），重現 spec 的 Missing required field；以 `gen_inventory_fixtures` 重新產生，只提交這一個檔案。
- **D3 補充：CLI 摘要**：`asset import` 的預覽與完成摘要在待補數大於 0 時另印「版本待補 N 個元件（未納入比對，請在畫面補上版本）」。
- **D4 補充：模組**：`isms/import.rs` 放 `to_inventory` 與 `import_isms`（design 原寫 `convert.rs`；`conversion.rs` 已是轉換本體，避免兩個名稱相近的檔案）。`import_isms(conn, classes, catalog, path, dry_run, now)` 在函式內載入 `ConversionContext`，呼叫端不必先借用連線。
- **D4 補充：描述空白**：只有**新資產**的描述空白（沒有可建立的元件）才略過；既有資產照常更新 ISMS 欄位、保留原有元件。
- **D4 補充：報表計數**：重複對應同一資產而被略過的列，其合併的產品、未辨識描述行與待補數不列入報表（17 會列入）。`IsmsReport.pending_components` 只計這次新加入的目錄產品元件；既有的待補元件不計，所以重匯時為 0。
- **D4 補充：資產說明衝突的列號**：`systems` 的錯誤以第一個使用該代碼的 ISMS 列號呈現（「第 N 列：…」）。
- **D5 補充：`FieldChange`**：放在 `assets/plan_fields.rs`；`field` 為 `StandardField`，`name()` 是標準欄名、`label()` 是畫面用語、`describe(&AssetClasses)` 把類別代碼呈現為「代碼 名稱」。是否含個人資料呈現為「是／否」。`AssetUpdate::field_names()` 供一般匯入的預覽使用，輸出不變。
- **D6 補充：CLI 輸出**：`asset import-isms` 依序印出標題（預覽／完成）、「新增 N、更新 N（版本變更 N）、未變 N、版本待補 N 個元件、略過 N 列」、ISMS 報表、匯入差異（未辨識產品由報表的「未辨識的描述行」列出、不在清冊中的資產由「不在 ISMS 清冊」列出，差異中不重複），有待補時最後提醒。報表各類**全部列出**（不再有 `isms_report` 工作表可查，移除 20 筆上限）。輸出由 `isms_lines` 產生，CLI 的整合測試無法開啟加密資料庫，所以預覽與匯入的輸出以 `commands/asset_isms.rs` 的單元測試在記憶體資料庫驗證；整合測試涵蓋說明、資料庫未初始化與 `isms-draft` 已移除。
- **D7 補充：sync 摘要位置**：`cvewatch sync` 在通知之後、結束前印出（輸出末尾）；查詢失敗只記日誌，不影響結束碼。`cvewatch match` 在比對摘要之後印出；資產匯入後的自動比對不另外印（匯入摘要已列出）。
- **D7 補充：每日摘要**：在「無新事項」之後、同步狀態之前另列「版本待補 N 個元件（M 個資產），未納入比對。請在畫面補上版本。」；`DigestData.pending_versions` 取自 `dashboard::load`，不影響 `is_empty`。
- **D8 補充：Dashboard**：GUI 一律顯示版本待補按鈕（沒有時為「版本待補 0」，有時以警示色顯示）；點選後開啟資產畫面、勾選「只顯示版本待補」並取消「顯示已退役」。TUI 一律顯示一行（有待補時粗體）；既有的 Dashboard 快照因此多一行，已更新。
- **D8 補充：資產清單標示**：GUI 在名稱後加「（版本待補）」；TUI 在名稱前加 `glyphs.warn`（Unicode「⚠」、conhost「!」），快照確認欄位不錯位。詳情的元件列為「<顯示名稱> 版本待補（未納入比對）」，未辨識產品與 TUI 的元件版本欄顯示「版本待補」。
- **D8 補充：編輯對話框**：`ComponentDraft::is_untouched_pending()`（既有元件、原版本與目前版本都空白）不檢查必填；版本欄以提示文字「版本待補」標示。core 拒絕把已知版本清空時的訊息為「<編號> 的 <產品> 版本為必填，已有版本 X 不能清成空白」。
- **D8 補充：GUI 匯入預覽**：成功的預覽輸出含「未納入比對」（CLI 的待補提醒）時，以警示色另列「有版本待補的元件：匯入後不參與比對，請在編輯對話框補上版本」（Risks 的緩解）。「從 ISMS 清冊匯入」的檔案對話框取消時，狀態列顯示「已取消從 ISMS 清冊匯入」；匯入成功的狀態列為「已從 ISMS 清冊匯入並重新載入；版本待補的元件請在編輯對話框補上版本」。
- **任務 8.1 的整合驗證**：自動化流程不能建立加密資料庫（`cvewatch init` 會寫入 Credential Manager）也無法操作視窗，所以比照 17，以暫時的 example 程式（未提交、執行後已刪除）用與 `asset import-isms` 相同的 `import_isms`，在記憶體資料庫＋本機 `data/catalog.d` 讀取使用者的 v2.0 清冊，只印數量：預覽與實際匯入都是「可匯入 15 列、略過 30 列（PE 4、IF 17、SE 3，另 6 列個別略過）、新增 15、版本待補 17 個元件」，與 17 的實測（轉換 15、略過 30）一致；重匯為「新增 0、更新 0、未變 15」，新增的待補數為 0。Dashboard 計數為「版本待補 17 個元件（13 個資產），未納入比對」。GUI 畫面未以人工確認，由 `crates/gui/tests` 的 kittest 測試涵蓋。終端輸出的人名未寫入任何檔案。

## Code review 後的修正（一輪，依建議做法）

- **等待區在待補期間被移出**（D2 補充）：比對不載入版本待補元件後，只和該產品有關的等待區 CVE 會被判為不再相關而移出，補上版本後以新的進入時間重新進入（逾期計時歸零）。改為把「只屬於版本待補元件的產品」（`assets::pending::pending_product_keys`，扣除仍有已知版本元件的產品）併入等待區重新評估的略過產品，待補期間維持原狀。spec delta 補上 Scenario: Waiting CVE of a pending product。
- **已可辨識的未辨識產品**（D4 補充）：既有資產的未辨識產品若已可由目錄辨識（例如之後在 catalog.d 加了別名），描述比對到同一產品時不再另加待補元件（以「匯入時會成為的 key」去重）；若它與既有元件的產品重複，略過該列並說明原因，不讓整批因「產品重複」中止。
- **既有元件的產品已不在目錄中**（D4 補充）：該元件匯入時會變成未辨識產品並被移除，與「ISMS 匯入不移除元件」矛盾，所以略過該列並說明須先恢復目錄或在畫面移除該元件。spec delta 的 Invalid rows are skipped 補上這兩種略過情形與 Scenario: Existing component no longer in the catalog。
- **欄位變更以原值比較**（D5 補充）：`changed_fields` 原本比較轉成畫面文字後的值，`None` 與 `"-"` 被當成相同而不列出，但寫入時仍會寫入。改為以原值比較、另外產生顯示文字，預覽與寫入一致。
- **待補提醒的單一來源**（D3、D8 補充）：提醒文字移到 `assets::pending::PENDING_REMINDER`；`asset import` 與 `asset import-isms` 有待補時都以這一整行結尾（一般匯入的摘要行改為「版本待補 N 個元件」），GUI 以整行比對判斷預覽是否有待補，不再依子字串，備註等內容含相同字詞不會誤判。
- **兩個不同的待補數字**（D6 補充）：ISMS 匯入的摘要行改為「匯入後版本待補 N 個元件」（含先前已待補的），報表摘要改為「這次新加入的版本待補元件 M 個」，兩者意義在字面上區分。
- **GUI 狀態列用語**（D8 補充）：匯入完成、比對失敗與匯入失敗的訊息都依匯入種類使用「已從 ISMS 清冊匯入」「ISMS 清冊匯入失敗」，ISMS 匯入的完成與比對失敗訊息都附上補版本的提醒。
- **版本文字的共用函式**：新增 core `plan::version_or(raw, pending)`，CLI 預覽（「待補」）與畫面（「版本待補」）共用同一個判斷。
- **doc comment**：`is_pending_version` 插在 `AssetRecord` 的說明與定義之間，已移回正確位置。
- **未修改：資產說明名稱衝突仍中止整批**：review 建議改為略過，但 spec（Systems from the ISMS workbook 的 System name used by another code）與 D4 明定整批中止、資料庫不變。名稱衝突影響同一代碼下的所有列，略過會讓結果不完整，所以依 spec 保留，錯誤訊息指出衝突的代碼與 ISMS 列號。

## 架構圖判斷

架構圖：不需更新。`git status`／`git diff HEAD` 顯示沒有 workspace `members`、`crates/*/Cargo.toml` 的 `path =` 相依或執行檔（`[[bin]]`、`src/bin/*.rs`）的變更；沒有新的 CVE 來源、HTTP 端點、通知管道、Credential Manager 或工作排程器的互動。ISMS 清冊與一般清冊一樣是 core 讀取的資產清冊檔（圖上的「資產清冊檔」節點），GUI 以子程序執行 `cvewatch.exe` 的方式沿用既有的 companion，只換子命令（`asset import-isms`）。其餘是 core 內部模組（`assets/isms/`、`assets/pending.rs`、`plan_fields.rs`）、CLI 子命令與畫面的變更。`commands/sync.rs` 在 `run_locked` 結尾多 2 行，圖上「sync 流程」的行號範圍仍落在同一個函式內，下次更新架構圖時一併核對。

## Open Questions

（無）
