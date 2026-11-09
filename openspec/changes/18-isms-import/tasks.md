# Tasks

前提：17-isms-draft-import 已 archive。測試不呼叫真實外部 API；fixture 只用虛構的系統代碼、人名與型號，commit 前執行 `scripts/check-sensitive.ps1`。每完成一個任務群組就執行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test`，並勾選本檔。

## 1. core：版本待補與一般清冊匯入

- [x] 1.1 新增 `is_pending_version`，並在 `Component`、`UnresolvedEntry` 提供 `is_pending()`（design D1）。完成條件：單元測試涵蓋 `""`、`"  "` 為待補，`"0"`、`"7.2"` 不是
- [x] 1.2 `validate.rs`：`assets` 列與 `components` 列的 `version` 改為選填，`product` 仍必填（design D3）。完成條件：單元測試重現 spec 的 Missing required field（缺 product 中止），空白 version 的列不產生錯誤
- [x] 1.3 `plan.rs`：待補版本不解析、不產生版本格式警告；`VersionChange` 的空白端以「待補」呈現；新增 `ImportPlan::pending_components()`（design D3）。完成條件：單元測試重現 spec 的 New component without version 與 Known version cleared in the spreadsheet 的預覽文字 `11.1.6 → 待補`
- [x] 1.4 `import.rs`／`repo.rs`：舊版本為空白時不寫 `component_history`；待補的未辨識產品照常寫入（design D3）。完成條件：整合測試斷言 `11.1.6 → 待補` 留下一筆 `11.1.6`，`待補 → 7.2.2-72806` 沒有新增歷史紀錄（spec 的 Pending version filled）
- [x] 1.5 測試：`crates/core/tests` 重現 spec 的 Pending version is exported blank（匯出 → `--dry-run` 重匯為「新增 0、更新 0」）與 Asset with only pending components，並確認 `cargo test -p cvewatch-core assets` 通過

## 2. core：比對排除與待補計數

- [x] 2.1 `matching/load.rs`：元件查詢排除待補的目錄產品元件（design D2）。完成條件：整合測試重現 spec 的 Pending component is skipped（本機有 DSM CVE，待補元件沒有命中）
- [x] 2.2 新增 `assets::pending::pending_counts` 與 `PendingCounts::summary()`（design D7）；`Dashboard` 新增 `pending_versions`，來源矛盾數排除待補元件。完成條件：單元測試斷言只計 active 資產的目錄產品元件、`summary()` 為「版本待補 3 個元件（2 個資產），未納入比對」；Dashboard 測試中待補元件的舊 `match_conflict` 不計入
- [x] 2.3 測試：整合測試重現 spec 的 Known version becomes pending（open 命中保留不更新）與填上版本後下一次比對沿用原命中，並確認 `cargo test -p cvewatch-core matching` 通過

## 3. core：編修待補元件

- [x] 3.1 `edit.rs`：原版本空白、送出空白 → 不變；原版本非空、送出空白 → 拒絕；新增元件版本空白 → 拒絕；填上待補版本後觸發比對（design D8）。完成條件：單元測試重現 spec 的 Filling a pending version 與 Clearing a known version
- [x] 3.2 未辨識產品改為目錄產品時沿用空白版本。完成條件：單元測試重現 spec 的 Mapping a pending unresolved product
- [x] 3.3 測試：`cargo test -p cvewatch-core` 全部通過，既有編修測試不變

## 4. core：ISMS 匯入

- [x] 4.1 改名：`draft.rs` → `conversion.rs`、`Draft` → `Conversion`、`DraftContext` → `ConversionContext`、`DraftReport` → `IsmsReport`；移除 `output.rs`、`generate_draft`、`write_draft`、`isms_report` 工作表相關常數與 `--prune` 提醒（design D4、D6、D9）。完成條件：`cargo build --workspace` 通過（呼叫端暫時改用新名稱），`rg -i "isms_report|isms-draft|write_draft" crates` 沒有結果
- [x] 4.2 轉換調整：新資產描述沒有項目時改為 `SkipReason`；新資產曝露度用預設值；`IsmsReport` 新增待補元件數（design D4、D6）。完成條件：單元測試重現 spec 的 Blank description for a new asset 與 Mapping a storage row（曝露度 internal）
- [x] 4.3 新增 `to_inventory(&Conversion) -> Inventory` 與 `import_isms(conn, ctx, path, dry_run, now)`，以 `ImportOptions { prune: false, dry_run }` 交給 `import_inventory`（design D4）。完成條件：整合測試 `tests/isms_import.rs` 對 fixture 重現 spec 的 Importing an ISMS workbook、Previewing an ISMS import、Re-importing the same ISMS workbook
- [x] 4.4 `AssetUpdate.changed_fields` 改為 `Vec<FieldChange>`（欄位、舊值、新值，以畫面用語呈現）；一般匯入的預覽仍只列欄位名稱（design D5）。完成條件：單元測試重現 spec 的 ISMS fields changed for an existing asset；既有的 `asset import` 預覽測試輸出不變
- [x] 4.5 測試：`tests/isms_import.rs` 另重現 spec 的 ISMS ID already in cvewatch、New product on an existing asset、ISMS ID of a retired asset、System name used by another code（整批中止、資料庫不變）、Assets missing from the ISMS workbook；原 `tests/isms_draft.rs` 刪除；`cargo test -p cvewatch-core isms` 通過

## 5. CLI、摘要 email 與使用說明

- [x] 5.1 `cli.rs`：移除 `asset isms-draft`，新增 `asset import-isms <workbook> [--dry-run]`；處理放在 `commands/asset_isms.rs`，依 design D6 先印 ISMS 報表，再印匯入預覽與摘要。完成條件：`crates/cvewatch/tests` 對 fixture 斷言 `--dry-run` 不改資料庫、實際匯入結束碼 0 且摘要含新增數與待補數、未初始化時重現 spec 的 Database not initialized
- [x] 5.2 `asset import` 的預覽與完成摘要列出待補元件數（spec 的 Import dry-run）。完成條件：CLI 測試斷言含空白 version 的清冊匯入成功並印出「版本待補 1 個元件」
- [x] 5.3 `cvewatch match` 與 `cvewatch sync`（含略過比對時）結束時印出 `PendingCounts::summary()`，沒有待補時不印（design D7）。完成條件：CLI 測試重現 spec 的 Summary after sync，並斷言沒有待補時輸出不含「版本待補」
- [x] 5.4 `notify`：`DigestData` 加入待補計數，摘要內容另列一行，不影響「無新事項」判斷。完成條件：單元測試重現 spec 的 Pending versions in the digest
- [x] 5.5 `docs/user-guide/assets.md`：「由 ISMS 清冊建立資產」改寫為 `asset import-isms`；新增「版本待補」一節（不比對、在哪裡看到提醒、如何補上、空白版本重匯會改為待補、不支援降版）。`matching.md` 註明待補元件不參與比對。完成條件：文件中的指令與 `cvewatch asset import-isms --help` 一致，文件中不再出現 `isms-draft`
- [x] 5.6 測試：`cargo test -p cvewatch` 全部通過

## 6. frontend 與 GUI

- [x] 6.1 frontend：`PENDING_VERSION` 常數、元件版本顯示、資產篩選 `pending_only`、Dashboard 的待補計數（design D8）。完成條件：frontend 單元測試斷言待補元件顯示「版本待補」、篩選只留有待補元件的資產
- [x] 6.2 `asset_form`：未修改的待補元件可以儲存；把已知版本清空、新加入元件未填版本時顯示「必填」（design D8）。完成條件：frontend 測試重現 spec 的 Saving other fields with a pending component 與 Clearing a known version 的畫面錯誤
- [x] 6.3 GUI 資產畫面：清單的待補標示、詳情「版本待補（未納入比對）」、「只顯示版本待補」篩選；Dashboard 待補數字可點選並切換到已篩選的資產畫面。完成條件：`crates/gui/tests` 重現 spec 的 Pending component in the detail pane 與 Jumping to pending versions
- [x] 6.4 GUI ISMS 匯入：`companion.rs` 新增 `ImportKind::Isms`（`asset import-isms <file> [--dry-run]`）；`dialogs/import.rs` 在 `Isms` 時不顯示元件檔與退役勾選、改用 ISMS 覆寫提醒；工具列以「從 ISMS 清冊匯入」取代「由 ISMS 清冊產生草稿」，移除草稿另存對話框與 `workspace/isms.rs` 的草稿函式（design D8、D9）。完成條件：`companion` 單元測試斷言參數；`crates/gui/tests` 以替身 companion 重現 spec 的 Importing from the window、Cancelling the ISMS preview、Not an ISMS workbook in the window，以及找不到 `cvewatch.exe` 時按鈕停用
- [x] 6.5 `docs/user-guide/gui.md`：按鈕說明改為「從 ISMS 清冊匯入」，並說明版本待補的標示與 Dashboard 數字。完成條件：文件中不再出現「產生草稿」
- [x] 6.6 測試：`cargo test -p cvewatch-frontend -p cvewatch-gui` 全部通過

## 7. TUI

- [x] 7.1 Dashboard 加入待補計數行；資產畫面標示有待補元件的資產，元件版本顯示「版本待補」（design D8）。完成條件：TUI 畫面測試（`TestBackend`）重現 spec 的 Pending versions on the dashboard 與 Pending component in the assets screen，寬字元對齊不跑版
- [x] 7.2 測試：`cargo test -p cvewatch-tui` 全部通過

## 8. 整合驗證

- [x] 8.1 以使用者自己的 ISMS 清冊（repo 根目錄、不入版控）對測試用資料目錄執行 `cvewatch asset import-isms --dry-run`，再實際匯入；確認新增數、略過數與 17 的實測結果（轉換 15、略過 30）一致，GUI Dashboard 與資產畫面顯示待補數。完成條件：結果記入 design.md 的實作紀錄，終端輸出的人名不寫入任何檔案
- [x] 8.2 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 全部通過，`scripts/check-sensitive.ps1` 沒有命中

## Workflow follow-up

- 依 `/opsx-auto` 流程：code review 一輪 → 判斷架構變更（沒有新 crate 或外部整合；ISMS 子模組改名、GUI companion 新增匯入種類）→ commit → 需要時更新 `docs/architecture/` → archive（`openspec validate --all --strict`）→ commit
- archive 時把 `openspec/specs/isms-inventory/spec.md` 的 Purpose 改為「讀取機構 ISMS 資訊資產清冊並匯入 cvewatch」，不再提到清冊格式轉換
- archive 後依序 propose 19-asset-identity-network
