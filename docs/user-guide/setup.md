# 首次設定與通知

```powershell
cvewatch init                                    # 建立資料目錄、範例設定檔、加密資料庫
cvewatch db export-key                           # 顯示復原金鑰，請自行存入密碼管理器
cvewatch asset template assets.xlsx              # 取得空白清冊範本，填好後匯入
cvewatch asset import assets.xlsx
cvewatch sync                                    # 首次同步（約 650 MB baseline）並自動比對
cvewatch tui                                     # 按 6 進入「設定」畫面
cvewatch schedule install                        # 建立 cvewatch\sync 與 cvewatch\digest 兩個排程工作
```

> 首次 `cvewatch init` 後請立刻執行 `cvewatch db export-key`，把畫面顯示的復原金鑰自行存進密碼管理器（cvewatch 不會另外保存，也不要存成檔案或截圖）。Windows 重灌或換帳號時，沒有它就打不開資料庫。

清冊的填寫方式見[資產清冊](assets.md)；設定畫面也可以在[視窗版](gui.md)中操作（`Ctrl+6`）。

## TUI「設定」畫面

在 TUI「設定」畫面（`6`）：

1. **Email** 分頁：填 SMTP 主機、連接埠、加密（STARTTLS / TLS）、帳號、寄件者與收件者；密碼按 Enter 以遮罩輸入。按 `t` 以尚未儲存的值寄出測試信
2. **Redmine** 分頁：填網址（必須 https）與 API key，按 `l` 從 Redmine 載入專案、tracker、優先權與成員，以清單挑選（無法連線時可直接輸入數字 ID）；按 `t` 測試（不建立議題）
3. **排程與門檻** 分頁：同步間隔、每日摘要時間、緊急門檻（KEV 或 CVSS ≥ 門檻）
4. `Ctrl+S` 儲存：一般欄位寫回 `cvewatch.toml`（保留註解），密碼與 API key 只存入 Windows Credential Manager；排程時間有變時按 `i` 重新安裝排程

## 通知行為

- 首次啟用 Email 或 Redmine 時，既有的緊急命中會一次通知（Email 合併為一封、Redmine 依 CVE 各開一張議題）；
  既有命中很多時，建議先在 TUI 處理（resolved／ignored）後再啟用
- 每次 sync 後，未處理且 CVE 在 KEV 或 CVSS ≥ 門檻的命中立即寄出一封合併的緊急信，並依 CVE 在 Redmine 開一張議題（之後有新資產受影響時加註記；在 cvewatch 變更狀態時加註記，不變更議題狀態）
- 每日摘要涵蓋上次成功寄出的摘要至今；筆電離線數天後開機只寄一封
- 寄送失敗會留在送出佇列，下次 sync 或摘要時重送（5 分、30 分、2 小時，之後每 6 小時；10 次後標為無法送出）：`cvewatch notify outbox`
- 指令列：`cvewatch notify urgent | digest | test-email | test-redmine | outbox`，`cvewatch schedule install | status | uninstall`
- 排程以目前 Windows 使用者「登入時」執行（可讀取 Credential Manager，不需儲存密碼），使用者未登入時不會執行；
  錯過時間會在開機後補跑，只在有網路時執行。排程透過與 `cvewatch.exe` 同資料夾的 `cvewatchw.exe` 執行，不會跳出主控台視窗
- 機構內部 CA 簽發的 SMTP / Redmine 憑證：在設定檔 `[notify] ca_file` 指定 CA 的 PEM 檔。無加密 SMTP 或 http Redmine 只能手動設定
  `allow_insecure_transport = true`（TUI 頂端會顯示紅色警示），且仍不會以明文送出密碼或 API key
