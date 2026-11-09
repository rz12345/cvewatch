# Design

## Context

- 資產欄位驗證集中在 `crates/core/src/assets/record.rs`：`validate_record(RecordInput, &AssetClasses)` 由清冊匯入（`validate.rs` → `plan.rs`）與畫面編修（`edit.rs`）共用。它是純函式，只依賴子類別表。
- 13 已有「清冊沒有某欄 → 保留既有值＋一則 notice」的先例：`Inventory.keep_isms_ids` 由 `plan.rs` 套用。
- `owner` 是自由文字欄，出現在 `columns.rs`、`record.rs`、`export.rs`、`repo.rs`、CLI `asset show`（「負責人」）、TUI 資產詳情與 GUI 對話框。
- 資料庫是 SQLCipher 加密的 SQLite，連線時已開啟 `foreign_keys`。`component`、`asset_unresolved` 以 `ON DELETE CASCADE` 參照 `asset`，所以不能重建 `asset` 表（13 design D7）。
- 檔案大小：`frontend/src/asset_form.rs` 675 行、`core/src/assets/edit.rs` 719 行、`record.rs` 568 行、`gui/src/dialogs/asset_edit.rs` 432 行。加入新欄位後會接近或超過 800 行上限，需要拆分。

## Goals / Non-Goals

**Goals:**
- 資產說明是資料庫裡的實體，可由 GUI 與清冊 `systems` 工作表維護，匯出後再匯入結果不變
- 新欄位沿用現有驗證路徑：匯入與編修共用同一套規則與錯誤訊息
- 舊清冊、舊設定檔（`[import.columns]` 對應到 `owner`）不需修改就能繼續使用

**Non-Goals:**
- 不改變比對、風險等級與通知；防護等級只用於顯示與日後的 ISMS 匯出
- 不提供資產說明的 CLI 管理指令

## Decisions

### D1. 資產說明存在資料庫，以代碼為外鍵

新增 `asset_system(code TEXT PRIMARY KEY, name TEXT NOT NULL)`，`code` 與 `name` 各有 `COLLATE NOCASE` 的唯一索引。`asset.system_code TEXT REFERENCES asset_system(code)`，可為 NULL；不設 `ON DELETE`（預設 NO ACTION），由 core 先檢查歸屬數並回報易懂的錯誤，外鍵是最後防線。

- 代碼建立後不可修改（spec 的 Asset system），所以用代碼當外鍵是安全的。匯入、匯出與 migration 都直接使用代碼，不需要查 id。
- 替代方案「放在設定檔 `[asset_systems]`」：GUI 就得改寫 config.toml，也無法隨清冊匯入。不採用。
- 替代方案「允許改代碼並連動改 ISMS 編號」：會一次改動多個資產的 ISMS 編號，還得處理編號衝突。不採用；需要時可新增資產說明，再把資產移過去。

### D2. 驗證時傳入資產說明對照表

`validate_record` 的參數改為一個 `RecordContext { classes: &AssetClasses, systems: &SystemDirectory }`。`SystemDirectory` 是「代碼 → 名稱」的不可變快照，可依代碼或名稱（不分大小寫、全形轉半形）查詢。

- 畫面編修：由資料庫載入。
- 清冊匯入：資料庫內容與 `systems` 工作表合併。工作表的列優先，用來判斷 `assets.system` 能否解析。

規則的順序：解析 `system` 欄 → 若為空且有 ISMS 編號，就取 ISMS 編號前三碼查詢 → 找不到時回報「資產說明尚未定義」。兩者都有值時，比對系統代碼是否一致。錯誤以 `StandardField::System` 或 `StandardField::IsmsId` 標示欄位，前端據此把錯誤顯示在對應欄位旁（沿用 13 的 `FieldError` 機制）。

### D3. 清冊欄位

- 新增 `StandardField`：`System`、`ResponsibleUnit`、`Custodian`、`UserUnit`、`Compliance`、`PersonalData`。`assets` 工作表的欄序為：asset_tag、name、category、product、version、exposure、confidentiality、integrity、availability、isms_id、system、responsible_unit、custodian、user_unit、compliance、personal_data、location、notes。
- `Owner` 改為「別名欄」：表頭解析時視同 `Custodian`，並記錄一則警告；同一工作表同時出現兩者就回報表頭錯誤。`[import.columns]` 中對應到 `owner` 的自訂欄名同樣適用（不必修改設定檔）。範本與匯出只寫 `custodian`。這和 `Criticality`（停用、匯入時忽略）不同，`Owner` 的值仍會匯入。
- 新增 `SheetKind::Systems`（必填 `code`、`name`），工作表名稱為 `systems`。`classes` 工作表仍被忽略。
- `personal_data` 接受 `Y`／`N`／`是`／`否`（不分大小寫、全形轉半形）。匯出寫 `Y`／`N`，與 ISMS 清冊一致。

