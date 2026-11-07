# Proposal

## Why

前面四個 change 都能用 CLI 操作，但日常判斷「今天要處理什麼」需要一個能快速瀏覽、篩選、看證據、改狀態的介面。使用者偏好單一執行檔、終端機即可使用的 TUI。

## What Changes

- `cvewatch tui` 開啟 ratatui 介面，畫面：
  - **Dashboard**：各等級未處理數、KEV 命中、等待區（含逾期）、各來源最後同步狀態
  - **Findings**：命中清單，可篩選（狀態、等級、資產、信心、KEV）、排序、多選後批次改狀態
  - **Finding detail**：CVE 描述、各來源 CVSS/EPSS/KEV、證據（哪個來源、哪個版本區間）、矛盾提示、原廠修正版本、時間軸、參考連結
  - **Assets**：資產與元件清單，進入可看該資產所有命中；顯示未辨識產品
  - **Pending**：等待區清單，可手動套用到資產或標為不相關
  - **Sync log**：最近同步紀錄與錯誤
- 鍵盤操作（方向鍵與 vim 鍵位皆可）、`/` 即時搜尋、`?` 說明
- 改狀態時彈出備註輸入框
- `o` 以預設瀏覽器開啟 CVE.org / NVD / 原廠公告連結
- 偵測到背景 sync 寫入新資料時顯示提示並可一鍵重新整理
- `S` 在背景啟動一次 `cvewatch sync` 子程序（不阻塞 TUI）

## Capabilities

### New Capabilities

- `tui-console`: 終端機介面的畫面、導覽、篩選、狀態操作與資料重新整理行為

### Modified Capabilities

（無）

## Out of scope

- 在 TUI 中編輯資產或產品目錄
- 滑鼠操作（不禁止，但不保證）
- 主題自訂

## Impact

- 新 crate `crates/tui`（ratatui、crossterm、tui-input、open），由 `cvewatch tui` 呼叫
- 只透過 `core` 的 repository 存取資料；狀態變更走 `finding-triage` 的同一套函式，確保事件紀錄一致
