# Design

## Context

動機見 proposal.md - Why。目前 `crates/tui` 的模組依是否相依 ratatui / crossterm / tui-input 可分為兩群：

| 不相依終端機 | 相依終端機 |
|---|---|
| `data.rs`、`filter.rs`、`links.rs`、`services.rs`、`settings/form.rs`、`settings/save.rs`、`settings/jobs.rs`、`actions.rs`（但是 `impl App`） | `app.rs`（`KeyEvent`、`tui_input::Input`）、`dialog.rs`、`settings/mod.rs`、`settings/keys.rs`、`ui/*`、`terminal.rs`、`glyphs.rs`、`width.rs`（純函式，但只對等寬終端機有意義） |

`actions.rs` 雖然不 import ratatui，卻直接改寫 `App` 的 `message`、`dialog`、`stack`、游標，所以是本 change 主要的拆解對象。

啟動流程分散在兩處：`crates/cvewatch/src/app.rs::start`（設定 → 資料目錄 → 日誌 → 未知欄位警告，警告直接 `eprintln!`）與 `commands/tui.rs`（開資料庫、組 `Services` 與 `Context`）。GUI 是另一個執行檔，無法呼叫 CLI crate 的私有函式，也沒有 stderr 可以印警告。

## Goals / Non-Goals

**Goals:**

- GUI 與 TUI 對同一個操作呼叫同一段程式，寫入的事件、Redmine 註記、訊息文字一致
- 共用層不出現任何 UI 框架型別（按鍵、輸入框、繪製），可在無畫面的單元測試中驗證
- TUI 對外完全不變，以既有測試與快照作為驗收

**Non-Goals:**

- 不重新設計 TUI 的狀態機或對話框流程
- 不把共用層做成通用的 MVVM 框架；只抽出兩個前端確實都需要的部分

## Decisions

### D1. 新 crate `cvewatch-frontend`，而不是放進 core

core 是資料、比對與通知的領域層，CLI 的大部分子命令只需要它。「畫面快照」「多選只套用可見列」「操作結果的提示文字」是前端關注點，放進 core 會讓 CLI 背上不需要的概念。另開 crate 也讓 GUI 的相依方向單純：`gui → frontend → core`、`tui → frontend → core`。

替代方案：放在 `crates/tui` 並讓 GUI 相依 tui —— GUI 會被迫連結 ratatui 與 crossterm，否決。

### D2. 以 `Workspace` 承載共用狀態，操作回傳 `Report`

`Workspace` 擁有 `Connection`、寫入用 `Settings`、`Config`、設定檔路徑、`Services`、`Snapshot`、命中清單模型（篩選、搜尋字串、排序、多選、可見列）、新資料旗標與執行中的背景 sync。

畫面專屬狀態留在各前端：TUI 的游標、捲動位移、`View` 堆疊、`Dialog`、搜尋輸入框；GUI 之後的分頁、選取列、視窗。

有副作用的操作改為 `Workspace` 的方法，回傳 `Report`（提示文字與等級：資訊／警告／錯誤），不直接寫入任何畫面欄位：

- 狀態變更（給定命中 id 清單、狀態、原因、理由、備註）、等待區套用／不相關
- 重新載入（含「命中已不存在」附註）、新資料偵測（5 秒輪詢 `data_marker`）
- 啟動背景 sync、輪詢 sync 結束
- 開啟網址（沿用 `is_safe_url` 檢查）、複製文字、匯出 VEX
- 讀取命中詳情、CVE 連結清單

TUI 的 `App` 改為持有 `ws: Workspace`，原本的 `self.message = ...` 改為把 `Report.message` 填入 `message`。訊息字串整段搬移、不改寫，以確保快照不變。

替代方案：用 trait 讓前端實作 `fn show_message(&mut self, ..)` 的回呼 —— 回呼讓共用層需要知道前端型別，也較難在單元測試中斷言，否決。

### D3. 詳情畫面的重新整理以「待重新整理的 id」交換

TUI 在重新載入後會更新 `View` 堆疊中開啟的命中詳情，命中消失時關閉並附註。`View` 是 TUI 專屬型別，因此 `Workspace::reload` 接受前端提供的開啟中命中 id 清單，回傳每個 id 的新 `DetailData` 或「已不存在」，前端據此更新自己的堆疊；附註文字由共用層產生。

### D4. 設定表單只搬模型、驗證與儲存

