# Design

## Context

動機見 proposal.md - Why，行為見 `specs/gui-console/spec.md`。本 change 建立在 08（`cvewatch-frontend`）之上：

- `Workspace` 已提供狀態變更、等待區處理、重新載入、新資料輪詢、背景 sync、開啟網址、複製、VEX 匯出，結果以 `Report` 回傳
- `FindingsModel` 已提供篩選、搜尋、排序（`SortKey`：風險、CVSS、EPSS、首次發現、資產）、多選與「只套用可見的所選命中」
- `settings::{Form, errors, save, jobs}` 已提供設定表單的不可變模型、驗證、寫回設定檔、秘密寫入憑證庫與背景連線工作
- `core::runtime::start` 與 `frontend::context::build` 已提供啟動；`ContextOptions` 可指定子程序使用的 CLI 執行檔
- `test_support` feature 已提供外部效果的替身

TUI 的資料存取模式沿用不變：資料庫在 UI 執行緒上同步存取（快照數千列、在記憶體中篩選），只有網路工作（測試寄信、Redmine）與子程序在背景執行緒。排程 sync 與視窗同時開啟時，靠 SQLite WAL 與既有 busy timeout 處理鎖定，與 TUI 相同。

## Goals / Non-Goals

**Goals:**

- 只寫「畫面」：所有寫入與判斷都經 `cvewatch-frontend`，GUI crate 不直接呼叫 core 的寫入函式
- 在沒有 GPU 的測試環境也能以程式驗證主要互動流程
- 單一資料夾即可執行（`cvewatch.exe`、`cvewatchw.exe`、`cvewatch-gui.exe`），不需安裝 runtime 或字型

**Non-Goals:**

- 不追求原生 Win32 / WinUI 外觀
- 不記憶視窗大小、位置或欄寬（eframe `persistence` 不啟用）

## Decisions

### D1. egui / eframe 0.37 + egui_extras 表格

使用者已選定 egui。版本以 2026-11-08 crates.io 最新穩定版為準：`eframe` / `egui_extras` / `egui_kittest` 0.37.0。`eframe` 關閉預設功能，只啟用 `accesskit`、`default_fonts`、`wgpu`、`glow`（不需要 `wayland`、`x11`、`persistence`）。命中表格以 `egui_extras::TableBuilder` 的虛擬列（只繪製可見列）呈現，數千列也不卡頓。

替代方案已在規劃時比較（Slint、Iced、Tauri），由使用者選擇 egui，不再重議。

### D2. crate 結構：lib + 薄的 bin

`crates/gui` 同時是 library（`cvewatch_gui`，供 `tests/` 使用）與 binary（`src/bin/cvewatch-gui.rs`，`#![windows_subsystem = "windows"]`）。bin 只負責解析 `--config`、啟動、建立 eframe 視窗。模組依畫面拆分，單檔控制在 400 行左右：

```
crates/gui/src/
  lib.rs            GuiApp（實作 eframe::App）
  startup.rs        執行環境、Context、錯誤視窗
  fonts.rs          中文字型載入
  companion.rs      同資料夾 cvewatch.exe 偵測與子程序
  shell.rs          頂端警示列、側邊導覽、狀態列、快捷鍵
  screens/{dashboard,findings,detail,assets,pending,sync_log}.rs
  settings/{mod,email,redmine,schedule}.rs
  dialogs/{note,reason,pending_target,confirm_leave,import,error}.rs
```

### D3. GuiApp 的狀態與更新迴圈

`GuiApp` 持有 `Workspace` 與畫面專屬狀態（目前畫面、詳情面板開啟的命中與其 `DetailData`、各畫面選取列、開啟中的對話框、設定表單的原始值與編輯值、狀態列訊息）。每一幀：

1. 處理視窗關閉要求（D9）與拖放檔案（D8）
2. 輪詢 `Workspace` 的新資料與背景 sync、設定畫面的背景工作、匯入子程序
3. 繪製畫面與對話框；使用者操作轉為 `Workspace` 方法呼叫，回傳的 `Report` 顯示在狀態列（錯誤等級以紅色顯示）

