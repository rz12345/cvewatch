# Tasks

前提：16-isms-admin-fields 已 archive，主 spec 已同步（資產說明、`systems` 工作表、管理欄位、`RecordContext`）。測試不呼叫真實外部 API；fixture 只用虛構的系統代碼、人名與型號，commit 前執行 `scripts/check-sensitive.ps1`。每完成一個任務群組就執行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。

## 1. core：讀取 ISMS 清冊

- [x] 1.1 `assets/isms/workbook.rs`：欄名正規化、工作表選擇、在前 10 列找表頭、兩欄資產編號、必要欄位檢查、`#N/A` 視為空白、只保留第二欄資產編號非空的列（design D2）。完成條件：單元測試涵蓋「機密性\n(A)」→「機密性」、表頭在第 4 列、缺少「可用性」的錯誤訊息、「選項(勿刪)」不被選為資料工作表
- [x] 1.2 fixture：`gen_inventory_fixtures.rs` 產出 `tests/fixtures/isms_workbook.xlsx`（design D8 列出的版面與虛構內容）與一個非 ISMS 的 xlsx。完成條件：重新產生後 `git diff --stat tests/fixtures` 只列出新檔，`scripts/check-sensitive.ps1` 沒有命中
- [x] 1.3 測試：以 fixture 斷言 spec 的 Header below document information rows、Footnote rows、Not an ISMS workbook、Missing required column，並確認 `cargo test -p cvewatch-core isms` 通過

## 2. core：列轉換與描述比對

- [x] 2.1 `assets/isms/row.rs`：ISMS 編號組成、PE/IF/SE 過濾、組成 `RecordInput` 並以 `validate_record` 驗證、`－` 轉成未評定、個資 Y/N、錯誤轉為 `SkipReason`（design D3）。完成條件：單元測試重現 spec 的 Two ID columns、Personnel and information rows、Major class mismatch、Compliance dash，以及 ISMS 編號重複、SW01 未填 C/I/A 兩種略過原因
- [x] 2.2 `assets/isms/describe.rs`：拆行、去除標籤、註記與數量，以目錄比對，中文前綴重試，同產品合併，未辨識行附建議（design D4）。完成條件：單元測試重現 spec 的 Multi-line description、Leading Chinese words、Same switch used twice（使用測試專用目錄別名）
- [x] 2.3 測試：`cargo test -p cvewatch-core isms` 通過

## 3. core：草稿組裝、報表與寫檔

- [x] 3.1 `assets/isms/draft.rs`：`DraftContext` 載入（子類別、目錄、資產說明、ISMS 編號對照既有資產）、欄位對應、`systems` 名稱決定、沿用既有 active 資產並保留元件、retired 資產略過（design D1、D5）。完成條件：單元測試重現 spec 的 Mapping a storage row、Two names for one system code、ISMS ID already in cvewatch、ISMS ID of a retired asset，並斷言既有資產的未辨識產品不會重複
- [x] 3.2 `assets/isms/report.rs`：`DraftReport`、CLI 摘要文字（每類最多 20 筆）、`isms_report` 工作表列（design D6）。完成條件：單元測試重現 spec 的 Assets missing from the ISMS workbook（含 `--prune` 提醒）
- [x] 3.3 寫出草稿：重用 `export.rs` 的列組裝與工作表寫出（必要時改為接收資料結構），寫出 `assets`、`components`、`systems`、`isms_report`；目的檔已存在時先拒絕。完成條件：測試重現 spec 的 Generating a draft（四個工作表、資料庫不變）與 Draft target exists；`cargo test -p cvewatch-core` 通過，既有匯出測試不變
- [x] 3.4 往返整合測試：對 fixture 產生草稿 → 未補版本就匯入，中止並列出缺少 version 的列 → 補上版本後匯入成功，資產帶有 ISMS 欄位 → 資料庫已有同 ISMS 編號的資產時，預覽為「更新 1」。完成條件：重現 spec 的 Importing a draft before filling versions 與 Importing a completed draft

## 4. CLI 與文件

- [x] 4.1 `cli.rs` 新增 `asset isms-draft <workbook> <output>`，處理放在 `commands/asset_isms.rs`，結束碼依 design D7。完成條件：`crates/cvewatch/tests` 新增案例，對 fixture 執行後斷言結束碼 0、摘要含轉換數與略過數；目的檔存在時結束碼非 0 且原檔不變
- [x] 4.2 `docs/` 使用說明新增「由 ISMS 清冊建立資產」：產生草稿、補版本、在 `catalog.d` 為比對不到的描述加別名、匯入時不要用 `--prune`。完成條件：文件中的指令與 `cvewatch asset isms-draft --help` 的參數一致
- [x] 4.3 測試：`cargo test` 全部通過

## 5. GUI

- [x] 5.1 `gui/src/files.rs` 的 `Files` trait 新增 `open_isms_workbook()`；`frontend::workspace` 新增 `isms_draft_default()` 與 `write_isms_draft()`，回傳含摘要的 `Report`（design D7）。完成條件：frontend 單元測試斷言預設檔名含當天日期、摘要文字含轉換數、沿用數、略過數、待補版本數與 `--prune` 提醒
- [x] 5.2 資產工具列的清冊檔案組加入「由 ISMS 清冊產生草稿」。完成條件：`crates/gui/tests` 以替身 `Files` 重現 spec 的 Generating a draft from the window、Not an ISMS workbook in the window、Cancelling the save dialog
- [x] 5.3 測試：`cargo test` 全部通過，並手動以使用者自己的 ISMS 清冊（不入版控）在 GUI 產生一次草稿，確認摘要合理

## Workflow follow-up

- 依 `/opsx-auto` 流程：code review 一輪 → 判斷架構變更（新增 core 子模組，沒有新的外部整合）→ commit → archive（`openspec validate --all --strict`）→ commit
- archive 後依序 propose 18-asset-identity-network