### D4. 「缺少的欄」一般化

把 `Inventory.keep_isms_ids: bool` 改為 `kept_fields: BTreeSet<StandardField>`，列出 `assets` 工作表缺少的可保留欄（isms_id 與 D3 的六個新欄）。`plan.rs` 對既有資產，把這些欄位從資料庫的值複製過來。新資產則以未填或預設值建立：資產說明仍可由 ISMS 編號帶出，是否含個人資料用子類別的預設值。

notice 的處理：isms_id 維持 13 的原文案；新欄位合併成一則「清冊沒有 system、custodian 欄，保留資料庫中既有資產的值」。這樣既有測試與文件的 isms_id 文案不必改。

### D5. 資產說明的匯入與編修

- 匯入計畫（`ImportPlan`）新增 `systems: Vec<SystemChange>`（新增，或名稱 A → B），會出現在 `--dry-run` 與 GUI 預覽的明細中。寫入時，在同一交易內先 upsert 資產說明，再寫資產。
- `systems` 工作表內的代碼或名稱重複、與資料庫中「其他代碼」的名稱衝突，都是 `systems` 工作表的列錯誤，整批中止。
- 新模組 `core/src/assets/systems.rs`：`AssetSystem` 型別、讀取、`create_system`、`rename_system`、`delete_system`（先查歸屬數，含 retired）。GUI 透過 `frontend` 呼叫，不經過 `EditOp`。資產說明的變更不影響比對，所以不記錄「比對待補」。
- 永久刪除資產不會刪除資產說明。資產更名（12）也不受影響。

### D6. 防護等級、法規遵循性與個資

- `AssetRecord` 新增 `system_code: Option<String>`、`responsible_unit`、`custodian`、`user_unit: Option<String>`、`compliance: Option<Rating>`、`personal_data: bool`；移除 `owner`。
- 防護等級不儲存，由 `AssetRecord::protection_level() -> Option<u8>` 計算：`compliance.map(|d| max(cia.value(), d))`。spec 規定未評定時不顯示數值，所以 D 留空時回傳 None。
  - 替代方案「D 留空時等於資產價值」：數學上合理，但 ISMS 清冊在 D 未填時，MAXABCD 也留空。為了和使用者交給 ISMS 的資料一致，不採用。
- `personal_data` 以解析後的值儲存（`INTEGER NOT NULL DEFAULT 0 CHECK (personal_data IN (0, 1))`），不保存「依類別」。這和 C/I/A 帶入範本值的作法相同，修改預設值也不會影響既有資產。
- `AssetClass` 新增 `personal_data: bool`，內建全為 false（v2.1 選項工作表的 HW／SW 列都是 N）。設定檔 `[asset_classes.<代碼>]` 新增選填的 `personal_data = true|false`；TOML 用布林值，不用 Y/N。

### D7. Migration `0008_isms_admin.sql`

```sql
CREATE TABLE asset_system (code TEXT PRIMARY KEY, name TEXT NOT NULL);
CREATE UNIQUE INDEX idx_asset_system_code ON asset_system (code COLLATE NOCASE);
CREATE UNIQUE INDEX idx_asset_system_name ON asset_system (name COLLATE NOCASE);
ALTER TABLE asset ADD COLUMN system_code TEXT REFERENCES asset_system (code);
ALTER TABLE asset ADD COLUMN responsible_unit TEXT;
ALTER TABLE asset ADD COLUMN custodian TEXT;
ALTER TABLE asset ADD COLUMN user_unit TEXT;
ALTER TABLE asset ADD COLUMN compliance INTEGER CHECK (compliance BETWEEN 1 AND 4);
ALTER TABLE asset ADD COLUMN personal_data INTEGER NOT NULL DEFAULT 0 CHECK (personal_data IN (0, 1));
UPDATE asset SET custodian = owner;
INSERT OR IGNORE INTO asset_system (code, name)
    SELECT DISTINCT upper(substr(isms_id, 1, 3)), upper(substr(isms_id, 1, 3))
    FROM asset WHERE isms_id IS NOT NULL;
UPDATE asset SET system_code = upper(substr(isms_id, 1, 3)) WHERE isms_id IS NOT NULL;
```

- `owner` 欄保留在資料表，但程式不再讀寫，和 13 的 `criticality` 作法相同，不必重建 `asset` 表。
- 名稱暫時等於代碼，會同時滿足兩個唯一索引；使用者之後在 GUI 改名。
- `active_component` view 不受影響。

### D8. 前端與 GUI