閒置時以 `ctx.request_repaint_after(NEW_DATA_POLL)` 定期喚醒以偵測新資料；有背景工作時改為每 200 ms 喚醒。不在每一幀查詢資料庫。

### D4. 表格欄位排序對應 `SortKey`

點選欄位標題即設定 `FindingsModel.sort`：「等級」→ 風險、「CVSS」→ CVSS、「EPSS」→ EPSS、「首次發現」→ 首次發現、「資產」→ 資產；其餘欄位（CVE、元件、版本、狀態）標題不可點選。不另做升冪／降冪切換，排序方向與 TUI 相同，確保兩個前端對「依某欄排序」的結果一致。

### D5. 中文字型：載入 Windows 內建字型作為後備

egui 內建字型沒有中日韓字元。啟動時依序嘗試 `%WINDIR%\Fonts\msjh.ttc`（微軟正黑體，Windows 10/11 內建）、`msjhl.ttc`、`mingliu.ttc`，讀到第一個可用的檔案後加入 `Proportional` 與 `Monospace` 字族的後備清單（排在內建字型之後，英數字仍用內建字型）。都找不到時照常啟動並在頂端顯示警告。

替代方案：把 Noto Sans TC 打包進執行檔 —— 執行檔增加 10 MB 以上，且 Windows 筆電一定有正黑體，否決。`egui-cjk-font` 等第三方 crate 只是包裝同樣的作法，不另增相依。

### D6. 繪製後端：wgpu 優先，失敗改用 glow，再失敗用系統訊息框

eframe 0.34 起預設 wgpu（Windows 上走 DX12）。遠端桌面或部分 VM 可能無法建立 wgpu adapter，因此 bin 以 `Renderer::Wgpu` 啟動失敗時改以 `Renderer::Glow` 重試；兩者都失敗時以 `rfd::MessageDialog` 顯示原生錯誤訊息框（說明可改用 `cvewatch tui`）後結束。

### D7. 啟動錯誤視窗

`core::runtime::start` 或 `frontend::context::build` 失敗時，不建立 `GuiApp`，改以同一個 eframe 視窗顯示錯誤畫面：`CoreError` 的訊息原文（已含 `cvewatch init`、`cvewatch db import-key` 等提示）、設定檔路徑與「關閉」按鈕。`Runtime.warnings`（未知欄位、無法寫入日誌）在主畫面頂端以可關閉的黃色訊息列顯示。

### D8. 清冊匯入以子程序執行 `cvewatch.exe asset import`

匯入流程：選檔或拖放 → 以隱藏視窗的子程序執行 `cvewatch.exe [--config P] asset import <file> [--components C] [--prune] --dry-run`，擷取 stdout / stderr → 在預覽對話框以等寬字顯示輸出 → 使用者按「匯入」後以相同參數去掉 `--dry-run` 再執行一次 → 顯示結果並呼叫 `Workspace::reload`。子程序在背景執行緒等待，stdin 為 null（`asset import` 沒有互動提示）。結束碼非 0 時顯示 stderr 內容。

選擇子程序而非在行程內呼叫 core：

- 清冊解析、欄名對應、匯入計畫、`--prune` 與匯入後自動比對（`rematch_if_needed`，目前在 CLI crate）都與 CLI 完全同一段程式，不需再搬移比對邏輯
- 與背景 sync、重新安裝排程的作法一致（都經 `companion.rs`）
- 代價是預覽為 CLI 文字輸出而非結構化表格；對單人工具可以接受

拖放時只接受副檔名 .xlsx 或 .csv 的單一檔案，其他情況顯示說明。CSV 時預覽對話框提供「選擇元件檔」按鈕，選擇後重新預覽。

### D9. 關閉保護

每一幀檢查 `ctx.input(|i| i.viewport().close_requested())`；設定有未儲存變更或備註對話框有內容時，送出 `ViewportCommand::CancelClose` 並開啟儲存／放棄／取消對話框。標題列關閉鈕、Alt+F4 都走同一路徑。

