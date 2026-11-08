# Tasks

前提：08-extract-frontend-model 已 archive。GUI 測試一律以 `egui_kittest::Harness`（不啟用 `snapshot`／`wgpu`）、`crates/core/tests/common` 的記憶體資料庫與 `cvewatch_frontend::test_support` 替身執行，不呼叫真實外部 API、不存取真實 Credential Manager、不啟動真實子程序。

## 1. crate 骨架、啟動與字型

- [ ] 1.1 新增 `crates/gui`（套件名 `cvewatch-gui`，lib + `src/bin/cvewatch-gui.rs`，`#![cfg_attr(windows, windows_subsystem = "windows")]`），相依 `eframe` 0.37（關閉預設功能，啟用 `accesskit`、`default_fonts`、`wgpu`、`glow`）、`egui_extras` 0.37、`rfd` 0.17.x；dev-dependency `egui_kittest` 0.37。完成條件：`cargo build --release` 產生 `target\release\cvewatch-gui.exe`，雙擊開啟空白視窗且沒有主控台視窗
- [x] 1.2 `startup.rs`：解析 `--config`，呼叫 `core::runtime::start(location, "gui")` 與 `frontend::context::build`；失敗時顯示錯誤畫面（design D7），`Runtime.warnings` 在主畫面頂端顯示黃色訊息列。完成條件：kittest 以「資料庫不存在」與「金鑰遺失」兩種 `CoreError` 建立錯誤畫面，斷言畫面含 `cvewatch init`／`cvewatch db import-key`；以含未知欄位的設定斷言警告文字
- [ ] 1.3 繪製後端 wgpu → glow → `rfd::MessageDialog` 的退回流程（design D6）。完成條件：以環境變數 `WGPU_BACKEND=invalid` 手動啟動時改用 glow 開啟視窗；在遠端桌面工作階段手動開啟一次並記錄結果於 design.md
- [ ] 1.4 `fonts.rs`：依序嘗試 `msjh.ttc`、`msjhl.ttc`、`mingliu.ttc` 並加入字族後備；找不到時回傳警告。完成條件：單元測試以暫存資料夾驗證「第一個存在的檔案被採用」與「全部不存在時回傳警告且不 panic」；手動確認視窗中「資訊所機房核心防火牆主機」正常顯示
- [x] 1.5 `companion.rs`：計算同資料夾 `cvewatch.exe` 並傳入 `ContextOptions`；提供隱藏視窗執行子程序並擷取 stdout／stderr 與結束碼的 trait 與實作。完成條件：單元測試以暫存資料夾驗證「存在 → Some(path)、不存在 → None」；fake companion 可記錄參數

## 2. 外框與 Dashboard

- [x] 2.1 `shell.rs`：側邊導覽六個畫面、`Ctrl+1`～`Ctrl+6`、狀態列顯示 `Report`（錯誤為紅色）、`allow_insecure_transport` 紅色警示列、「有新資料」提示與重新載入按鈕、同步按鈕（companion 不存在時停用並顯示提示）。完成條件：kittest 斷言按 `Ctrl+3` 切到資產畫面、設定 `allow_insecure_transport = true` 時每個畫面都有警示列、companion 為 None 時同步按鈕停用
- [x] 2.2 `GuiApp::update` 的輪詢：新資料（`request_repaint_after(NEW_DATA_POLL)`）、背景 sync 結束、背景工作中改為 200 ms（design D3）。完成條件：kittest 在測試中改寫資料庫的同步標記後推進時鐘，斷言出現「有新資料」提示且命中表格順序不變，按重新載入後才更新
- [x] 2.3 `screens/dashboard.rs`：各等級未處理數、KEV 數、等待區與逾期數、來源矛盾數、各來源最後同步時間與結果；點選等級或 KEV 數字跳到套用篩選的命中畫面。完成條件：kittest 斷言 OSV 最近一次失敗時顯示失敗與時間、點選 P1 後命中畫面篩選為「未處理、P1」

## 3. 命中表格、詳情與處理

- [x] 3.1 `screens/findings.rs`：`TableBuilder` 虛擬列、等級／CVE／資產／元件／版本／CVSS／EPSS／狀態／首次發現欄、probable（`?`）與仍受影響（`!`）標記；搜尋框即時篩選；篩選面板（狀態、等級、資產、信心、KEV、重設）；可排序欄位標題對應 `SortKey`（design D4）。完成條件：kittest 斷言預設只列未處理且依風險排序、輸入 `2026-0` 即時縮小清單、點選「CVSS」標題後順序與 `SortKey::Cvss` 相同
- [x] 3.2 多選（Ctrl／Shift 點選）、`r`／`i`／`u` 快捷鍵（僅在文字欄位未取得焦點時，design D10）、右鍵選單與工具列按鈕；`dialogs/reason.rs`（原因 → 不受影響時再選理由）與 `dialogs/note.rs`（備註，Esc／取消不變更）。完成條件：kittest 重現 spec 的 Batch ignore、Cancelling the note dialog、Selected findings hidden by search 三個情境，並斷言在搜尋框輸入 `r` 不會觸發狀態變更
- [x] 3.3 `screens/detail.rs`：按兩下或 Enter 開啟詳情面板（描述、各來源 CVSS、EPSS、KEV、各來源判定與版本區間、來源矛盾、修正版本、時間軸）、連結按鈕（經 `Workspace` 開啟，只允許 https）、複製 CVE ID；重新載入後以 `Workspace::reload` 的回傳更新或關閉詳情。完成條件：kittest 斷言 PAN-OS 命中詳情含判定來源、受影響區間與元件版本；點選原廠公告連結時 Opener 替身收到該網址；命中被刪除後重新載入，詳情關閉並顯示「已不存在」附註