`settings/form.rs`（`Form`、`FieldId`、`Tab`、驗證規則、`apply`）、`save.rs`（以 `toml_edit` 寫回並保留註解、秘密寫入憑證庫）、`jobs.rs`（`NotifyBackend`、`Spawner`、`Running`、`JobKind`、`LiveBackend`）搬到 `cvewatch_frontend::settings`。`Form` 已是不可變更新（`with_text`、`toggled` 回傳新值），GUI 可直接使用。

`SettingsPage`（`Editing` 含 `tui_input::Input`、`Picker` 游標、`Confirm` 選項）與 `keys.rs` 是 TUI 的互動方式，留在 tui。

### D5. 啟動拆成兩層：`core::runtime` 與 `frontend::context`

- `cvewatch_core::runtime::start(location, command) -> Runtime`：載入設定、解析 `DataPaths`、啟動日誌與清理、建立 `KeyringStore`；未知欄位與日誌失敗不再 `eprintln!`，改放進 `Runtime.warnings: Vec<String>`（訊息文字不變）。CLI 的 `app.rs::start` 改為呼叫它並逐行印到 stderr，CLI 輸出不變。
- `cvewatch_frontend::context::build(runtime, ContextOptions) -> Context`：載入產品目錄、`open_working_db`、組 `Services`。`ContextOptions` 由呼叫端提供 sync／排程子程序要執行的 CLI 執行檔路徑與明確指定的設定檔路徑。TUI 傳入 `current_exe()`（不變）；09 的 GUI 會傳入同資料夾的 `cvewatch.exe`。

`Runtime` 持有 `LogGuard`，生命週期必須涵蓋整個前端事件迴圈；`Context` 不持有它，由呼叫端保留 `Runtime`。

### D6. 保留 TUI 的公開路徑

`crates/tui/tests` 以 `cvewatch_tui::data::Settings`、`cvewatch_tui::services::*`、`cvewatch_tui::settings::{NotifyBackend, RedmineLists, Spawner}` 等路徑存取型別。tui 以 `pub use cvewatch_frontend::{data, filter, links, services}` 與在 `settings` 模組內 re-export 保留這些路徑，測試案例與快照不需修改。

### D7. 測試替身移到 frontend 的 `test-support` feature

瀏覽器、剪貼簿、sync 子程序、通知後端、背景工作的替身目前寫在 `crates/tui/tests/support/mod.rs`。09 的 GUI 測試也需要同一組替身，因此搬到 `cvewatch_frontend::test_support`（`[features] test-support = ["dep:tempfile"]`：`Recorder` 的臨時目錄需要 tempfile；只在 dev-dependencies 啟用）。tui 的 `tests/support` 改為 re-export，記憶體資料庫建構器仍以 `#[path]` 引用 `crates/core/tests/common`。

### D8. 實作時的決定（apply 階段，依建議做法處理）

