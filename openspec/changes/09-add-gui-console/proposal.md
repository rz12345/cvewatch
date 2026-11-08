# Proposal

## Why

TUI 適合在終端機裡快速處理，但有些日常操作在視窗介面中更順手：用滑鼠排序與多選大量命中、在詳情中同時看證據與時間軸、用檔案對話框選清冊、把 Excel 直接拖進視窗匯入。cvewatch 只在 Windows 筆電上使用，因此提供 Windows 視窗版 `cvewatch-gui.exe`，功能與 TUI 同等，與 TUI、CLI 並存並共用同一個加密資料庫與設定檔。08 已把兩個前端共用的邏輯抽到 `cvewatch-frontend`，本 change 只新增視窗介面本身。

## What Changes

- 新增 `cvewatch-gui.exe`（Windows GUI subsystem，不開主控台視窗），與 `cvewatch.exe` 放在同一資料夾；以 egui / eframe 實作
- 畫面與 TUI 同等：Dashboard、命中（篩選、排序、即時搜尋、多選、詳情）、資產（元件、各資產命中、未辨識產品）、等待區（套用到資產、標為不相關）、同步紀錄、設定（Email、Redmine、排程與門檻）
- 處理狀態、等待區、開啟連結、複製 CVE ID、背景 sync、新資料提示、VEX 匯出全部呼叫 `cvewatch-frontend`，寫入結果與 TUI、CLI 相同
- GUI 才有的操作：
  - 以 Windows 檔案對話框選擇 VEX 匯出位置
  - 以檔案對話框或把 .xlsx / .csv 拖進視窗匯入資產清冊：先顯示預覽（等同 `--dry-run`），確認後才寫入，可選擇是否將消失的資產標為退役
- 繁體中文顯示：使用 Windows 內建的中文字型，不另外打包字型
- 啟動失敗（資料庫尚未初始化、金鑰遺失、設定檔錯誤）以對話框說明原因與對應的 CLI 指令，不會無聲結束
- `cvewatch-gui.exe` 支援 `--config <path>`，與 CLI 相同的設定檔解析順序

## Capabilities

### New Capabilities

- `gui-console`: Windows 視窗介面：畫面、命中處理、設定、VEX 匯出、清冊匯入、啟動錯誤處理與中文顯示

### Modified Capabilities

（無。TUI、CLI、排程、清冊匯入規則都不變；GUI 匯入清冊沿用 `asset-inventory` 既有的匯入與預覽行為）

## Out of scope

- `cvewatch init`、資料庫金鑰（export-key / import-key / rekey）、備份與還原、排程 status / uninstall：仍只在 CLI 提供
- 主機套件清單匯入（`asset import-packages`）與 VEX 匯入：仍只在 CLI 提供
- 系統匣常駐、Windows 通知中心（toast）、開機自動啟動
- 安裝程式（MSI / MSIX）與程式碼簽章
- 深色／淺色主題切換以外的外觀自訂；多語系介面
- macOS / Linux 視窗版（程式可編譯但不測試、不保證）
- 打包中文字型到執行檔

## Impact

- 新 crate：`crates/gui`（套件名 `cvewatch-gui`，產生 `cvewatch-gui.exe`），相依 `cvewatch-frontend`、`cvewatch-core`、`eframe`、`egui_extras`、`rfd`；dev-dependency `egui_kittest`
- `cargo build --release` 產物多一個 `cvewatch-gui.exe`；首次編譯 wgpu 相關 crate 會增加建置時間與執行檔大小
- 清冊匯入、背景 sync、重新安裝排程以隱藏視窗的子程序執行同資料夾的 `cvewatch.exe`，因此兩個執行檔必須放在一起
- README 新增「視窗版（cvewatch-gui）」使用說明
