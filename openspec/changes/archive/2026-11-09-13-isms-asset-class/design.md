# Design

## Context

- 機構的 ISMS「資訊資產清冊」以「資產類別（PE/IF/SW/HW/SE）＋資產名稱（子類別，例如 HW01 伺服器）」分類。選項工作表為每個子類別提供描述與 C/I/A 範本值；資產價值 = MAX(C, I, A)，範圍 1–4
- 目前 `Category` 是 `string_enum!` 產生的固定列舉（`crates/core/src/assets/model.rs`），資料庫 `asset.category` 為 `TEXT NOT NULL`，沒有 CHECK。`criticality` 為 `INTEGER NOT NULL DEFAULT 2 CHECK (criticality IN (1,2,3))`
- 重要性只用在：風險排序（`matching/scoring.rs` 的 `cmp_findings`）、`active_component` view（0004）、命中查詢（`matching/query.rs`）、各介面的顯示。比對規則與風險等級不使用 category 或重要性
- migration 在單一交易內套用（`db/migrate.rs`）。重建 `asset` 表需要 DROP TABLE，而 `component`、`asset_unresolved` 以 `ON DELETE CASCADE` 參照 `asset`，在交易內 DROP 會連帶刪除元件
- 匯入報表目前只有版本警告（`ImportPlan.warnings: Vec<VersionWarning>`）
- 未對應到標準欄位的表頭會被忽略（`ColumnMap::locate`）

## Goals / Non-Goals

**Goals:**

- 子類別與 C/I/A 規則集中在 core，CLI、TUI、GUI 共用
- 舊清冊（舊 category 值、`criticality` 欄）仍可匯入，不需要使用者先改檔
- migration 不重建 `asset` 表、不動元件與命中

**Non-Goals:**

- 不做 ISMS 的其他管理欄位、ISMS 格式匯出（見 proposal 的後續 change）
- 不刪除 `asset.criticality` 實體欄位

## Decisions

### D1. 子類別表 `AssetClasses`

- `model.rs` 以 `ClassCode`（已驗證的代碼字串，大寫，形如 `HW01`）取代 `Category` 列舉；`MajorClass { Hardware, Software }` 由代碼前兩碼推得
- 新模組 `assets/classes.rs`：
  - `AssetClass { code, name, description, template: Option<Cia> }`；`Cia { confidentiality, integrity, availability }`，各為 1–4
  - `BUILTIN`：v2.1 的 18 筆，範本值取自選項工作表 D–F 欄：

    | 代碼 | 名稱 | C/I/A | 代碼 | 名稱 | C/I/A |
    |---|---|---|---|---|---|
    | HW01 | 伺服器 | 2/3/4 | SW01 | 應用系統 | 無 |
    | HW02 | 通訊設備 | 2/3/4 | SW02 | 作業系統 | 2/3/3 |
    | HW03 | 資安設備 | 2/3/4 | SW03 | 資料庫管理系統 | 2/3/3 |
    | HW04 | 週邊設備 | 2/3/3 | SW04 | 網站服務系統 | 2/3/3 |
    | HW05 | 儲存設備 | 2/3/4 | SW05 | 應用工具軟體 | 2/3/3 |
    | HW06 | 個人電腦 | 2/3/3 | SW06 | 資通安全系統 | 2/3/3 |
    | HW07 | 公共使用電腦 | 2/3/2 | SW07 | 一般套裝軟體 | 2/2/2 |
    | HW08 | 日常作業事務機器 | 2/3/3 | | | |
    | HW09 | 行動裝置 | 2/3/3 | | | |
    | HW10 | 物聯網設備 | 2/2/3 | | | |
    | HW11 | 基礎設施 | 3/3/4 | | | |

    描述沿用選項工作表 C 欄原文（HW11 名稱去除尾端空白）
  - SW01 在原表的資產價值為 0（依個案評定），因此 `template = None`，C/I/A 必填（spec 的 Application system without ratings）
  - `AssetClasses::resolve(raw) -> Resolved`：先試代碼（不分大小寫），再試名稱（NFKC 後完全相同），再試舊值（D3）；回傳代碼與「是否由舊值轉換」