### D10. 鍵盤操作

`Ctrl+1`～`Ctrl+6` 切換畫面、`Ctrl+F` 聚焦搜尋框、`F5` 重新載入。與 TUI 相同的單鍵（`r`、`i`、`u`、`o`、`y`、`E`、`a`、`d`、`S`、`R`）只在沒有文字欄位取得焦點時（`!ctx.wants_keyboard_input()`）生效，避免輸入備註或搜尋時誤觸。

### D11. 秘密欄位

密碼與 API key 以 `TextEdit::password(true)` 輸入，緩衝區為 `Zeroizing<String>`；儲存或取消後立即清空並 drop。表單只保存「已設定／未設定／待寫入」狀態（沿用 `settings::Secret`），秘密內容不進入 `Form` 的一般欄位，也不寫入日誌。

### D12. 子程序與同資料夾執行檔

`companion.rs` 在啟動時計算 `current_exe().with_file_name("cvewatch.exe")` 並檢查是否存在，傳入 `ContextOptions` 作為 sync 與排程 backend 的 CLI 執行檔。不存在時，同步、匯入清冊、重新安裝排程按鈕停用並顯示提示（spec：Companion command-line executable）。所有子程序都以 `core::process::hide_window` 啟動，不閃主控台視窗。

### D13. 測試：egui_kittest + AccessKit 查詢，不做影像快照

GUI 測試以 `egui_kittest::Harness` 在無視窗環境中執行 `GuiApp`，以 AccessKit 標籤找到按鈕、表格列與輸入框並模擬點擊與鍵入，再斷言 `Workspace` 的資料庫內容與畫面文字。資料庫使用 `crates/core/tests/common` 的記憶體建構器，外部效果使用 `cvewatch_frontend::test_support` 替身，子程序以 fake companion（記錄參數、回傳固定輸出）取代。不啟用 kittest 的 `snapshot` / `wgpu` 功能，測試不需要 GPU。

## Risks / Trade-offs

- [egui 在 Windows 注音／倉頡輸入法下的組字視窗位置或候選字行為不完整] → 任務中以手動測試驗證搜尋框、備註、設定欄位的中文輸入；若有問題，記錄於 design 並以 egui issue 追蹤，不阻擋其他功能
- [wgpu 讓執行檔變大、首次編譯變慢] → 接受；release 已啟用 LTO 與 strip。若大小不可接受，可只保留 glow（D6 已有 glow 路徑）
- [匯入預覽與實際匯入之間檔案被修改] → 實際匯入時重新讀檔，結果對話框顯示實際結果；預覽對話框註明「匯入時會重新讀取檔案」
- [GUI 與 TUI 同時開啟並寫入同一資料庫] → 與兩個 TUI 同時開啟相同：寫入走 core 交易，另一方以新資料提示得知變更
- [egui 的文字版面快取可能殘留秘密字元] → 密碼欄只顯示遮罩字元，egui 對遮罩欄位只排版遮罩字元；緩衝區本身以 `Zeroizing` 清除
- [AccessKit 查詢在 egui 升版時標籤或結構改變導致測試失敗] → 測試只依賴本專案自訂的標籤文字，不依賴 egui 內部結構

## Migration Plan

新增執行檔，不影響既有資料、設定與排程。回復方式為刪除 `cvewatch-gui.exe`；CLI 與 TUI 不受影響。

## Implementation Notes（apply 時依建議自行決定）

以下為實作時遇到需要調整或確認之處，依建議做法處理：

