# Tasks

前提：12-rename-asset-tag 已 archive。測試不呼叫真實外部 API、不存取真實 Credential Manager；core 測試使用記憶體資料庫與 `tests/fixtures/`，GUI 測試使用 `egui_kittest::Harness` 與 `cvewatch_frontend::test_support` 替身。

## 1. core：子類別表與設定

- [x] 1.1 新增 `assets/classes.rs`：`ClassCode`、`MajorClass`、`Cia`（含 `value()`）、`AssetClass`、內建 18 筆子類別（design D1 表格與選項工作表描述原文）。完成條件：單元測試斷言內建清單有 18 筆、`HW01` 範本值 2/3/4、`SW01` 無範本值、`ClassCode::parse("hw05")` 為 `HW05`
- [x] 1.2 `AssetClasses::resolve`：依代碼（不分大小寫）、名稱（NFKC）、舊值對照表（design D3）解析，回傳代碼與是否由舊值轉換。完成條件：單元測試涵蓋「資安設備」→ HW03、`security` → HW03（標記舊值）、`IF01` 與 `printer` 失敗
- [x] 1.3 `config.rs` 新增 `asset_classes: BTreeMap<String, AssetClassConfig>` 與 `AssetClasses::from_config`：代碼格式、新增時 name 必填、C/I/A 全寫或全不寫、名稱不重複（design D2）；範例設定加入註解範例。完成條件：單元測試斷言覆寫 `HW05` 機密性只改該欄、新增 `HW12` 可被解析、`PE06` 與部分 C/I/A 的錯誤訊息指出代碼
- [x] 1.4 測試：`cargo test -p cvewatch-core classes` 與 `config` 相關測試全部通過

## 2. core：資料模型、驗證與 migration

- [x] 2.1 `model.rs`：以 `ClassCode` 取代 `Category`，`AssetRecord` 移除 `criticality`、加入 `cia` 與 `isms_id`；更新 `repo.rs` 的讀寫（`AssetFilter` 的類別篩選改為代碼或大類）。完成條件：`cargo build -p cvewatch-core` 通過
- [x] 2.2 `validate.rs`：`validate_record` 接收 `&AssetClasses`；C/I/A 空白帶範本值、1–4 範圍、SW01 必填；ISMS 編號格式與大類一致（design D4、D5）；錯誤訊息使用 design D4 的用語。完成條件：單元測試重現 spec 的 Template values for a server、Explicit values、Value out of range、Application system without ratings、Malformed ISMS ID、Major class mismatch
- [x] 2.3 `migrations/0007_isms_class.sql` 與 `migrate.rs` 登錄（design D7 步驟 1–7）。完成條件：測試以 0006 版本資料庫建立 `FW-01`（security、重要性 3、notes「機房 A 主防火牆」、2 筆命中）與 `NAS-01`（storage、notes 空白），套用後斷言 `HW03`、2/3/4、notes「機房 A 主防火牆；舊重要性：3」、`NAS-01` notes「舊重要性：2」、命中數不變
- [x] 2.4 ISMS 編號唯一性：寫入前以 `COLLATE NOCASE` 查詢並回報被哪個資產使用（含 retired）；同一清冊兩列重複時列出兩個列號。完成條件：測試重現 spec 的 Duplicate ISMS ID
- [x] 2.5 測試：`cargo test -p cvewatch-core` 通過（含 2.2～2.4，並更新既有測試中的 `Category`／`criticality` 用法）

## 3. core：匯入、匯出、範本與排序

- [x] 3.1 `columns.rs` 新增 `confidentiality`、`integrity`、`availability`、`isms_id`，`criticality` 改為停用欄（design D3、D4 的欄位順序）；`ImportPlan.notices` 與舊值轉換、criticality 停用兩種 notice。完成條件：單元測試斷言 `criticality` 表頭只產生一筆 notice、範本欄位不含 `criticality`
- [x] 3.2 `import.rs`／`plan.rs`：把 `AssetClasses` 傳入解析與比對；`plan.rs` 的欄位差異改列 C/I/A 與 ISMS 編號。完成條件：以新增的 `tests/fixtures/inventory_legacy.xlsx`（舊 category、有 `criticality` 欄）匯入，報表列出 `security 已轉為 HW03 資安設備` 與 criticality 停用 notice
- [x] 3.3 `export.rs` 寫出代碼與實際 C/I/A；`template.rs` 範例列改用 `HW03`、`HW01`、`HW05` 並加入 `classes` 工作表（依目前生效的子類別）。完成條件：測試斷言範本有三個工作表、設定新增 `HW12` 時 `classes` 含 `HW12`；匯出後 dry-run 重匯為「新增 0、更新 0」
- [x] 3.4 `matching/load.rs`、`query.rs`、`scoring.rs` 改用 `asset_value`。完成條件：`matching_scoring.rs` 新增 spec 的 Same scores on different assets 案例
- [x] 3.5 fixtures：`gen_inventory_fixtures.rs` 產出新欄位的 `inventory_basic.xlsx`、`inventory_v2.xlsx` 等，並產出 `inventory_legacy.xlsx`；更新使用這些檔案的測試。完成條件：`cargo run -p cvewatch-core --example gen_inventory_fixtures` 後 `git diff --stat tests/fixtures` 只列出預期檔案
- [x] 3.6 測試：`cargo test -p cvewatch-core` 全部通過