## 4. 資產、等待區與同步紀錄

- [x] 4.1 `screens/assets.rs`：資產清單、選取後顯示元件、該資產命中與未辨識產品。完成條件：kittest 斷言選取 `PVE-01` 後顯示其元件與命中數
- [x] 4.2 `screens/pending.rs` 與 `dialogs/pending_target.rs`：原因、進入天數、逾期、KEV 置頂；套用到資產（多元件時選元件）或標為不相關，皆需備註。完成條件：kittest 重現 spec 的 Applying a pending CVE to an asset 情境，並斷言標為不相關後該 CVE 移出等待區
- [x] 4.3 `screens/sync_log.rs`：最近同步紀錄（新到舊、來源、結果、筆數、錯誤摘要）。完成條件：kittest 斷言列出順序為新到舊、失敗列顯示錯誤摘要

## 5. 設定畫面

- [x] 5.1 `settings/email.rs`：啟用、主機、連接埠、加密（只有 STARTTLS／TLS）、帳號、密碼（遮罩，`Zeroizing` 緩衝，design D11）、寄件者、多個收件者（新增／刪除）；欄位即時以 `settings::errors` 驗證並標示，無效時「儲存」停用。完成條件：kittest 斷言連接埠 `70000` 時欄位標示錯誤且儲存按鈕停用；加密選項中沒有「無加密」
- [x] 5.2 `settings/redmine.rs`：網址（必須 https）、API key（遮罩）、「從 Redmine 載入」以背景工作載入專案、tracker、優先權、成員並以下拉選單「名稱 (#ID)」挑選；載入失敗時顯示原因並改為數字輸入；狀態變更註記開關。完成條件：kittest 以 NotifyBackend 替身斷言載入後選擇專案與 tracker 顯示「基礎設施資安 (#12)」「漏洞 (#3)」且儲存後設定檔記錄數字 ID；替身回傳連線失敗時顯示錯誤原因且欄位改為數字輸入
- [x] 5.3 `settings/schedule.rs`：同步間隔、摘要時間（HH:MM）、緊急門檻（0–10）；儲存後排程有變更時提示並提供「重新安裝排程」按鈕（companion 不存在時停用）。完成條件：kittest 斷言摘要時間由 08:30 改為 07:45 儲存後出現提示，按鈕呼叫 NotifyBackend 替身的 install_schedule
- [x] 5.4 儲存與測試：「儲存」以 `settings::save` 寫回設定檔（保留註解）並將秘密寫入憑證庫替身；「測試寄信」「測試 Redmine」以表單目前值在背景執行並顯示結果。完成條件：kittest 斷言儲存後暫存設定檔的 `notify.email.smtp_host` 更新且原註解仍在、API key 只出現在 MemoryStore 而不在設定檔；未儲存即測試時 backend 收到表單中的 465/TLS 且設定檔不變
- [x] 5.5 關閉保護（design D9）：設定有未儲存變更或備註對話框有內容時，切換畫面或視窗關閉要求都顯示儲存／放棄／取消。完成條件：kittest 送出 close_requested 事件，斷言出現確認對話框、選取消後視窗未關閉且修改仍在、選放棄後才關閉

## 6. VEX 匯出與清冊匯入

- [x] 6.1 「匯出 VEX」：以檔案對話框 trait（正式實作為 `rfd::FileDialog`，預設文件資料夾、預設檔名）取得路徑，呼叫 `Workspace` 匯出目前篩選；不覆蓋既有檔案。完成條件：kittest 以對話框替身重現 spec 的 Exporting filtered findings 與 Target file already exists 情境（暫存資料夾）
- [x] 6.2 `dialogs/import.rs`：「匯入清冊」按鈕（開啟檔案對話框）與拖放（只接受單一 .xlsx／.csv）；以 companion 執行 `asset import ... --dry-run` 顯示預覽，「將消失的資產標為退役」勾選、CSV 的「選擇元件檔」；按「匯入」以相同參數去掉 `--dry-run` 執行，完成後 `Workspace::reload`（design D8）。完成條件：kittest 以 fake companion 斷言拖放 `assets.xlsx` 時先只執行含 `--dry-run` 的命令、確認後才執行不含 `--dry-run` 的命令且帶 `--prune`（有勾選時）與 `--config`（有指定時）；取消後不執行第二次；拖放 `report.pdf` 顯示只支援 .xlsx／.csv 的說明
- [ ] 6.3 手動整合測試：以 `cargo build --release` 的實際執行檔，把 `cvewatch asset template` 產生並填好的清冊拖進視窗，確認預覽、匯入、自動比對與重新載入正常，過程中沒有主控台視窗閃現；記錄結果於 design.md

## 7. 中文輸入、文件與整合檢查

- [ ] 7.1 手動測試中文輸入法：以注音輸入法在搜尋框、備註、收件者欄位輸入中文，確認組字與候選字可用、內容正確寫入；結果（含任何限制）記錄於 design.md
- [x] 7.2 README 新增「視窗版（cvewatch-gui）」段落：三個執行檔需放在同一資料夾、`--config`、與 TUI 的對照（畫面、快捷鍵）、拖放匯入清冊、啟動錯誤的處理方式、仍只在 CLI 提供的功能清單；並更新 README 開頭「本 repo 目前只有 OpenSpec 規格」的過時敘述。完成條件：README 中的指令照抄可執行
- [x] 7.3 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 全過；`cargo tree -p cvewatch-gui` 不含 ratatui、crossterm；`crates/gui/src` 每個檔案不超過 800 行

## Workflow follow-up

- `/code-review` 修正（最多三輪，某輪無發現即提前結束）→ commit
- `/opsx:archive 09-add-gui-console`（同步主 spec `gui-console`），`openspec validate --all --strict` 通過 → commit