- **eframe 另啟用 `clipboard` 功能**：D1 只列 `accesskit`、`default_fonts`、`wgpu`、`glow`，但沒有 `clipboard` 時文字欄位無法貼上（例如貼上 Redmine API key、收件者）。「複製 CVE ID」仍經 `Workspace`（frontend 的剪貼簿替身可測試）。
- **eframe 0.37 的 `App` 介面**：0.37 以 `App::ui(&mut self, ui, frame)` 取代 `update`，面板改為 `egui::Panel::{left,right,top,bottom}(…).show(ui, …)`。`GuiApp::show(ui)` 是一幀的進入點，eframe（`startup::RootApp`）與 kittest（`Harness::build_ui_state`）都呼叫它。
- **rfd 使用 0.17.2**：tasks 指定 0.17.x（crates.io 已有 0.18.0），依 tasks 不升版。
- **連結只開 https**：frontend 的 `Workspace::open_url` 允許 http(s)；spec 要求視窗版只允許 https，因此 GUI 只顯示 https 連結，並在 `GuiApp::open_link` 再檢查一次（`app::is_https`）。
- **詳情文字、時間格式與等待區天數移到 `cvewatch-frontend`**（code review 第 1 輪）：新增 `cvewatch_frontend::display`（`sections`、`full_time`、`short_time`、`pending_days`），TUI 與 GUI 都由它產生詳情段落（TUI 的 ratatui 樣式、GUI 的標題與粗體各自處理），避免兩份文字分歧；TUI 的畫面輸出不變。
- **命中表格多一欄「資產名稱」**：spec 要求表格完整顯示中文資產名稱，因此在「資產」（資產編號）之後加「資產名稱」欄；欄寬可拖曳。另有「標記」欄顯示 `?`（probable）、`!`（已處理但仍受影響）與 `KEV`。
- **點選規則**：單擊為單選（並設為游標列）、`Ctrl`＋點選加選、`Shift`＋點選以上次點選的列為起點做範圍選取；右鍵點在未選取的列時先改為只選該列。批次變更沿用 `Workspace::list_targets`（只套用可見的所選命中）。
- **快捷鍵細節**（D10）：單鍵比對修飾鍵需完全相符（`consume_key` 會忽略多按的 Shift，`r` 與 `R` 必須分開）；TUI 的 `E`、`S`、`R` 對應 `Shift+E`、`Shift+S`、`Shift+R`，另加 `F5` 重新載入、`Enter`／`o` 開啟詳情、上下鍵移動游標、等待區 `a`／`d`／`y`。對話框開啟時不處理畫面快捷鍵。
- **備註可留空**：與 TUI 相同，備註對話框確認時直接送出（去除前後空白），是否必填由 core 驗證決定。
- **設定表單的編輯緩衝**：`Form::with_text` 會 trim，若每次按鍵都寫回會吃掉輸入中的空白（例如寄件者「cvewatch <…>」），因此 `SettingsPage` 保存各文字欄位與收件者的原始輸入，表單只存 trim 後的值；秘密以 `Zeroizing<String>` 緩衝，儲存、放棄後清空（D11）。清空秘密輸入框表示「不變更」，另有「清除密碼／清除 API key」按鈕刪除憑證庫中的值。
- **確認對話框選「儲存」時的備註**：關閉視窗時若備註對話框有內容，「儲存」會送出該備註（等同按「確認」），設定有變更時一併儲存；任一失敗則不關閉。
- **不使用 spinner**：egui 的 spinner 每一幀都要求重繪，背景 sync 期間改以文字顯示，結束由 200 ms 輪詢偵測（D3）。
- **未知命令列參數**：`cvewatch-gui.exe` 只接受 `--config <path>`／`--config=<path>`，其他參數以錯誤畫面說明（GUI subsystem 沒有主控台可輸出）。
- **清冊預覽與退役勾選**：勾選「將清冊中消失的資產標為退役」或選擇元件檔後，以新參數重新執行 `--dry-run` 預覽，確保按「匯入」時執行的參數與畫面上的預覽一致。
- **繪製後端退回**（D6）：`run_native` 在建立 `App` 之前失敗時，視窗內容留給下一個後端；若在建立之後才失敗，以同一個執行環境重建（見 code review 第 3 輪）。
- **code review 第 1 輪的行為調整**：
  - 清冊匯入的結束碼 4（清冊已寫入、自動比對失敗）視為已寫入：重新載入並以警告提示執行 `cvewatch match`；重新載入失敗時保留其錯誤訊息，不以「已重新載入」覆蓋。
  - 未儲存確認開啟時，下層的備註與匯入對話框暫不繪製、不接收 Esc；確認對話框選「儲存」時備註送出失敗（例如命中已被刪除）則不關閉視窗、保留備註。
  - Esc 改由畫面快捷鍵處理（沒有對話框、文字焦點或下拉選單時才關閉詳情），一次 Esc 只關閉最上層。
  - 右鍵選單的「開啟詳情」「複製 CVE ID」針對按右鍵的那一列；右鍵點在已選取的列只移動游標、不改變多選。
  - 游標列被搜尋／篩選隱藏或已不存在時不作為變更、詳情或複製的對象（`GuiApp::current_visible`）。
  - 上下鍵移動游標時表格捲動到該列；等待區選取 CVE 的連結快取到選取或資料重新載入為止，不在每一幀查詢資料庫。