- 載入方式：`AssetClasses::from_config(&config.asset_classes)`，與 `Catalog` 一樣在指令開頭建立一次，傳給匯入、編修、範本、匯出與前端

替代方案：沿用 `string_enum!` 列舉加 18 個變體。否決，因為設定檔需要能新增子類別。

### D2. 設定檔 `[asset_classes.<代碼>]`

```toml
[asset_classes.HW05]
confidentiality = 3          # 只覆寫有寫的欄位

[asset_classes.HW12]
name = "實驗儀器"
description = "示波器、量測儀器等具網路功能的設備"
confidentiality = 2
integrity = 3
availability = 3
```

- 代碼以 `^(HW|SW)\d{2}$` 驗證（不分大小寫，存為大寫）；不符時 `ConfigError` 指出代碼（spec 的 Class code outside HW and SW）
- 覆寫內建子類別：沒寫的欄位沿用內建值。新增子類別時 `name` 必填；C/I/A 三項要全寫或全不寫（全不寫 = 無範本值，同 SW01），只寫一部分時設定檔載入失敗
- 名稱不可與其他子類別重複（NFKC 後比較），否則載入失敗
- 不提供刪除內建子類別。資料庫中出現未定義的代碼（例如設定檔刪除了自訂子類別）時，資產仍可讀取，顯示為「HW12（未定義）」；編修該資產時要求改選已定義的子類別
- `config.rs` 的範例設定加入一段註解範例；GUI 設定頁不提供編輯（Out of scope）

### D3. 舊值轉換與警告

- 對照表放在 `classes.rs`：network → HW02、security → HW03、virtualization → HW01、server → HW01、storage → HW05、host → HW01（host 多為 Linux/Windows 伺服器；個人電腦請改為 HW06）
- 舊值只在**清冊匯入**接受；GUI 與 CLI 的編修只列出已定義子類別
- 匯入報表新增 `ImportPlan.notices: Vec<ImportNotice { row, asset_tag, message }>`，與版本警告分開，CLI 與 GUI 預覽在版本警告之後列出：
  - `第 5 列 FW-01：security 已轉為 HW03 資安設備`
  - `清冊的 criticality 欄已停用，改用 confidentiality、integrity、availability`（整批一次，`row = None`）
- `StandardField::Criticality` 保留為「停用欄位」：`columns()` 不再包含它（範本與匯出不寫出），`locate` 認得表頭時只記錄一筆 notice。`[import.columns]` 指向 `criticality` 時設定仍可載入，匯入時給出同一個 notice

### D4. 清冊欄位

- assets 工作表欄位順序：`asset_tag, name, category, product, version, exposure, confidentiality, integrity, availability, isms_id, location, owner, notes`
- `category` 匯出寫代碼（`HW01`），範例列改為 `HW03`、`HW01`、`HW05`
- C/I/A 留空 → 範本值；有填 → 必須是 1–4 的整數（`2.0` 這類浮點儲存格沿用 `sheet.rs` 的整數處理）
- 匯出寫出實際值（不留空），因此匯出再匯入不會因為範本值改變而變動（round trip 維持「更新 0」）
- `classes` 工作表：`code, name, description, confidentiality, integrity, availability`（無範本值時留空）；匯入時不讀
- 錯誤訊息用語：「第 12 列：可用性「5」必須是 1–4」「第 7 列：SW01 應用系統必須填寫機密性、完整性與可用性」

### D5. ISMS 編號

- 格式 `^[A-Za-z0-9]{3}-(HW|SW)-\d{3}$`，比對時不分大小寫，存放時保留使用者輸入（去除前後空白）；中間兩碼 MUST 與子類別大類相同
- 唯一性：資料庫加 `CREATE UNIQUE INDEX idx_asset_isms_id ON asset (isms_id COLLATE NOCASE) WHERE isms_id IS NOT NULL`；core 在寫入前先以 `COLLATE NOCASE` 查詢並回報被哪個資產使用（含 retired），index 是最後防線
- 同一份清冊內兩列使用相同 ISMS 編號：與重複 `asset_tag` 一樣在驗證階段列出兩個列號並中止
- 不自動產生流水號（Out of scope）

### D6. 資產價值取代重要性