- `frontend::data::Snapshot` 載入資產說明。新增 `asset_groups(&AssetListFilter) -> Vec<AssetGroup>`：依代碼排序，「未歸屬」在最後，空組不輸出，每組附資產數與未處理命中總數。
- GUI 資產清單：每組是一個 `CollapsingHeader`（以代碼作 id），組內沿用現有三欄表格；每組各自有 `id_salt`。收合狀態只存在 egui 記憶體，不寫入設定。原本 `list_table` 的列繪製（`list_row`）不需修改。
- 新增 `gui/src/dialogs/asset_systems.rs`：資產說明管理對話框，包含清單、新增列與各列的改名、刪除。錯誤顯示在欄位旁；資料寫入後重新載入快照。
- 資產對話框新增「ISMS 管理」區塊，包含資產說明、權責單位、保管人、使用單位、法規遵循性、是否含個人資料，以及即時顯示的防護等級。
- 拆分檔案，避免超過 800 行：
  - `frontend/src/asset_form.rs` 改為 `asset_form/` 模組，管理欄位放在 `asset_form/admin.rs`
  - `gui/src/dialogs/asset_edit.rs` 的新區塊放在 `dialogs/asset_admin.rs`
  - `core/src/assets/record.rs` 的新欄位解析放在 `record_admin.rs`
- TUI 資產詳情的「負責人」改為「保管人」，並加上資產說明。CLI `asset show` 同樣調整，並顯示權責單位、使用單位、法規遵循性、防護等級與個資。

### D9. 測試策略

- core：migration 測試（從 0007 狀態的資料庫升級：owner、ISMS 編號、退役資產、命中數不變）。另外逐一測試 spec 的匯入與編修 scenario，包括 `systems` 工作表、別名欄、缺欄保留、系統代碼不符、個資值。
- export → import `--dry-run` 往返（含資產說明）。
- frontend：表單驗證錯誤對應到欄位、`asset_groups` 的分組與排序。
- GUI：沿用 `crates/gui/tests` 的 egui 測試方式，測試分組清單、收合標題的未處理數、資產說明對話框，以及詳情欄位。
- fixtures：`gen_inventory_fixtures.rs` 產生含 `systems` 工作表的清冊，以及只有 `owner` 欄的舊清冊；資料一律使用虛構的單位、人名與系統代碼（例如 `NET`、`SYS`、王小明）。

## Risks / Trade-offs

- [使用者的外部工具讀取匯出檔的 `owner` 欄] → 匯出改為 `custodian`。在 docs 的清冊欄位說明與 release note 註明；匯入仍接受 `owner`。
- [GUI 和清冊匯入同時改資產說明名稱，匯入會覆蓋 GUI 的修改] → 和資產欄位的覆寫行為一致。預覽中列出名稱變更，GUI 的覆寫提醒涵蓋這種情況。
- [migration 建立的資產說明名稱就是代碼，看起來像資料不完整] → 升級後第一次開啟資產畫面時，分組標題會顯示「SYS SYS」，使用者可以一眼看出要改名。不另加提示。
- [分組清單的鍵盤操作：原本只有一個表格] → 選取仍以 `asset_tag` 記錄，Enter／Delete 只看選取的資產，與表格數量無關。GUI 測試涵蓋跨組選取。
- [收合的組裡有被選取的資產] → 詳情照常顯示；清單不會自動展開，保持使用者的操作。

## Migration Plan

1. `0008_isms_admin.sql` 隨程式升級自動執行（加密資料庫的 migration 流程不變）。
2. 升級後建議使用者到 GUI「資產說明…」把名稱改成正式名稱，再匯出一次新格式的清冊。
3. 回退：不提供降版 migration。舊版程式遇到較新的 schema 版本會拒絕開啟資料庫（`migrate.rs` 的既有行為），要回退請從升級前的資料庫備份還原。

## Apply 階段的決定（依建議做法，未另行詢問）

