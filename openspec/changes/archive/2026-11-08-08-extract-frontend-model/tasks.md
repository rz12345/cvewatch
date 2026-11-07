# Tasks

驗收基準（每個任務群組都適用）：`crates/tui/tests` 的測試函式與 `crates/tui/tests/snapshots/*.snap` 不修改即通過，`git diff --exit-code crates/tui/tests/snapshots` 無差異；`tests/support` 只允許改 import 路徑。

## 1. crate 骨架與執行環境啟動

- [x] 1.1 新增 `crates/frontend`（套件名 `cvewatch-frontend`，相依 `cvewatch-core`），加入 workspace members；`cargo build -p cvewatch-frontend` 成功
- [x] 1.2 `cvewatch_core::runtime::start(location, command) -> Runtime`：自 `crates/cvewatch/src/app.rs` 搬入設定載入、資料目錄、日誌與清理、`KeyringStore`；未知欄位與日誌失敗警告放進 `Runtime.warnings`（文字不變）；CLI 的 `app.rs::start` 改呼叫它並把 warnings 逐行印到 stderr。完成條件：`crates/cvewatch/tests/cli.rs` 的未知欄位警告測試不修改即通過
- [x] 1.3 測試：`core::runtime` 單元測試涵蓋「未知欄位 → warnings 含 ``設定檔包含未知欄位 `x`（已忽略）``」與「日誌目錄不可寫 → warnings 含無法寫入日誌檔且 start 仍成功」；`cargo test -p cvewatch-core runtime` 通過

## 2. 資料、篩選、連結與外部效果

- [x] 2.1 將 `data.rs`、`filter.rs`、`links.rs`、`services.rs` 及其 `#[cfg(test)]` 單元測試搬到 frontend；tui 以 `pub use cvewatch_frontend::{data, filter, links, services}` 保留原路徑。完成條件：`cargo test -p cvewatch-frontend` 中搬來的單元測試通過、`cargo test -p cvewatch-tui` 全過
- [x] 2.2 `ProcessLauncher` 改由呼叫端指定要執行的 CLI 執行檔（TUI 仍傳 `current_exe()`），並以 `core::process::hide_window` 啟動；完成條件：frontend 單元測試斷言組出的命令為 `<exe> [--config <path>] sync`
- [x] 2.3 測試替身（Opener、Clipboard、SyncLauncher/RunningSync、NotifyBackend、Spawner、固定時鐘）移到 `cvewatch_frontend::test_support`（`test-support` feature，僅 dev-dependencies 啟用）；tui 的 `tests/support/mod.rs` 改為 re-export。完成條件：`cargo test -p cvewatch-tui` 全過且快照無差異

## 3. 命中清單模型與 Workspace

- [x] 3.1 `FindingsModel`（filter、search、sort、selected、visible；`refresh_visible`、`visible_selection`、`prune_selection`）搬到 frontend；TUI `FindingsState` 只保留 cursor 與 search_input。完成條件：frontend 單元測試涵蓋「被搜尋隱藏的所選命中不在 `visible_selection`，放寬搜尋後仍保持選取」與「重新載入後移除已不存在的選取」
- [x] 3.2 `Workspace`（conn、settings、config、config_path、services、data、findings、new_data、sync）與 `Report { message, level }`；TUI `App` 改為持有 `ws: Workspace`。完成條件：`cargo test -p cvewatch-tui` 全過
- [x] 3.3 將 `actions.rs` 的狀態變更（含 Redmine 狀態註記）、等待區處理、VEX 匯出改為 `Workspace` 方法並回傳 `Report`，訊息字串原樣搬移；依操作拆成 `workspace/{triage,pending,vex}.rs`。完成條件：`findings`、`assets_pending`、`vex_export` 測試與快照不修改即通過
- [x] 3.4 重新載入、新資料輪詢（`NEW_DATA_POLL`）、背景 sync 啟動與結束輪詢、開啟網址、複製改為 `Workspace` 方法；`reload` 接受開啟中的命中 id 清單並回傳各自的新詳情或「已不存在」（design D3）。完成條件：`refresh`、`dashboard` 測試與快照不修改即通過
- [x] 3.5 測試：frontend 新增不經 TUI 的 `Workspace` 測試（`crates/frontend/tests/workspace.rs`，以 `#[path]` 引用 `crates/core/tests/common` 的記憶體資料庫）：批次忽略寫入每筆一個事件、原狀態相同時不寫事件且 Report 說明未變更、等待區套用產生 probable 命中、sync 執行中再次啟動回報「已有 sync 正在執行」、不安全網址被拒絕；`cargo test -p cvewatch-frontend` 通過

## 4. 設定表單與前端 Context

- [x] 4.1 `settings/form.rs`、`save.rs`、`jobs.rs` 及其單元測試搬到 `cvewatch_frontend::settings`；`SettingsPage`、`keys.rs` 留在 tui，tui 的 `settings` 模組 re-export `NotifyBackend`、`RedmineLists`、`Spawner` 等原路徑。完成條件：`settings` 測試與快照不修改即通過
- [x] 4.2 `cvewatch_frontend::context::build(&Runtime, ContextOptions) -> Context`（產品目錄、`open_working_db`、`Services`），`ContextOptions` 含 CLI 執行檔路徑與明確指定的設定檔路徑；`crates/cvewatch/src/commands/tui.rs` 改用它並持有 `Runtime` 到 TUI 結束。完成條件：`cargo build --release` 後手動執行 `cvewatch tui` 可開啟 Dashboard，按 `S` 背景 sync 正常
- [x] 4.3 測試：frontend 單元測試驗證 `ContextOptions` 的 CLI 執行檔與設定檔路徑傳到 sync launcher 與排程 backend；`cargo test -p cvewatch-frontend` 通過

## 5. 整合檢查

- [x] 5.1 確認 `crates/tui` 不再含 `data.rs`、`filter.rs`、`links.rs`、`services.rs`、`actions.rs`、`settings/{form,save,jobs}.rs`，且 `cargo tree -p cvewatch-frontend` 不含 ratatui、crossterm、tui-input
- [x] 5.2 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 全過，且 `git diff --exit-code crates/tui/tests/snapshots` 無差異
- [x] 5.3 README「建置」段落補充 workspace 新增的 `crates/frontend` 與其用途（一句話），並確認 README 中的指令仍可照抄執行

## Workflow follow-up

- `/code-review` 修正（最多三輪，某輪無發現即提前結束）→ commit
- `/opsx:archive 08-extract-frontend-model`（本 change `skip_specs: true`，無主 spec 需同步），`openspec validate --all --strict` 通過 → commit