- `AssetRecord` 移除 `criticality`，加入 `cia: Cia` 與 `isms_id: Option<String>`；`Cia::value()` = 三者最大值
- `active_component` view 重建，以 `max(a.confidentiality, a.integrity, a.availability) AS asset_value` 取代 `a.criticality`
- `matching/load.rs`、`query.rs`、`scoring.rs`：欄位改名為 `asset_value`，排序規則不變（由高到低）
- 介面顯示：CLI `asset list` 的「重要性」欄改為「價值」；`asset show` 顯示「C/I/A 2/3/4（價值 4）」與 ISMS 編號；TUI 命中清單的重要性欄改為「價值」；`finding` 輸出同步改名

### D7. Migration `0007_isms_class.sql`

不重建 `asset` 表（Context 所述 CASCADE 風險），步驟：

1. `ALTER TABLE asset ADD COLUMN confidentiality INTEGER NOT NULL DEFAULT 2 CHECK (confidentiality BETWEEN 1 AND 4)`，`integrity`、`availability` 相同；`ADD COLUMN isms_id TEXT`
2. 先依**舊值**設定 C/I/A（此時 category 仍是舊值）：`UPDATE asset SET (confidentiality, integrity, availability) = (CASE category …)`，對照 D1 表中轉換後子類別的內建範本值（HW01/HW02/HW03/HW05 均為 2/3/4）
3. 附記舊重要性：`notes = CASE WHEN notes IS NULL OR trim(notes) = '' THEN '舊重要性：' || criticality ELSE notes || '；舊重要性：' || criticality END`
4. 轉換類別：`UPDATE asset SET category = CASE category WHEN 'network' THEN 'HW02' … END`；不在對照表中的值維持原樣（理論上不存在，視為未定義代碼，見 D2）
5. 建立 `idx_asset_isms_id`
6. `DROP VIEW active_component` 後以 `asset_value` 重建（其餘欄位不變）
7. `criticality` 欄保留（NOT NULL DEFAULT 2 仍成立），程式不再讀寫；新寫入的列使用預設值 2

- migration 只能使用內建範本值，不讀設定檔的覆寫；在 README 升級說明中註明
- migration 沿用既有的單一交易套用，失敗時整批回復。目前開啟資料庫時不會自動備份，且新版的 `db backup` 也先開啟（並升級）資料庫，因此 README 升級說明請使用者在替換執行檔之前以舊版執行 `cvewatch db backup <path>`（code review R2）

### D8. 前端與 GUI

- `frontend/display.rs`：`class_label(code, classes)` → 「HW01 伺服器」；未定義 → 「HW12（未定義）」；`rating_summary(cia)` → 「C2 / I3 / A4（價值 4）」
- `asset_form.rs`：`category` 改為子類別代碼；C/I/A 各為 `String`，空字串 = 依類別；`record_input()` 傳給 core 驗證。新增 `isms_id`
- GUI 對話框：類別 `ComboBox` 依代碼排序列出「代碼 名稱」；C/I/A 三個 `ComboBox`，第一項「依類別（N）」（無範本值時為「依類別（無預設）」，儲存時視為未填）；資產價值以唯讀標籤即時顯示，C/I/A 有缺時顯示「—」；ISMS 編號 `TextEdit`
- 篩選（`frontend/filter.rs`）：類別篩選改為子類別代碼清單，另提供「實體（HW）」「軟體（SW）」兩個大類選項
- TUI：只改顯示（命中清單的價值欄、資產詳情），TUI 不編修資產

### D9. 測試策略

- core 單元：`classes.rs`（解析代碼／名稱／舊值、設定覆寫、新增代碼驗證、名稱重複、部分 C/I/A）、`validate.rs`（範本值、超出範圍、SW01 必填、ISMS 格式與大類不符）
- core 整合（記憶體資料庫）：匯入舊清冊（舊值 notice、criticality notice）、ISMS 編號重複（含 retired）、round trip、migration：以 0006 版本的資料庫加入 `security`/重要性 3 的資產與命中，套用 0007 後斷言類別、C/I/A、notes 與命中數
- 排序：`matching_scoring.rs` 加入「同分不同資產價值」案例
- frontend：表單「依類別」解析、類別切換不改已填值（spec Changing class keeps ratings）
- GUI kittest：spec 的 Template values follow the chosen class、Application system needs ratings、ISMS ID with the wrong major class、Bilingual category value、Ratings in the detail pane
- fixtures：`gen_inventory_fixtures.rs` 產出新欄位清冊，另保留一份舊格式清冊（`inventory_legacy.xlsx`）供相容測試

