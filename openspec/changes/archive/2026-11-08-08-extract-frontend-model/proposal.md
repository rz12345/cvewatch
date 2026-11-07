# Proposal

## Why

下一個 change（09）要新增 Windows GUI（`cvewatch-gui.exe`），功能與 TUI 同等。目前 TUI 的畫面資料載入、命中篩選與排序、狀態變更、等待區處理、背景 sync、開啟連結、設定表單的驗證與儲存，都和 ratatui 的事件迴圈綁在 `crates/tui` 的 `App` 上。不先拆開的話，GUI 只能複製這些邏輯，兩邊的寫入行為與訊息就會分歧，違反「結果與 CLI 的狀態變更紀錄相同」的既有要求。

## What Changes

- 新增 `crates/frontend`（`cvewatch-frontend`）：不依賴任何 UI 框架的前端共用層，內容自 `crates/tui` 搬移：
  - 畫面資料：`Snapshot`、`AssetEntry`、`DetailData`、寫入用的 `Settings`
  - 命中清單模型：篩選、排序、即時搜尋、多選與「只套用到可見的所選命中」規則
  - 有副作用的操作：狀態變更（含 Redmine 狀態註記）、等待區處理、重新載入、新資料偵測、背景 sync、開啟連結、複製 CVE ID、VEX 匯出；結果以「操作報告」回傳，由各前端自行顯示
  - 外部效果的抽象（瀏覽器、剪貼簿、sync 子程序、時鐘、憑證庫、通知後端、背景工作）與其系統實作
  - 設定表單：欄位模型、即時驗證、寫回 `cvewatch.toml`（保留註解）、秘密寫入憑證庫、連線測試／載入 Redmine 清單／重新安裝排程的背景工作
  - 啟動：由執行環境建立前端 `Context`（開啟加密資料庫、產品目錄、系統服務），供 TUI 與 09 的 GUI 共用
- `cvewatch-core` 新增執行環境啟動（載入設定 → 資料目錄 → 日誌與清理 → 憑證庫），自 `crates/cvewatch/src/app.rs` 搬入；未知欄位與日誌失敗的警告改為回傳，由呼叫端決定顯示方式（CLI 印到 stderr，GUI 之後以對話框顯示）
- `crates/tui` 改為只負責 ratatui 繪製、鍵盤事件、對話框與輸入框狀態，其餘呼叫 `cvewatch-frontend`
- `crates/cvewatch` 的 `tui` 子命令改用 `cvewatch-frontend` 的啟動函式；其他 CLI 子命令行為不變
- 背景 sync 子程序的執行檔路徑改由呼叫端指定（TUI 仍為 `cvewatch.exe` 本身；09 的 GUI 會指定同資料夾的 `cvewatch.exe`）

外部行為完全不變：TUI 的畫面、鍵位、訊息文字、寫入的事件與設定檔內容都與目前相同，既有 TUI 測試與 insta 快照不修改即通過。

## Capabilities

### New Capabilities

（無。本 change 為純重構，`.openspec.yaml` 設定 `skip_specs: true`）

### Modified Capabilities

（無）

## Out of scope

- 任何 GUI 程式碼與 egui 相依（屬於 09-add-gui-console）
- TUI 的畫面、鍵位或訊息文字調整
- 顯示寬度截斷（`width.rs`）與框線字元偵測（`glyphs.rs`）：只對終端機有意義，留在 `crates/tui`
- 對話框與文字輸入框（`tui-input`）的按鍵處理：屬於 TUI 的互動方式，不搬移
- CLI 其他子命令的重構（`asset`、`sync`、`notify` 等維持現狀）

## Impact

- 新 crate：`crates/frontend`，加入 workspace members；相依 `cvewatch-core`、`rusqlite`、`chrono`、`toml_edit`、`open`、`arboard`、`zeroize`、`tracing`
- `crates/tui`：移除已搬移的模組，改相依 `cvewatch-frontend`；`toml_edit`、`open`、`arboard` 相依移到 frontend
- `crates/core`：新增 `runtime` 模組（自 CLI 搬入的啟動邏輯）
- `crates/cvewatch`：`app.rs` 改呼叫 `core::runtime`（CLI 仍把警告印到 stderr，文字不變）；`commands/tui.rs` 改用 frontend 建立 `Context`
- 測試：既有 `crates/tui/tests` 全部保留；搬移的單元測試跟著模組移到 frontend；`crates/tui/tests/support` 的測試建構器改為依賴 frontend 的公開 API