- **code review 第 2 輪的調整**：
  - 設定畫面的狀態與動作（表單與比較基準、Redmine 清單、背景工作、儲存、測試、載入清單、重新安裝排程、取回結果）移到 `cvewatch_frontend::settings::SettingsState`，TUI 與 GUI 共用；動作回傳 `Saved`／`Refused`／`Finished`，說明文字（含按鍵或按鈕名稱）由各前端產生。TUI 的訊息文字不變。挑選欄位一律以「有非空清單就挑選，否則手動輸入」判斷（TUI 原本即如此）。
  - 清冊匯入的子程序改由 frontend 的 `CliTarget` 組出命令（`--config` 的傳遞方式與 sync、排程相同，參數以 `OsString` 傳遞，不做有損轉換）；GUI 不再另存 `cli_config`。子程序執行超過 10 分鐘即終止並回報；預覽（`--dry-run`）中可以取消，實際匯入中不可取消。
  - 連結規則移到 frontend：`Services.link_policy`（`LinkPolicy::Web`／`HttpsOnly`），`Workspace::open_url` 依規則拒絕並說明，`Workspace::link_allowed` 供畫面篩選連結。GUI 啟動時設為 `HttpsOnly`，TUI 維持 http(s)。
  - 外部文字去除控制字元改用共用的 `cvewatch_frontend::display::one_line`（TUI 的 `width::sanitize` 為其別名），命中表格與資產畫面所有來自清冊或 CVE 紀錄的文字都經過它。
  - 備註送出失敗（例如命中已被刪除）時保留對話框與備註；等待區的選取只在處理成功後清除。
  - 有對話框開啟時拖放清冊會被拒絕並提示；`Ctrl+F` 被未儲存確認攔下時不留下聚焦要求；原本就未設定的秘密按「清除」不算變更。
- **code review 第 3 輪的調整**：
  - 設定動作的說明文字也移到 frontend（`Saved`／`Refused`／`Finished::message`、`JobKind::started`），各前端只提供操作方式（`ActionWords`：TUI「按 Ctrl+S」「按 i 重新安裝排程」，GUI「按「儲存」」「請按「重新安裝排程」」）。TUI 取回背景工作時，被捨棄的過時清單結果也觸發重繪（修正共用化造成的回歸）。
  - 秘密欄位每一幀清除 egui `TextEdit` 的復原紀錄（避免明文快照留在記憶體，也讓 Ctrl+Z 無法叫回）；按「清除」後輸入再刪掉仍維持刪除。egui 內部排版快取仍可能短暫持有字元，屬 Risks 已列的限制。
  - 進入設定畫面時（沒有未儲存變更與背景工作）重新讀取設定檔與憑證庫狀態，與 TUI 相同，避免與同時開啟的 TUI、CLI 不一致；保留已載入的 Redmine 清單。
  - 實際匯入步驟結束後一律重新載入（失敗、逾時被終止時資料庫仍可能已寫入），並依結果提示成功、自動比對失敗或失敗。
  - 子程序結束或被終止後，最多再等 5 秒讀取輸出（孫程序仍持有管線時不無限等待）。
  - 連結規則改在產生連結處套用：`data::load_detail`／`cve_links` 依 `LinkPolicy` 只列出可開啟的連結，GUI 不再自行篩選。
  - 詳情段落在開啟或重新載入時產生一次並快取（`OpenDetail::sections`）；Dashboard 與資產畫面改為借用資料，不在每一幀複製。
  - 繪製後端退回（D6）：執行環境（日誌寫入器）由 `main` 保留到程式結束；第二個後端需要重建視窗內容時沿用同一個執行環境（`startup::rebuild`），不重新啟動日誌。