- **清單模型的畫面專屬狀態以泛型 `view` 承載**：既有 TUI 測試同時存取 `app.findings.visible`（模型）與 `app.findings.search_input`、`app.findings.cursor`（TUI 專屬）。為了讓清單模型只存在 `Workspace`、又不讓 frontend 出現 `tui_input` 型別，`FindingsModel<V = ()>` 帶一個前端提供的 `view: V`，並 `Deref` 到 `V`；`Workspace<V = ()>` 隨之泛型化。`V: ListView`，每次 `refresh_visible` 後模型呼叫 `visible_changed(len)`，TUI 以此修正游標（對應 Risks 中「依可見列數修正游標」）。GUI 不需要時用 `()`。
- **TUI `App` 以 `Deref<Target = Workspace<FindingsState>>` 保留 `app.conn`、`app.data`、`app.new_data`、`app.sync`、`app.services` 等原路徑**；`App` 內部的程式一律寫 `self.ws.…`，以免經由 `Deref` 借用整個 `App`。
- **驗收條件的唯一例外**：`crates/tui/tests/refresh.rs` 有一行同時以 `&mut app.conn` 與 `&app.settings.catalog` 呼叫 `run_match`，經 `Deref` 時兩者都借用整個 `App`，無法編譯。改為 `&mut app.ws.conn`、`&app.ws.settings.catalog`（只改欄位路徑，測試內容與斷言不變）。其餘 tui 測試函式與全部快照未修改。
- **`actions.rs` 的 TUI 端剩餘部分改名為 `dispatch.rs`**：執行對話框命令、把 `Report` 顯示到狀態列、依 `Refreshed` 更新 `View` 堆疊；多個連結時開選單、沒有連結的提示仍屬 TUI。
- **D3 的回傳型別**：`reload`、`set_status`、`resolve_pending` 回傳 `Refreshed { report, details: Vec<DetailRefresh> }`，`details` 與傳入的開啟中 id 同順序，值為 `Updated`／`Gone`／`Kept`（清單或詳情載入失敗時保留原詳情，與原本行為相同）。
- **其他前端共用的判斷一併搬到 `Workspace`**：「所選命中都被隱藏」的提示（`list_targets`）、等待區套用目標清單（`pending_targets`）、VEX 預設檔名與「沒有命中」提示（`vex_export_default`）、讀取詳情與連結的錯誤文字。文字原樣搬移。
- **`Report.level`**：成功為 Info；拒絕開啟不安全網址、所選命中被隱藏、找不到命中、寫入後重新載入失敗、sync 結束碼 3（排程 sync 進行中）與 4（部分失敗）為 Warning；其餘失敗（含 sync 其他結束碼或沒有結束碼）為 Error。TUI 目前只顯示文字。
- **任務 3.5 的「已有 sync 正在執行」**：本程序再次按 `S` 的既有訊息是「背景 sync 仍在執行」，「已有 sync 正在執行」是排程 sync 進行中（結束碼 3）的訊息。為維持訊息不變，frontend 測試兩者都驗證。
- **`context::build` 接受 `ContextOptions`（CLI 執行檔、明確設定檔）**；`ProcessLauncher` 與 `LiveBackend` 由同一份選項建立，新增 `ProcessLauncher::command()` 供測試斷言命令列。
- **`Runtime.warnings` 不含「警告：」前綴**，CLI 印出時加上，stderr 輸出與原本相同。
- **`save.rs` 的 insta 快照隨模組搬到 `crates/frontend/src/settings/snapshots/`**，檔名與 `source` 欄位改為 frontend，內容不變。
- **Code review 第 1 輪後的調整**：
  - `Report`、`Refreshed` 加上 `#[must_use]`：`App` 經 `Deref` 可呼叫到 `Workspace` 同名方法，忘了顯示結果時改為編譯警告（`-D warnings` 下為錯誤）。
  - 「明確指定的設定檔轉絕對路徑後傳給子程序」移到 `cvewatch_core::runtime::Runtime::explicit_config`，CLI 的 `schedule install`、`tui` 與 frontend 的 `ContextOptions::for_runtime` 共用，GUI 不必重寫；CLI 的 `schedule::absolute` 移除。
  - `cvewatch_tui::Context` 改為 `{ base: cvewatch_frontend::Context, glyphs }`，不再逐欄複製；`tests/support` 的建構器隨之調整（只動建構方式）。
  - 背景 sync 與重新安裝排程共用 `services::cli_command(exe, config, args)` 組命令列。
  - TUI 內部存取清單的畫面狀態一律寫 `findings.view.<欄位>`，`FindingsModel` 的 `Deref` 只留給既有測試的路徑；移除 `App` 上只轉呼叫 `Workspace` 的 `refresh_visible`、`visible_selection`、`visible_ids`（測試經 `Deref` 呼叫同名方法，行為相同）。
- **Code review 第 2 輪後的調整**：
  - **`Report` 文字不含 TUI 按鍵**：新增 `Report.hint: Option<Hint>`（`StaleAfterWrite`、`Reload`、`RevealSelection`），原本內嵌的「（畫面可能過時，按 R 重試）」「（請按 R 重新載入）」「（放寬搜尋／篩選，或按 Esc 清除多選）」改由 TUI 依 `Hint` 接在文字後面，狀態列顯示的完整文字與原本相同；GUI 之後以自己的按鈕名稱說明。
  - `sync_exit_message` 改為 `sync_exit_report`，結束碼的文字與等級在同一處決定。
  - sync 結束時的新資料檢查算作一次輪詢（`last_poll` 設為該次 `tick` 的時間），取代第 1 輪的旗標寫法；`Workspace::poll_sync` 不再公開，一律經 `tick`。
  - `FindingsModel` 的 `visible_selection`、`visible_ids`、`row_at` 以 `.get()` 取列：`data` 換過而未 `refresh_visible` 時略過越界索引，不 panic。
  - TUI `App` 上與 `Workspace` 同名的方法改名（`reload_screen`、`copy_text`、`start_background_sync`），避免外部呼叫經 `Deref` 靜默改呼叫到 `Workspace` 版本。
  - `FindingsModel` 的 `Deref` 保留（既有測試路徑需要，且 orphan rule 不允許在 tui 為 frontend 型別實作 `Deref`）；tui 加一個完整解構 `FindingsModel` 與 `FindingsState` 的單元測試，任一邊新增欄位時無法編譯，提醒檢查同名欄位。
  - `context::system_services` 重用 `Runtime.store`（`KeyringStore` 為 `Clone`），資料庫金鑰與通知秘密使用同一個憑證庫設定。
  - `rusqlite` 從 tui 的一般相依移到 dev-dependencies（tui 程式碼不再直接存取資料庫）。
  - TUI 與 frontend 測試共用的資料片段（`PANOS_CVES`、`PENDING_CVE`、`PROXMOX_CNA`、`panos_cves`、`keyword_only_pending`）移到共用測試模組（第 3 輪再移到 `crates/frontend/tests/shared/mod.rs`，見下）；tui 的 `seeded_world` 呼叫順序不變，快照不受影響。
