# Design

## Context

Windows Terminal / PowerShell 7 為主要終端機；內容大量中文（備註、資產名稱），需要正確處理全形字寬。

## Goals / Non-Goals

**Goals:**
- 開啟 1 秒內顯示 Dashboard（資料量為數千筆）
- 任何寫入操作與 CLI 行為完全一致

**Non-Goals:**
- 即時串流 sync 進度（只顯示子程序結束與結果）

## Decisions

### 架構

Elm 式：`App { screen_stack, state }`，事件迴圈 `crossterm::event::poll(250ms)` → `update(msg)` → `draw()`。
資料讀取在進入畫面時一次載入到記憶體（數千列沒問題），篩選與排序在記憶體中做。

### 畫面與鍵位

```
┌ cvewatch ─ Dashboard ───────────────────── last sync 14:05 ✓ ┐
│ P1 open  2   P2 open  5   P3 open 14   P4 open 31             │
│ KEV hits 2   Pending 7 (stale 1)   Conflicts 1                │
│ Sources: cvelist ✓ 14:05  nvd ✓ 14:05  kev ✓  epss ✓  osv ✗   │
└ [1]Dash [2]Findings [3]Assets [4]Pending [5]Sync  ?help q quit┘
```

| 鍵 | 動作 |
|---|---|
| `1`–`5` | 切換主畫面 |
| `↑↓` / `j k` | 移動 |
| `Enter` | 進入詳情 |
| `Esc` / `h` | 返回 |
| `/` | 即時搜尋（CVE ID、資產名稱、描述） |
| `f` | 篩選面板 |
| `s` | 切換排序 |
| `Space` | 多選 |
| `r` / `i` / `u` | 設為已處理 / 忽略 / 未處理（彈出備註；忽略時先選原因，若為「不受影響」再選理由，附中文說明） |
| `o` | 開啟連結（多個連結時跳出選單） |
| `y` | 複製 CVE ID 到剪貼簿（arboard） |
| `R` | 重新載入資料 |
| `S` | 背景執行 sync |
| `?` | 說明 |
| `q` | 離開（有未儲存備註時確認） |

Findings 清單欄位：`等級 | KEV | ? | CVE | CVSS | EPSS | 資產 | 元件@版本 | 狀態 | 首次發現`

### 新資料偵測

每 5 秒查 `SELECT max(id) FROM sync_run WHERE status IS NOT NULL`；大於載入時的值 → 狀態列顯示「有新資料，按 R 重新整理」。不自動重新整理，避免使用者正在看的列跳動。

### 背景 sync

`S` → `std::process::Command::new(current_exe()).arg("sync")` 以 `CREATE_NO_WINDOW` 啟動，TUI 記錄 child handle；結束後依結束碼在狀態列顯示結果。若結束碼 3（已有 sync）顯示「排程 sync 正在執行」。

### 中文字寬

ratatui 以 `unicode-width` 計算；表格欄寬以顯示寬度截斷並加 `…`。測試以含中文的 fixture 驗證不破版。

### 測試

ratatui `TestBackend` 做快照測試（insta）：每個畫面一張基準快照、篩選後、彈出備註框。

## Risks / Trade-offs

- 舊版 conhost 對部分框線字元支援不佳 → 偵測 `WT_SESSION` 不存在時改用 ASCII 框線
- 剪貼簿在 SSH 遠端無效 → 失敗時只在狀態列提示

## Implementation Decisions（/opsx-auto 依建議自行決定）

