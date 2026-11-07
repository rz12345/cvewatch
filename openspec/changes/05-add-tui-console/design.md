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