## Risks / Trade-offs

- [舊重要性附記會改變 notes] → 使用者可自行刪除；附記讓使用者能比對轉換前後的價值，比靜默丟棄好
- [migration 用內建範本值] → 設定覆寫只影響之後新增或留空匯入的資產；README 說明升級後可在 GUI 調整或重新匯入
- [`criticality` 實體欄位殘留] → 不影響行為；日後若需要重建資料表，再一併移除
- [host → HW01 不一定正確] → 匯入報表與 migration 後的 notes 都可讓使用者發現並改為 HW06
- [舊清冊重匯時 C/I/A 欄不存在] → 這時 C/I/A 依範本值決定，會覆寫使用者在 GUI 調整過的值。dry-run 的「更新」明細會列出 C/I/A 的變化，README 建議升級後先 `asset export` 一份新格式清冊

## Migration Plan

1. **替換執行檔之前**以舊版的 `cvewatch db backup <path>` 備份（新版的任何指令，包含 `db backup` 與排程的 `sync`，第一次開啟資料庫時即套用 0007）；升級後第一次開啟資料庫時自動套用 0007
2. 使用者執行 `cvewatch asset export` 取得新格式清冊，在其中調整類別（例如 host → HW06）與 C/I/A
3. 之後以新格式清冊維護；舊清冊仍可匯入，但會帶 notice

回復：以 `cvewatch db restore` 還原升級前的備份，並改回舊版執行檔（新版資料庫無法以舊版開啟）。

## Apply 階段的決定（依建議做法，未另行詢問）

