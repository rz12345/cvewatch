# cvewatch

單人使用的「資產 × CVE」比對工具：匯入 Excel 資產清冊 → 同步相關 CVE → 規則比對 → 風險排序 → TUI 處理 → Email / Redmine 通知。
本 repo 目前只有 OpenSpec 規格，程式碼由 Claude Code 依 change 逐步實作。

## 需求決策紀錄（2026-11-07 訪談）

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

## 實作順序

| # | change | 內容 |
|---|---|---|
| 01 | `01-add-project-foundation` | workspace、CLI、設定、Credential Manager、SQLite、HTTP |
| 02 | `02-add-asset-inventory` | 資產/元件模型、產品目錄、Excel/CSV 與 SBOM 匯入 |
| 03 | `03-add-cve-ingestion` | cvelistV5、NVD、KEV、EPSS、OSV、通用 CSAF 原廠公告讀取器（預設 Palo Alto） |
| 04 | `04-add-asset-cve-matching` | 版本比較、證據判定、等待區、風險等級、處理狀態 |
| 05 | `05-add-tui-console` | ratatui 介面 |
| 06 | `06-add-alerting-and-scheduling` | Email、Redmine、Windows 排程、TUI 設定畫面 |
| 07 | `07-add-vex-exchange` | CycloneDX VEX 匯出/匯入（處理判斷可移轉到其他工具） |

## 在 VS Code + Claude Code 中使用

前置：Node.js ≥ 20.19、Rust stable（MSVC toolchain）、OpenSpec CLI；SQLCipher 內建 OpenSSL 的編譯另需 Strawberry Perl 與 NASM（`winget install StrawberryPerl.StrawberryPerl NASM.NASM`）。

> 首次 `cvewatch init` 後請立刻執行 `cvewatch db export-key`，把復原金鑰存進密碼管理器。Windows 重灌或換帳號時，沒有它就打不開資料庫。

```powershell
npm install -g @fission-ai/openspec@latest
cd cvewatch
git init
openspec update          # 依你本機的 OpenSpec 版本更新 .claude/ 指令與 skill
openspec list            # 應看到 7 個 change
openspec validate --all --strict
```

### 建置（Windows MSVC）

資料庫使用 rusqlite 的 `bundled-sqlcipher-vendored-openssl`，首次編譯會從原始碼建置 OpenSSL（約 5 分鐘）：

1. 安裝 Visual Studio Build Tools（「使用 C++ 的桌面開發」）與 Rust MSVC toolchain
2. `winget install StrawberryPerl.StrawberryPerl NASM.NASM`，安裝後重新開啟終端機讓 PATH 生效
3. 若 PATH 上先出現 Git for Windows 內附的 MSYS perl（`C:\Program Files\Git\usr\bin\perl.exe`），OpenSSL 會設定失敗；
   請以環境變數指定 Strawberry Perl：`$env:PERL = 'C:\Strawberry\perl\bin\perl.exe'`
4. 找不到 `nasm.exe` 時會改用無組譯最佳化的版本（較慢但可用）；winget 的 NASM 安裝在 `%LOCALAPPDATA%\bin\NASM`

```powershell
cargo build --release                          # 產出 target\release\cvewatch.exe（單一執行檔，不需另裝 SQLite/OpenSSL）
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                                     # 不呼叫外部 API、不存取真實 Credential Manager
```

在 Claude Code 中，一次做一個 change：

```
/opsx:explore 01-add-project-foundation      # （選用）先讓 Claude 讀過、提問題
/opsx:apply 01-add-project-foundation        # 依 tasks.md 實作
# 檢查、cargo test 全過、commit
/opsx:archive 01-add-project-foundation      # 規格併入 openspec/specs/
```

做完 01 再開始 02，依此類推。實作過程中若發現規格要改，請 Claude 用 `/opsx:update` 修改該 change，而不是直接改程式碼跳過規格。

## 同步 CVE（`cvewatch sync`）

```powershell
cvewatch sync                      # 依序：cvelist → nvd → csaf:* → kev → epss → osv
cvewatch sync --source kev         # 只更新指定來源（可重複；csaf 或 csaf:<id>）
cvewatch sync --rescan             # 目錄（catalog.d）變更後，以最新 baseline 重新篩選
cvewatch sync --history            # 最近 20 次各來源的執行結果
cvewatch cve show CVE-2024-3400    # 整合後的 CVE 資料；本機沒有時加 --fetch 直接向 CVE.org 取得
cvewatch cve search proxmox        # 在 baseline 索引中搜尋（含未存入本機者），找出漏掉的別名
```

- **首次同步會下載 cvelistV5 baseline（約 650 MB）**，串流解壓不落地，依網路速度約需數分鐘到十數分鐘；之後每次只下載當日 delta（數 KB～數 MB）。超過 25 天沒同步會重新下載 baseline
- 只保留與產品目錄或資產套件相關的 CVE；另在 `<data_dir>\cache\cvelist\search-index.jsonl.zst` 保留全部 CVE 的精簡索引供 `cve search` 使用
- 任一來源失敗時其他來源照常更新，失敗來源的資料與游標不變，結束碼為 4
- NVD API key（選用，速率由 5 次/30 秒提高到 50 次/30 秒）：`cvewatch config set-secret nvd_api_key`
- 新增提供 CSAF 2.0 公告的原廠：在設定檔加一筆 `[[sources.csaf]]`（見範例設定檔），不需更新程式

## 實作前請先確認

- 機構 SMTP 的連接埠與認證方式（實作完成後在 TUI「設定」畫面填寫並測試）
- 03 任務 1.1：各外部端點的 URL 與格式已於 2026-11-07 實測（見 `openspec/changes/03-add-cve-ingestion/design.md`）