## 4. CLI 與 TUI

- [x] 4.1 CLI：`asset list` 的價值欄、`asset list --category` 接受代碼或 `HW`／`SW`、`asset show` 的類別、C/I/A、ISMS 編號、`finding` 輸出改名；`asset import` 輸出 notices（design D6）。完成條件：`cargo run -p cvewatch -- asset list --help` 顯示類別參數接受子類別代碼或大類
- [x] 4.2 TUI：命中清單「重要性」欄改為「價值」，資產詳情顯示「HW01 伺服器」與 C/I/A。完成條件：TUI 快照測試更新後通過
- [x] 4.3 測試：`cargo test -p cvewatch -p cvewatch-tui` 通過（CLI 以 `assert_cmd` 驗證 help 與參數錯誤，邏輯由 core 測試涵蓋）

## 5. frontend 與 GUI

- [x] 5.1 `frontend/display.rs`：`class_label`、`rating_summary`、未定義代碼顯示（design D8）；`filter.rs` 類別篩選改為代碼加大類。完成條件：單元測試斷言 `HW01` 顯示「HW01 伺服器」、未定義 `HW12` 顯示「HW12（未定義）」
- [x] 5.2 `asset_form.rs`：類別代碼、C/I/A 字串（空白 = 依類別）、`isms_id`；`record_input()` 與 `Workspace` 傳入 `AssetClasses`。完成條件：`crates/frontend/tests/asset_edit.rs` 斷言類別由 `HW01` 改為 `HW05` 時已填的 C/I/A 不變（spec Changing class keeps ratings）
- [x] 5.3 GUI 編輯對話框：類別、C/I/A 下拉選單（「依類別（N）」）、即時資產價值、ISMS 編號欄位與錯誤（design D8）。完成條件：kittest 重現 spec 的 Template values follow the chosen class、Application system needs ratings、ISMS ID with the wrong major class
- [x] 5.4 GUI 資產詳情與匯入預覽：類別「代碼 名稱」、C/I/A、資產價值、ISMS 編號；預覽在版本警告之後列出 notices。完成條件：kittest 重現 spec 的 Bilingual category value、Ratings in the detail pane，並斷言匯入 `inventory_legacy.xlsx` 的預覽出現舊值轉換 notice
- [x] 5.5 測試：`cargo test -p cvewatch-frontend -p cvewatch-gui` 全部通過（含更新既有測試中的 `server`／`security` 類別與重要性）

## 6. 文件與整合驗證

- [x] 6.1 README：清冊欄位表（category 改為子類別代碼、C/I/A、isms_id、criticality 停用）、子類別一覽與 `[asset_classes]` 設定範例、升級說明（先 `cvewatch db backup`、舊值對照表、升級後匯出一次新格式清冊）、風險排序改為資產價值。完成條件：README 出現 `HW01`、`[asset_classes`、`db backup`，不再以「重要性」描述排序
- [x] 6.2 全專案檢查：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`openspec validate 13-isms-asset-class --strict`。完成條件：四個指令皆成功

## Workflow follow-up

- archive 時同步主 spec：`asset-inventory`（移除 Exposure and criticality attributes；新增 Exposure attribute、Asset class、Legacy category values、Asset class configuration、CIA rating and asset value、Legacy criticality column、ISMS asset ID、Migrating existing assets；修改 Blank template export、Creating an asset、Editing asset fields）、`risk-scoring`（修改 Ordering within a tier）、`gui-console`（新增 GUI asset class and rating input；修改 GUI asset editing、GUI asset presentation）
- archive 後執行 `openspec validate --all --strict` 確認通過
- 之後依 proposal 的「後續 change」表以 propose 建立 14-isms-admin-fields
