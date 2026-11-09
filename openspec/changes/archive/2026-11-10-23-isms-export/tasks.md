# Tasks

前提：19-system-code-edit、20-asset-identity-network、21-component-roles、22-asset-lifecycle-eol 已依序 archive。本 change 依賴 20 在資產上新增的**廠牌、型號**欄位（20 的 proposal 命名為 `brand`、`model`；apply 時以 20 封存後的實際名稱為準）。本 change 沒有 MODIFIED 區塊；apply 前仍先依前一個 change 封存後的主 spec 重新核對：`isms-inventory` 與 `gui-console` 中被引用的 requirement（ISMS import command、Description to components、Protection level、GUI ISMS import、GUI asset presentation 的工具列分組）仍與本 change 的 ADDED requirement 一致，若 20–22 已改動 GUI 工具列分組的 requirement，將「匯出 ISMS 清冊」的位置一併核對。

測試不呼叫真實外部 API、不存取真實 Credential Manager；core 測試使用記憶體資料庫（`tests/common::memory_db`）與 `tempfile` 目錄；目錄使用內建種子＋ `tests/fixtures/isms_catalog.d`（其中虛構別名 `Aruba XYZ-100` 對應 `hpe-aoscx`，正好用來驗證廠牌型號行不會被推測成產品）。fixture 與測試只用虛構的型號、人名、系統代碼與編號（`Aruba XYZ-100`、「王小明」、`SYS`／`NET`、`FW-EDGE`、`SRV-01`），不寫入實際使用中的版本；commit 前執行 `scripts/check-sensitive.ps1`。每完成一個任務群組就執行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test`，並勾選本檔。

## 1. core：描述組成與讀回檢查

- [x] 1.1 `workbook.rs` 測試中的 16 欄 `HEADERS` 改為公開常數 `ISMS_V21_HEADERS`，讀取端測試改用它；`describe.rs` 的 `SEPARATORS` 改為 `pub(crate)`，新增 `pub(crate) const NOTE_BRACKETS`（design D2）。完成條件：`cargo test -p cvewatch-core isms` 既有測試全部通過、輸出不變
- [x] 1.2 新增 `isms/description.rs`：字元替換函式（NFKC 後把 `SEPARATORS`、`:`、`NOTE_BRACKETS` 換成空白並合併空白）、廠牌型號行 `廠牌型號：【…】`、元件行「產品（版本）」與版本待補只寫產品（design D5）。完成條件：單元測試重現 spec 的 Description with model and components、Pending and unresolved items、Separator in a version；另斷言含 `,`、`:`、`】`、`(`、換行的廠牌型號經 `split_lines` 仍為一行、`clean_line` 為空字串
- [x] 1.3 讀回檢查 `compose_description`：以 `describe::describe` 讀回並比較目錄產品 key、未辨識原文（`normalize`）與合併；顯示名稱讀回不一致時改寫 key 再檢查；仍不一致或沒有元件行時產生警告（design D6）。完成條件：單元測試斷言以測試目錄組出的「廠牌型號：【Aruba XYZ-100】」＋「AOS-CX（1.2.3）」讀回只有一個 `hpe-aoscx`、沒有合併；重現 spec 的 Line that cannot be read back（`Zyxel ABCD x2`）與 Asset with only a model 的警告文字
- [x] 1.4 測試：`cargo test -p cvewatch-core isms::description` 通過（含 1.2～1.3），`describe.rs` 的既有測試不變

## 2. core：列資料與 xlsx 版面

- [x] 2.1 新增 `isms/export.rs`：`IsmsExportRow`、`IsmsExport`、`isms_export_rows`（只讀 active 資產、略過沒有 ISMS 編號者、依 ISMS 編號不分大小寫排序、D4 的欄位對應與子類別未定義的警告）（design D4、D7）。完成條件：單元測試重現 spec 的 Mapping an exported row、Compliance raises the exported protection level、Rows sorted by ISMS ID、Asset without an ISMS ID、Retired asset is not exported
- [x] 2.2 新增 `isms/export_write.rs`：工作表「資訊資產清冊」、第 1–3 列文件資訊、第 4 列 `ISMS_V21_HEADERS`、第 5 列起資料、數字與文字儲存格、描述換行、凍結窗格、不合併儲存格；`template.rs` 的 `already_exists` 改為 `pub(crate)`（design D2、D3）。完成條件：單元測試把產生的位元組寫到 tempdir 後以 calamine 讀回，斷言 spec 的 Header row of the exported file（第 4 列 16 欄、前兩欄「資產編號」、文件資訊列不含三個表頭標記）且沒有合併儲存格
- [x] 2.3 `export_isms(conn, classes, catalog, path, exported_on)`：組列、寫檔（`files::write_new`，目的檔已存在時拒絕）、回傳 `IsmsExportSummary`、`tracing` 只記錄路徑與數量（design D7、D8、D10）；在 `isms/mod.rs` 重新匯出並更新模組表。完成條件：單元測試重現 spec 的 ISMS export target exists（原檔內容不變）與 0 個資產時仍產生只有表頭的檔案
- [x] 2.4 測試：`cargo test -p cvewatch-core isms` 通過（含 2.1～2.3）

## 3. core：round-trip 整合測試

- [x] 3.1 `crates/core/tests/isms_export.rs`：以一般清冊匯入在記憶體資料庫建立虛構資產（`FW-EDGE`：`NET-HW-001`、廠牌 `Aruba`、型號 `XYZ-100`、AOS-CX `1.2.3`；`SRV-01`：沒有 ISMS 編號；另一個資產含版本待補的 DiskStation Manager 與未辨識產品 `Zyxel ABCD` `2.0`；一個 retired 資產；一個有主機名稱 `srv01` 與 IP `192.0.2.10` 的資產），匯出後以 `read_isms_workbook` ＋ `build_conversion(ConversionContext::empty)` 讀回。完成條件：重現 spec 的 Exported file is read as an ISMS workbook 與 Importing into an empty database（每個資產的目錄產品 key 與未辨識原文相同、全部版本待補、報表沒有合併與來自廠牌型號行的未辨識描述行）
- [x] 3.2 同一資料庫讀回：完成條件：`import_isms(..., dry_run = true)` 重現 spec 的 Re-importing into the same database（`plan.added`、`plan.updated` 皆為 0，`report.pending_components` 為 0，`report.not_in_workbook` 只有 `SRV-01`），且資料庫不變
- [x] 3.3 敏感欄位：完成條件：測試以 calamine 讀出匯出檔全部儲存格，重現 spec 的 Host name and IP are not exported（不含 `srv01`、`192.0.2.10`）
- [x] 3.4 測試：`cargo test -p cvewatch-core --test isms_export` 通過

## 4. CLI：`cvewatch asset export-isms`

- [x] 4.1 `cli.rs` 新增 `AssetAction::ExportIsms { path }` 與中文說明（「匯出有 ISMS 編號的使用中資產為 ISMS 資訊資產清冊格式（可再以 import-isms 讀回）」）；`commands/asset.rs` 分派到 `commands/asset_isms.rs` 的 `export_isms`；更新兩個檔案開頭的子命令清單註解（design D1）。完成條件：`cvewatch asset export-isms --help` 顯示說明與路徑參數
- [x] 4.2 輸出由純函式 `export_lines(&IsmsExportSummary, path)` 產生：匯出數、略過清單（全部列出）、警告、`UNENCRYPTED_NOTICE` 與讀回提示（design D1）。完成條件：單元測試重現 spec 的 Exporting ISMS assets 與 Asset without an ISMS ID 的輸出；沒有略過與警告時不印對應行
- [x] 4.3 CLI 測試：`crates/cvewatch/tests/asset_isms.rs` 以 `assert_cmd` 驗證 `--help`、目的檔已存在時在開啟資料庫前以錯誤結束且原檔不變、尚未 init 時提示 `cvewatch init`；`tests/cli.rs` 的 asset 子命令清單加入 `export-isms`（加密資料庫上的實際匯出由 3.x 的 core 測試涵蓋，與 `asset_export.rs` 相同作法）。完成條件：`cargo test -p cvewatch` 通過

## 5. frontend 與 GUI

- [x] 5.1 frontend `workspace/assets.rs`：`isms_export_default()`（`dated_xlsx_default("cvewatch-isms")`）與 `export_isms(&self, path) -> Report`（design D7、D8）。完成條件：`crates/frontend/tests` 斷言預設檔名為 `cvewatch-isms-<當天日期>.xlsx`、沒有略過時為 info 且含「此檔未加密」、有略過時為 warning 並列出 `SRV-01`
- [x] 5.2 GUI：`asset_list.rs` 在「從 ISMS 清冊匯入」之後加 `ISMS_EXPORT_BUTTON`「匯出 ISMS 清冊」（不受 companion 影響）與滑鼠提示，更新模組說明註解；`screens/assets.rs` 新增 `export_isms()`（`save_xlsx("匯出 ISMS 清冊", …)`，取消時「已取消匯出 ISMS 清冊」）（design D9）。完成條件：`cargo build -p cvewatch-gui` 通過，按鈕位於「從 ISMS 清冊匯入」右側
- [x] 5.3 GUI 測試：`crates/gui/tests/assets_isms.rs`（或新增 `assets_isms_export.rs`）以 `egui_kittest::Harness`、替身檔案對話框與 `cvewatch_frontend::test_support` 重現 spec 的 Exporting the ISMS workbook from the window、Skipped assets in the window、Cancelling the ISMS export、ISMS export without the companion。完成條件：`cargo test -p cvewatch-frontend -p cvewatch-gui` 通過

## 6. 文件與整合驗證

- [x] 6.1 `docs/user-guide/assets.md` 在「從 ISMS 清冊匯入」之後新增「匯出 ISMS 清冊」一節：只匯出有 ISMS 編號的使用中資產、略過與警告的意義、資產描述的寫法（廠牌型號行與括號內的版本、為何如此寫才能讀回）、不含主機名稱與 IP、此檔未加密、不是機構範本；`gui.md` 加入按鈕說明。完成條件：文件中的指令與 `cvewatch asset export-isms --help` 一致
- [x] 6.2 以使用者自己的資料目錄（不入版控）執行 `cvewatch asset export-isms`，再以 `cvewatch asset import-isms <匯出檔> --dry-run` 讀回。完成條件：讀回為「新增 0、更新 0」、這次新加入的版本待補元件 0，結果（只有數量）記入 design.md 的實作紀錄；終端輸出的人名與匯出檔不寫入 repo
- [x] 6.3 全專案檢查：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`openspec validate 23-isms-export --strict`、`scripts/check-sensitive.ps1`。完成條件：五個指令皆成功、沒有敏感字詞命中

## Workflow follow-up

- 依 `/opsx-auto` 流程：code review 一輪 → 判斷架構變更（沒有新 crate 或外部整合；core `assets/isms/` 新增匯出模組、CLI 新增子命令、GUI 新增按鈕）→ commit → 需要時更新 `docs/architecture/` → archive → commit
- archive 時同步主 spec：`isms-inventory`（新增 ISMS export command、ISMS export layout、ISMS export field mapping、ISMS export description、ISMS export round trip）、`gui-console`（新增 GUI ISMS export）
- archive 時把 `openspec/specs/isms-inventory/spec.md` 的 Purpose 改為涵蓋讀取、匯入與匯出 ISMS 資訊資產清冊
- archive 後執行 `openspec validate --all --strict` 確認通過
