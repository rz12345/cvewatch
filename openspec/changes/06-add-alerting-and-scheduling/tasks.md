# Tasks

## 1. 資料表與 outbox

- [ ] 1.1 `0005_notify.sql`（notification_log、outbox、redmine_issue）
- [ ] 1.2 outbox 寫入、退避排程、dead 標記；sync / digest 開頭先清 outbox
- [ ] 1.3 測試：退避時間、10 次後 dead

## 2. 緊急事件判定

- [ ] 2.1 `NEW_URGENT` / `BECAME_KEV` 事件產生（掛在 04 比對之後）
- [ ] 2.2 notification_log 去重
- [ ] 2.3 測試：spec 中各情境（含 probable）

## 3. Email

- [ ] 3.1 lettre SMTP（starttls = `Tls::Required` / tls；none 僅在 `allow_insecure_transport`），選擇性驗證，密碼取自 Credential Manager；明文連線時拒絕送出密碼
- [ ] 3.2 緊急信內容組裝（一次 sync 一封）
- [ ] 3.3 每日摘要查詢與內容（五個段落、`send_empty`）
- [ ] 3.4 `notify urgent|digest|test-email|outbox`
- [ ] 3.5 測試：內容以 golden file 驗證；以本機假 SMTP（例如 `mailin-embedded` 或 wiremock 替身）驗證送出與失敗

## 4. Redmine

- [ ] 4.0 傳輸檢查：`base_url` 必須 https（例外開關除外）、`ca_file` 載入至 rustls 信任清單（SMTP 共用）；明文時拒絕送 API key

- [ ] 4.1 Redmine client（建立議題、加註記、查專案/tracker）
- [ ] 4.2 一 CVE 一議題、新增資產註記、triage 註記（掛在 04 triage 函式的事件 hook）
- [ ] 4.3 錯誤分類（暫時性 → outbox；422/401/403 → dead）
- [ ] 4.4 `notify test-redmine`
- [ ] 4.5 測試：wiremock 模擬建立、註記、502 重送、422；http 網址被拒；自簽 CA 經 `ca_file` 驗證通過、未設定時失敗

## 5. 排程

- [ ] 5.1 新 bin `cvewatchw`（windows_subsystem，CREATE_NO_WINDOW 啟動 `cvewatch.exe`，回傳結束碼）
- [ ] 5.2 Task Scheduler XML 產生（InteractiveToken、StartWhenAvailable、RunOnlyIfNetworkAvailable、IgnoreNew、PT1H）
- [ ] 5.3 `schedule install|status|uninstall`（呼叫 schtasks）
- [ ] 5.4 測試：XML 以 golden file 驗證；`cfg(windows)` 的整合測試標記 `#[ignore]`，在使用者機器上手動執行

## 6. TUI 設定畫面

- [ ] 6.1 設定表單模型（Email / Redmine / 排程與門檻），由 `Config` 載入；欄位驗證函式（與 CLI 共用）
- [ ] 6.2 畫面與鍵位：分頁切換、文字/數字/單選/核取方塊/清單欄位、遮罩秘密欄位、收件者新增刪除
- [ ] 6.3 `toml_edit` 寫回設定（保留註解、暫存檔 + rename）；秘密寫入/刪除 Credential Manager
- [ ] 6.4 Redmine 清單載入（projects 分頁、專案 trackers、priorities、memberships）與「名稱 (#ID)」顯示；連線失敗時改手動輸入
- [ ] 6.5 測試寄信 / 測試 Redmine（使用未儲存表單值，背景執行不阻塞畫面）
- [ ] 6.6 例外開關啟用時的紅色警示列（TUI 不提供開關）；未儲存變更確認對話框；排程變更提示與 `i` 重新安裝
- [ ] 6.7 測試：驗證規則單元測試；toml_edit 保留註解的 golden 測試；秘密不出現在設定檔；wiremock 模擬 Redmine 清單；TestBackend 快照（三個分頁、錯誤欄位、確認對話框）

## 7. 文件

- [ ] 7.1 README：首次設定流程（init → asset template/import → sync → tui 設定畫面填 SMTP/Redmine 並測試 → schedule install）