- **A1 驗證模組拆分**：單筆欄位驗證移到 `assets/record.rs`（`validate.rs` 重新匯出 `validate_record` 等），`validate.rs` 只留整份清冊的解析；`validate_record` 為畫面編修（不接受舊值），`validate_import_record` 為清冊匯入（接受舊值並回傳原值供 notice）
- **A2 SW01 必填的錯誤**：每個空白的 C/I/A 各一筆 `FieldError`（`Missing`，GUI 在三個欄位旁各顯示「必填」），訊息相同且只列出缺少的項目（例如「SW01 應用系統必須填寫完整性與可用性」）；清冊匯入時同一列相同訊息只列一次
- **A3 ISMS 編號的錯誤種類**：新增 `FieldErrorKind::ClassMismatch`（大類不符）；格式不符沿用 `NotAllowed`。GUI 用語為「格式應為「系統代碼-大類-流水號」，例如 SYS-HW-001」「大類（HW／SW）與類別不符」
- **A4 ISMS 編號重複的訊息**：「ISMS 編號 sys-hw-005 已被 NAS-02 使用（該資產已退役）」，匯入與 core 編修共用（`plan::isms_in_use_message`）；GUI 另以載入的資料先行檢查，錯誤標在「ISMS 編號」欄位（與更名的 `rename_conflict` 相同做法），core 仍在交易內再查
- **A5 匯入互換 ISMS 編號**：重複檢查以「匯入後的狀態」判斷（清冊中的資產改用清冊的編號）；寫入前先清除這次會變更的編號，避免逐筆寫入時暫時違反唯一索引
- **A6 類別錯誤訊息**：列出目前生效的全部子類別代碼（含設定檔新增者），例如「category「printer」不是允許的值（子類別代碼 HW01、…、SW07 或子類別名稱）」
- **A7 名稱比對**：子類別名稱以 NFKC 後完全相同比對（不轉小寫）；舊值與代碼不分 ASCII 大小寫
- **A8 設定檔驗證時機**：`[asset_classes]` 在載入設定檔時即驗證（與 `[import.columns]` 相同，錯誤訊息含欄位 `asset_classes` 與代碼），`Config::asset_classes()` 取得生效的子類別表；`[asset_classes.X]` 內的未知欄位與其他區段一樣只產生警告。`[import.columns]` 仍可指向 `criticality`（停用欄），錯誤訊息列出的可用欄位不含停用欄
- **A9 資料庫中無法解析的類別**：讀取時代碼格式不符（理論上不存在，0007 只留下對照表外的舊值）與原本 `Category` 一樣視為資料錯誤；格式正確但未定義的代碼（例如設定檔刪除了 HW12）可讀取並顯示「HW12（未定義）」，編修時要求改選
- **A10 前端類別篩選**：目前 GUI／TUI 的資產清單沒有類別篩選控制項，因此只在 frontend 提供 `AssetListFilter.class`（子類別或大類）與 `filter::class_filter_choices`（全部、實體（HW）、軟體（SW）、各子類別），GUI 不新增控制項（不改變既有工具列配置）；CLI 以 `asset list --category <代碼|HW|SW>` 提供
- **A11 CLI 參數錯誤**：`--category` 不是代碼或大類時由 clap 拒絕，結束碼沿用既有的參數錯誤（1，與未知子命令相同）
- **A12 TUI 版面**：資產清單的類別欄只顯示代碼（欄寬 14 → 6，名稱欄因此變寬）、「重要」欄改為「價值」；資產詳情第一行為「類別 HW01 伺服器  曝露度 …  C2 / I3 / A4（價值 4）  ISMS 編號 …」，地點與負責人移到第二行
- **A13 GUI 下拉選單高度**：選單最大高度設為 480，內建 18 個子類別不必捲動即可看到與點選
- **A14 匯入預覽的 notice**：GUI 預覽顯示 `asset import --dry-run` 的輸出，CLI 的明細改由 `plan_detail_lines` 產生，notice（「提醒：」）列在版本警告之後、清冊中消失之前；以 `inventory_legacy.xlsx` 在 CLI 單元測試驗證順序，GUI 測試以 fake companion 驗證預覽顯示
- **A15 範本範例列**：`FW-01`（HW03、ISMS 編號 `SYS-HW-001`、C/I/A 留空）、`SRV-01`（HW01、機密性 3）、`NAS-01`（HW05、C/I/A 留空），示範範本值與覆寫
- **A16 fixtures**：`inventory_basic.xlsx` 等改為新欄位（混用代碼、子類別名稱與小寫代碼）；`inventory_legacy.xlsx` 保留舊格式；`tests/fixtures/matching/inventory*.csv` 維持舊格式以持續驗證相容匯入

## Code review 後的調整

- **R1 舊清冊不清除 ISMS 編號**：清冊（assets 工作表）沒有 `isms_id` 欄時，既有資產沿用資料庫中的 ISMS 編號（`Inventory.keep_isms_ids`），有既有編號被保留時報表給出一次「清冊沒有 isms_id 欄，保留資料庫中既有資產的 ISMS 編號」；有該欄但留空仍表示沒有 ISMS 編號。C/I/A 欄不存在時維持原設計（依範本值，dry-run 列出差異），見 Risks
- **R2 升級前備份**：見 D7 與 Migration Plan，README 改為「替換執行檔之前以舊版備份」
- **R3 全形代碼**：`ClassCode::parse` 先做 NFKC，`ＨＷ０１` 視同 `HW01`
- **R4 代碼形式的名稱**：設定檔的子類別名稱不可是代碼形式（例如 `[asset_classes.HW12] name = "HW03"`），否則載入失敗——解析時先比對代碼，這種名稱永遠選不到該子類別
- **R5 重複程式碼**：「代碼 名稱／（未定義）」集中在 `AssetClasses::label`（frontend `class_label`、CLI `asset show`、匯入 notice 共用）；`rename_conflict` 與 `isms_conflict` 共用 `used_by`；C/I/A 欄位對應集中在 `Cia::rating`／`Cia::to_array`
- **R6 GUI 每幀配置**：子類別選單在開啟對話框時建立一次（`AssetEdit::new(form, classes)`），C/I/A 選單不再複製選項，每幀只解析一次目前的子類別（`AssetForm::asset_value_with`、`effective_rating_with`）
- **R7 檔案大小**：`[asset_classes]` 的反序列化與其測試移到 `assets/classes.rs`，`config.rs` 回到 800 行以內