- **清冊匯入的資產說明延後到規劃階段解析**（修正 D2）：`parse_inventory` 不連資料庫（CLI 與測試都在開啟資料庫前呼叫），因此它只驗證欄位格式，保留 `system` 欄原文（`IncomingAsset.system_raw`），由 `plan_import` 合併資料庫與 `systems` 工作表的資產說明後解析。結果是**兩階段回報**：欄位格式錯誤（法規遵循性、個資等）先回報；全部修正後，才會列出「資產說明尚未定義」與系統代碼不符。畫面編修仍在 `validate_record` 一次檢查（`RecordContext` 含資料庫的資產說明）。使用說明已寫明這一點。
- **新增 `FieldErrorKind::SystemMismatch` 與 `UnknownSystem`**：讓畫面區分「系統代碼與資產說明不符」「系統代碼尚未建立資產說明」與 ISMS 編號格式錯誤，不必解析 core 的訊息文字。ISMS 編號帶出的系統代碼不存在時，錯誤標在「ISMS 編號」欄；資產說明欄填了不存在的值時，錯誤標在「資產說明」欄。
- **清冊沒有 `system` 欄時**，以資料庫中的資產說明代入，再和最終的 ISMS 編號一起檢查。舊清冊改了 ISMS 編號的系統代碼時會被拒絕，使用者要補上 `system` 欄；這是保守的作法，不會靜默換掉資產說明。
- **`systems` 工作表互換兩個名稱**：寫入時先把改名的資產說明換成以控制字元開頭的暫時名稱，再寫最終名稱，避免暫時違反名稱的唯一索引。作法比照 13 的 ISMS 編號互換。
- **`EditOp::Fields` 改為 `Box<RecordInput>`**：`RecordInput` 增加 6 個欄位後，clippy 的 `large_enum_variant` 會報錯。
- **GUI 資產對話框的「ISMS 管理」區塊放在一般欄位的右側**，不放在下方：放下方時對話框變高，元件區會被捲出可視範圍。水平排列中不使用 `separator`，因為它會撐滿捲動區的高度。
- **資產說明管理對話框新增列的標籤為「新資產說明代碼」「新資產說明名稱」**，避免與資產清單的「名稱」欄同名。改名與刪除按鈕帶代碼（「改名 NET」「刪除 NET」）。
- **CLI `asset show` 的輸出以 `admin_lines` 的單元測試驗證**：CLI 整合測試不能存取真實的 Credential Manager，執行檔無法開啟加密資料庫（12、13 的既有作法）。
- **GUI 停用按鈕的滑鼠提示**需要停留時間，kittest 無法穩定觸發，所以測試只確認按鈕停用，並另外比對提示文字（`delete_blocked`）與 `Workspace::delete_system` 的回報。
- **TUI 資產詳情**：「負責人」改為「保管人」，並在同一列加上資產說明；其他管理欄位不在 TUI 顯示，因為 TUI 的資產需求沒有改變。
- **任務 5.6 的手動目視檢查**：自動化流程中無法開啟視窗目視確認，改列在 tasks.md 的 Workflow follow-up，請使用者執行；自動化部分由 kittest 測試涵蓋（分組、對話框、欄位值與錯誤訊息）。

## Code review 後的調整

- **清冊沒有 `system` 欄、但有新的 ISMS 編號時，由新編號重新帶出資產說明**，不沿用資料庫的值。資料庫的值可能只是由舊編號帶出（例如 migration 0008），原本的作法會擋下合理的重新編號。沒有新的 ISMS 編號時，仍沿用資料庫的值。此項修正 apply 決定中的「沒有 system 欄」一條。
- **所有資產說明名稱的比對都改用 NFKC＋不分大小寫**：`systems` 工作表內的重複、與資料庫名稱的衝突，都改用與查詢相同的 `name_key`。之前用 `to_lowercase`，全形與半形的同名會被放過。
- **新增規則：名稱不可等於其他資產說明的代碼，代碼也不可等於其他資產說明的名稱**（spec 的 Asset system 已補上規則與情境）。查詢時先比代碼、再比名稱，若允許這兩種情況，依名稱查詢會對應到錯的資產說明。名稱等於自己的代碼（migration 建立的「SYS SYS」）仍可以。
- **`systems` 工作表只認標準欄名**，`[import.columns]` 的自訂對應不套用；`code` 也不是可對應的目標。否則把 `code` 對應到 `asset_tag` 的設定，會讓範本與匯出檔都無法匯入。
- **畫面編修在其他欄位有錯時，也一併檢查資產說明與 ISMS 編號的系統代碼**，一次列出全部錯誤。只有 ISMS 編號本身格式錯誤時，才不再比對系統代碼。
- **防護等級、法規遵循性與資產說明的解析統一由 core 提供**：`compliance_value`、`protection_level`、`resolve_system`。frontend 的即時顯示直接呼叫，不另外實作。「未評定」與「是／否」的文字也統一用 frontend 的 `rated_text`、`personal_data_label`，CLI 與 GUI 共用。
- **GUI 的法規遵循性與個資選單改成只建立一次**（`LazyLock`）；資產清單分組改為一次掃描（`BTreeMap`）。

## 架構圖

不需更新：沒有 crate 增刪或相依變更（各 `Cargo.toml` 未修改），也沒有新的外部整合。變更都在 core 內部模組（`assets/systems.rs` 等）、資料庫 schema（`0008_isms_admin.sql`）與 CLI／TUI／GUI 的畫面與欄位。