- **crate 結構**：`crates/tui`（lib `cvewatch-tui`），模組 `app`（狀態與按鍵）、`actions`（寫入與外部效果）、`dialog`（對話框純函式狀態轉移）、`filter`、`data`（載入快照）、`ui/*`、`width`、`glyphs`、`links`、`services`、`terminal`。crossterm 一律經 `ratatui::crossterm` 使用，避免與 ratatui 內部版本不一致；`tui-input` 關閉預設 feature，按鍵自行轉成 `InputRequest`。
- **終端機還原**：使用 `ratatui::try_init()`／`try_restore()`；`try_init` 會安裝 panic hook，panic 時先還原終端機。只處理 `KeyEventKind::Press`（Windows 會同時送出放開事件）。
- **core 擴充**：`FindingRow` 新增 `asset_name`、`description`、`first_seen`（搜尋描述與「首次發現」欄位需要）；新增 `matching::dashboard`（Dashboard 統計）、`runlog::finished_marker`、`Justification::label()`（理由選單的中文說明）。
- **新資料偵測**：design 原寫 `max(id) WHERE status IS NOT NULL`，但 `sync_run.status` 一開始就是 `running`，會在 sync 開始時就誤報。改為 `dashboard::DataMarker`：已結束（`finished_at IS NOT NULL`）的 `sync_run` 最大 id 與筆數，加上全量比對完成時間（`run_match` 寫入的 meta `match_last_run`）。任一來源結束或比對完成（sync 結尾的自動比對、`cvewatch match`、資產匯入後的比對）即改變。標記在載入資料前先讀，載入期間完成的寫入仍會被偵測；TUI 自己啟動的 sync 結束時立即檢查，不等下一次輪詢。
- **KEV 命中數**：Dashboard 顯示「KEV 命中（未處理）」＝未處理命中中 CVE 已列入 KEV 者（Dashboard 用途是「今天要處理什麼」）。
- **時間顯示**：資料庫存 UTC；每筆時間以本機時區（`DisplayTz::Local`，含日光節約時間）換算後顯示；測試使用 `DisplayTz::Fixed(UTC)` 以確保快照穩定。
- **離開確認**：備註框中 `q` 是輸入文字，因此「有未送出備註時確認」以 `Ctrl+C` 觸發確認框（y 或再按一次 `Ctrl+C` 離開；n 或 Esc 回到備註）；沒有對話框時 `q`／`Ctrl+C` 直接離開。
- **等待區鍵位**：design 未指定，採 `a` 套用到資產、`d` 標為不相關（皆先輸入備註）；套用目標列出每個使用中資產，資產有多個產品元件時另列 `<資產>:<產品>`，與 CLI `--apply` 相同。core 的錯誤（例如已不在等待區）顯示在狀態列。
- **批次狀態變更**：所選命中在同一個 IMMEDIATE 交易內逐筆呼叫 `triage::set_status`，任一筆失敗整批回復；完成後重新載入資料並清除多選。
- **寫入後重新載入**：使用者自己的寫入（狀態變更、等待區處理）後立即重新載入並更新開啟中的詳情；只有背景／排程 sync 的新資料才需按 `R`。
- **連結**：CVE.org、NVD、原廠修正公告（`csaf_fix.url`）、CVE 紀錄參考連結；去重，且只開啟 `http(s)` 網址（參考連結來自外部資料，不得啟動其他協定）。只有一個連結時直接開啟，其餘先顯示選單。
- **背景 sync**：`--config` 只在使用者明確指定時傳給子程序（環境變數會自動繼承）；子程序標準輸出入接到 null。本程序啟動的 sync 執行中再按 `S` 不重複啟動；離開 TUI 不終止子程序（讓 sync 完成並釋放鎖）。結束碼 3 顯示「已有 sync 正在執行」。
- **ASCII 模式**：除框線外，✓/✗/●/▶ 也改為 `OK`/`NG`/`*`/`>`。
- **Findings 篩選面板**：列出狀態、等級（可多選）、信心、KEV、資產（循環選擇）與重設；Space／←→ 變更、Enter 套用、Esc 取消。排序 `s` 依序循環：風險 → CVSS → EPSS → 首次發現 → 資產。
- **Findings 的 Esc**：主畫面無上一層可返回，`Esc` 清除多選與搜尋。

### Code review 第 1 輪後的調整