- **測試步長**：kittest 每步預設 0.25 秒，超過 egui 按兩下的 0.3 秒門檻，GUI 測試的 Harness 改為每步 0.05 秒。

## 手動驗證

### 已自動驗證（2026-11-08）

以暫存資料夾中的獨立設定檔啟動（資料庫不存在 → 錯誤畫面；不建立資料庫、不存取憑證庫與使用者資料），讀取該設定檔的日誌並以 DPI-aware 的 `PrintWindow` 擷取視窗：

| 任務 | 結果 |
|---|---|
| 1.1 | `cargo build --release` 產生 `target\release\cvewatch-gui.exe`；啟動後開啟標題為 cvewatch 的視窗（日誌「繪製後端 renderer=Wgpu」「視窗已建立」）。PE 標頭的 subsystem 為 2（Windows GUI；`cvewatch.exe` 為 3），雙擊不會開主控台視窗 → 勾選 |
| 1.3（前半） | `WGPU_BACKEND=invalid` 啟動：日誌依序為 wgpu「Failed to create surface for any enabled backend」、「無法以此繪製後端開啟視窗 renderer=Wgpu」、「繪製後端 renderer=Glow」，視窗以 glow 正常顯示錯誤畫面 |
| 1.4 | 日誌「載入中文字型 path=C:\WINDOWS\Fonts\msjh.ttc」；設定檔路徑含「資訊所機房核心防火牆主機」時錯誤畫面正確顯示這串字，長路徑訊息在 125% 縮放下正確換行 → 勾選 |

（第一次擷取時截圖程式不是 DPI-aware，只截到視窗左上部分，看起來像長路徑沒有換行；以 kittest 載入實際字型與 DPI-aware 重新擷取確認是截圖問題，不是程式問題。）

### 使用者已手動驗證

| 任務 | 結果 |
|---|---|
| 1.3（後半） | 2026-11-08 在遠端桌面工作階段開啟視窗，日誌「2026-11-08T09:06:21.889659Z INFO 繪製後端 renderer=Wgpu」：遠端桌面下 wgpu 可直接建立視窗，不需退回 glow。連同前半的 glow 退回驗證 → 勾選 |
| 6.3 | 2026-11-08 以 release 版：`cvewatch asset template` 產生範本（三列範例）→ 拖進 `cvewatch-gui.exe` 的資產畫面 → 預覽 → 匯入。日誌依序為 companion `asset import … --dry-run`（09:11:52）、`asset import …`（09:11:56）、「清冊匯入完成 added=3 updated=0」、「比對完成 components=4」；資產畫面列出 FW-01、NAS-01、R6515-01，過程中沒有主控台視窗閃現 → 勾選 |
| 7.1 | 2026-11-08 以注音輸入法測試：搜尋框、Email 收件者（皆為 `TextEdit::singleline`）組字與候選字正常。**備註欄未測，sync 後補測**：備註對話框是 GUI 唯一的 `TextEdit::multiline`，本機尚無 CVE 資料、沒有命中可開啟備註。使用者決定依現況勾選並封存 → 勾選 |

### 封存後待補測

| 項目 | 方式 |
|---|---|
| 備註欄注音輸入（7.1） | sync 後在命中畫面標為已處理或忽略（或在等待區標為不相關）開啟備註對話框，以注音輸入含換行的兩行中文，確認組字、候選字與換行正常，儲存後時間軸內容正確 |

## Open Questions

- 視窗與工作列圖示：第一版使用 eframe 預設圖示；之後若要自訂，只需加入 .ico 資源，不影響其他設計