- **Code review 第 3 輪後的調整**：
  - 新增 `services::CliTarget { exe, config }`：`ProcessLauncher(CliTarget)`、`LiveBackend(CliTarget)` 共用同一個型別與 `CliTarget::command(args)`，取代第 1 輪的 `cli_command` 函式與兩組重複欄位；`ContextOptions` 改為 `{ cli: CliTarget }`。任務 4.3 的測試改為斷言兩者實際組出的命令列。
  - 共用測試資料片段從 core 的測試共用模組移到 `crates/frontend/tests/shared/mod.rs`（tui 的 `tests/support` 以 `#[path]` 引入），core 的整合測試不再編譯只給下游用的程式。
  - `core::runtime` 單元測試以 TOML 基本字串（`toml::Value`）寫入暫存路徑，路徑含引號時也正確。
  - `vex_export_default` 的筆數與 `export_vex` 實際匯出的 `visible_ids()` 一致。
  - TUI `on_tick` 的重繪判斷改用單一 `background_state()`。
  - frontend 也加上完整解構 `FindingsModel` 的單元測試，模型新增欄位時在 frontend 測試就會發現（tui 端的同樣檢查保留）。
- **三輪後仍未處理**：TUI `App` 的 `Deref<Target = Workspace>` 讓外部程式碼能直接呼叫 `Workspace` 的寫入方法（如 `app.reload(&[])`），略過 TUI 的詳情與游標更新。移除 `Deref` 需要把既有測試的 `app.conn`、`app.data`、`app.findings` 等路徑改成 `app.ws.…`，與本 change「tui 測試不修改」的驗收條件衝突，因此保留；tui 內部一律寫 `self.ws.…`，同名方法已改名，`Report`／`Refreshed` 為 `#[must_use]`。`FindingsModel` 的 `Deref` 同理保留（有兩處完整解構檢查）。建議在 09 之後另開 change 一併改測試路徑並移除兩個 `Deref`。
- **任務 4.2 的手動驗收**（互動式 `cvewatch tui`、按 `S`）需要真實 Credential Manager 與外部 API，未由自動流程執行；以非互動煙霧測試確認啟動路徑（未知欄位警告印出、`context::build` 執行到資料庫檢查），背景 sync 由 TUI 測試（替身）與 `ProcessLauncher::command()` 單元測試涵蓋。

## Risks / Trade-offs

- [搬移時不小心改到訊息文字或處理順序，造成行為差異] → 驗收條件是 `crates/tui/tests` 的測試函式與 `tests/snapshots/*.snap` 都不修改即通過（`git diff --exit-code crates/tui/tests/snapshots`）；support 檔只允許改 import
- [`Workspace` 與 TUI `App` 同時持有清單相關狀態，兩邊不同步] → 清單模型只存在 `Workspace`；TUI 只保留游標，並在每次 `refresh_visible` 後依可見列數修正
- [`actions.rs` 拆解後單一檔案仍超過 800 行] → 依操作類別拆成 `workspace/{triage,pending,reload,sync,links,vex}.rs`
- [`Runtime` 被提早 drop 導致日誌遺失] → `Runtime` 由 `commands/tui.rs` 持有到 `cvewatch_tui::run` 返回；以註解說明

## Migration Plan

純程式碼重構，沒有資料或設定遷移。依任務順序逐步搬移，每個任務群組結束時 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 全過；任一步驟失敗可單獨 revert 該 commit。