- **多選只對可見列生效**：多選在搜尋、篩選、排序變動時保留（縮小搜尋再放寬不會遺失），但批次變更只套用到目前可見的所選命中；摘要列顯示「已選 N（另 M 筆未顯示，不會變更）」。變更後只從多選移除被變更的命中（從詳情變更單筆時保留清單上的多選）；重新載入後移除已不存在的命中。（第 2 輪修正第 1 輪「隱藏即移除」的做法）
- **捲動**：各清單的捲動位置保存在 `App::offsets`（`Cell`），游標離開可見範圍才捲動；每次繪製只建立可見範圍的列。命中詳情的捲動上限依換行後的行數（ratatui `unstable-rendered-line-info` 的 `Paragraph::line_count`）計算，不會捲過結尾。
- **重繪**：只在按鍵、視窗大小改變或背景狀態（sync 結束、新資料提示、訊息）改變時重繪，閒置時不重建畫面。
- **狀態列訊息**：顯示到下一次按鍵為止，之後恢復該畫面的鍵位提示。
- **標題列最後同步**：任一來源最近一次同步失敗即標示 ✗ 與失敗來源數，不只看最後結束的來源。
- **寫入後重新載入失敗**：狀態變更／等待區處理已寫入但重新載入失敗時，訊息同時說明「已寫入」與「重新載入失敗，按 R 重試」。
- **等待區套用目標**：只列出含產品元件的使用中資產（等待區只含產品；只有套件的資產套用必定失敗）。

### Code review 第 2 輪後的調整

- **中斷的同步**：沒有結束、且開始超過 sync 鎖逾時（2 小時）的 `sync_run` 視為中斷：Dashboard 以各來源最近一次「結束」的紀錄判定成功／失敗，之後仍在執行（未逾時）的另外標示「執行中（開始 hh:mm）」；Sync log 顯示「中斷」。
- **終端機還原**：事件迴圈與還原都失敗時回報事件迴圈的錯誤（原因），還原錯誤寫入日誌。
- **日誌**：預設日誌篩選加入 `cvewatch_tui`，TUI 的 info 記錄（例如背景 sync 的 PID）會寫入日誌檔。
- **詳情的外部文字**：CVE 描述保留換行分段；其他外部文字的控制字元換成空白，避免前後文字黏在一起。
- **搜尋效能**：每列的小寫搜尋鍵在載入時建立一次，輸入搜尋字串時不再重複轉小寫。
- **ASCII 模式**：`→`、`⚠`、`↑↓`、`←→` 也納入 `Glyphs`，conhost 下改為 `->`、`!`、`Up/Down`、`Left/Right`；提示文字的 `1–5` 改為 `1-5`。
- **Findings 摘要列**：依重要性排列（筆數、多選、搜尋、排序、篩選），畫面窄時從右側截斷。
- **測試相依**：`tempfile`、`wiremock`、`zip` 由 `tests/support` 引入的 core 測試建構器使用，維持為 dev-dependencies（審查認為未使用，實際為間接使用；已在 Cargo.toml 註明）。

### Code review 第 3 輪後的調整

- **各來源的開始時間**：`cvewatch sync` 原本以整次 sync 的開始時間寫入每個來源的 `sync_run.started_at`，長時間的首次同步會讓後段來源被誤判為「中斷」。改為記錄各來源實際開始的時間（`ingest::sync::run`）。OSV 結果是否為最新（`runlog::last_success`）因此以 OSV 實際開始查詢的時間為準；OSV 查詢的是開始當下的元件，判定依舊正確。
- **批次狀態變更的共用函式**：新增 `triage::set_status_many`（單一 IMMEDIATE 交易、全部成功才寫入），`cvewatch finding set` 與 TUI 共用。CLI 傳 `skip_unchanged = false`（行為不變：同狀態也記錄事件與備註）；TUI 傳 `true`，略過狀態、原因、理由都相同的命中，不寫入「未處理 → 未處理」這類事件，並在訊息中說明略過筆數。
- **多選全部被隱藏**：按 r/i/u 時不變更、也不改用游標列，狀態列說明原因（放寬搜尋／篩選或按 Esc 清除多選）。
- **詳情的命中已不存在**：重新載入時關閉該詳情並在訊息中說明。
- **截斷標記**：conhost 的中文字型把 `…`（East Asian Ambiguous）畫成 2 欄會使欄位錯位，ASCII 模式改用 `~`（`Glyphs::ellipsis`）。
- **搜尋**：欄位以不會出現在資料中的分隔字元（U+001F）串接，各欄位與查詢的連續空白（含換行）正規化為一個空格，跨越換行的片語也能搜尋。
- **效能**：資產元件改為一次查詢（`ComponentRepo::all_by_asset`）並在記憶體彙整命中數；詳情畫面的按鍵處理不再複製整份詳情資料。
- **常數**：Sync log 的提示使用 `SYNC_LOG_LIMIT`；原因／理由說明文字統一由 `dialog::change_detail` 產生。
