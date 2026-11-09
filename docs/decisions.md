# 需求決策紀錄

## 2026-11-07 訪談

以下保留訪談當時的決定作為歷史紀錄；之後的調整見文末「[後續變更](#後續變更)」，現況以[使用說明](user-guide/)與 `openspec/specs/` 為準。

| 項目 | 決定 |
|---|---|
| 使用者 | 只有自己 |
| 資產來源 | Excel / CSV 手動維護 |
| 介面 | Rust TUI（ratatui） |
| AI | MVP 不用，全部規則比對 |
| 資產類型 | 網路/資安、虛擬化與伺服器、儲存、OS 與套件 |
| CVE 來源 | CVE.org cvelistV5、NVD、CISA KEV、EPSS、原廠公告、TWCERT/CC |
| 比對方式 | CPE + 版本範圍（另加 CNA affected 版本區間，見下方說明） |
| 資料不足 | 進等待區，每次 sync 重查 |
| 儲存 | SQLite 單檔 |
| 排程 | 獨立 `sync` 指令 + Windows 工作排程器 |
| 通知 | Email（每日摘要 + 緊急即時）、Redmine 自動開案（KEV 或 CVSS ≥ 9） |
| 套件清單 | 匯入 SBOM / dpkg / rpm 清單 |
| 風險排序 | KEV > EPSS > CVSS + 曝露度 |
| 處理狀態 | 未處理 / 已處理 / 忽略 |
| 執行環境 | Windows 筆電 |
| 借鑑其他專案 | CSAF 原廠公告讀取器（參考 CIRCL Vulnerability-Lookup）；忽略需附原因並可匯出 CycloneDX VEX（參考 Dependency-Track、cve-bin-tool） |
| 敏感資料保護 | 資料庫 SQLCipher 加密（金鑰在 Credential Manager，可匯出復原金鑰）、備份預設加密、SMTP/Redmine 只允許 TLS |
| 通知設定 | SMTP、Redmine ID 與 API key 在 TUI「設定」畫面自行設定（Redmine ID 從 API 載入後挑選） |

### 與選項不同的一處調整

你選了「CPE + 版本範圍比對」。規格中把 **CVE.org 記錄裡 CNA 提供的 affected 版本區間**也列為主要依據，原因是：
NVD 自 2026-04-15 起只優先 enrich KEV、聯邦使用軟體與 EO 14028 critical software，其餘標為 Lowest Priority；CISA ADP 自 2024-12-10 起也不再補 CPE。Synology、Proxmox 等設備的新 CVE 很可能長期沒有 NVD CPE，只靠 CPE 會漏報。CPE 仍保留為證據來源之一。

## 後續變更

| 項目 | 變更 | change |
|---|---|---|
| CVE 來源 | 另加 OSV.dev（套件元件的漏洞查詢）；TWCERT/CC 以 cvelistV5 的指派單位（`assignerShortName`）辨識，不另外爬網頁 | 03 |
| 介面 | 抽出 TUI 與視窗版共用的前端層 `cvewatch-frontend`，新增 Windows 視窗版 `cvewatch-gui.exe`（egui），與 TUI、CLI 共用資料庫與設定檔 | 08、09 |
| 通知設定 | 除 TUI 外，也可在視窗版「設定」畫面設定 | 09 |
| 資產來源 | 視窗版可直接新增、編修、退役資產；新增 `asset export`。**資料庫是目前狀態，清冊是交換格式** | 10、11 |
| 資產編號 | 允許更名（`asset rename`、視窗版編輯），元件、命中與處理紀錄保留，不記錄舊編號 | 12 |
| 資產類型 | 改為 ISMS 資訊資產清冊 v2.1 的子類別（HW01–HW11、SW01–SW07），可在設定檔覆寫或新增；新增選填的 ISMS 資產編號 | 13 |
| 風險排序 | 同等級內的排序以資產價值 MAX(C, I, A) 取代重要性（1–3）；等級判定規則不變 | 13 |
