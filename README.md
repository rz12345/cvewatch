# cvewatch

單人使用的「資產 × CVE」比對工具：匯入 Excel 資產清冊 → 同步相關 CVE → 規則比對 → 風險排序 → TUI 或視窗版處理 → Email / Redmine 通知。
規格在 `openspec/`，程式碼由 Claude Code 依 change 逐步實作；已實作的 change 封存在 `openspec/changes/archive/`。

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
| 08 | `08-extract-frontend-model` | 抽出 TUI 與視窗版共用的前端層 `cvewatch-frontend` |
| 09 | `09-add-gui-console` | Windows 視窗版 `cvewatch-gui.exe`（egui） |

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

Workspace 由 `crates/core`（資料、比對、通知與執行環境啟動）、`crates/frontend`（TUI 與 GUI 共用、不相依 UI 框架的前端層：畫面資料、命中清單模型、狀態變更等操作與設定表單）、`crates/tui`（ratatui 繪製與按鍵）、`crates/gui`（egui 視窗版）與 `crates/cvewatch`（CLI 執行檔）組成。

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

正式建置：`scriptsuild.bat` 以 release 建置 `cvewatch.exe`、`cvewatchw.exe`、`cvewatch-gui.exe`，並複製到 `dist\`（已 gitignore；三個檔案需放在目標電腦的同一資料夾）；
`check` 先執行 `cargo fmt --check`、clippy 與 `cargo test`，`debug` 改用 debug 建置，`nodist` 不複製到 `dist\`。

開發時測試視窗版：`scripts\dev-gui.bat` 會建置 `cvewatch` 與 `cvewatch-gui`，在 `.dev\`（已 gitignore）建立獨立的設定檔與資料目錄、
首次匯入 `tests\fixtures\inventory_basic.xlsx`，再以 `--config .dev\cvewatch.toml` 開啟視窗；`reset` 重建測試環境、`release` 改用 release 建置、`noseed` 不匯入範例清冊。
資料庫金鑰在 Credential Manager 與正式環境共用（只沿用、不覆蓋）。

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

## 比對、命中與等待區

```powershell
cvewatch match                                   # 全量重算（sync 結束、asset import / import-packages 後會自動執行）
cvewatch finding list --status open --priority P1,P2   # 依風險排序；? = probable、! = 已處理但仍在受影響範圍
cvewatch finding show 42                         # 各來源證據、修正版本、來源矛盾與時間軸
cvewatch finding set 42 ignored --reason not_affected --justification requires_configuration --note "未啟用 GlobalProtect"
cvewatch finding conflicts                       # 最終判定不受影響、但有來源判定受影響的組合
cvewatch pending list --stale                    # 等待區（KEV 置頂）；--stale 只列逾期
cvewatch pending resolve CVE-2099-0001 --apply PVE-01   # 或 --dismiss；資產有多個相關元件時用 PVE-01:proxmox-ve
```

- 判定優先序：原廠 CSAF 公告 > CNA affected > NVD CPE > CISA ADP；套件元件以 OSV 判定。版本以產品目錄的 `version_scheme` 比較，無法解析一律視為未知（建立 probable 命中，不判定為不受影響）
- 風險等級門檻在設定檔 `[scoring]`、等待區逾期天數在 `[pending] stale_days`；修改後下次比對重算
- 清冊只填 ESXi build 編號時，需要種子目錄或 `catalog.d` 的 `[[esxi_build]]` 對照表才能與 Update 名稱比較；對照不到時判定為未知

## 首次設定流程

```powershell
cvewatch init                                    # 建立資料目錄、範例設定檔、加密資料庫
cvewatch db export-key                           # 復原金鑰存入密碼管理器
cvewatch asset template assets.xlsx              # 取得空白清冊範本，填好後匯入
cvewatch asset import assets.xlsx
cvewatch sync                                    # 首次同步（約 650 MB baseline）並自動比對
cvewatch tui                                     # 按 6 進入「設定」畫面
cvewatch schedule install                        # 建立 cvewatch\sync 與 cvewatch\digest 兩個排程工作
```

在 TUI「設定」畫面（`6`）：

1. **Email** 分頁：填 SMTP 主機、連接埠、加密（STARTTLS / TLS）、帳號、寄件者與收件者；密碼按 Enter 以遮罩輸入。按 `t` 以尚未儲存的值寄出測試信
2. **Redmine** 分頁：填網址（必須 https）與 API key，按 `l` 從 Redmine 載入專案、tracker、優先權與成員，以清單挑選（無法連線時可直接輸入數字 ID）；按 `t` 測試（不建立議題）
3. **排程與門檻** 分頁：同步間隔、每日摘要時間、緊急門檻（KEV 或 CVSS ≥ 門檻）
4. `Ctrl+S` 儲存：一般欄位寫回 `cvewatch.toml`（保留註解），密碼與 API key 只存入 Windows Credential Manager；排程時間有變時按 `i` 重新安裝排程

通知行為：

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

## 資產清冊

資產以 Excel／CSV 清冊匯入，也可以在視窗版直接新增與修改；**資料庫是目前狀態，清冊是交換格式**。

```powershell
cvewatch asset template assets.xlsx              # 空白範本（assets、components 工作表與範例列）
cvewatch asset import assets.xlsx --dry-run      # 預覽差異（新增、更新、版本變更、清冊中消失），不寫入
cvewatch asset import assets.xlsx                # 匯入並自動比對
cvewatch asset export inventory.xlsx             # 匯出目前使用中的資產（格式與匯入相同）
cvewatch asset rename NAS-01 NAS-11              # 更改資產編號（元件、命中與處理紀錄保留）
```

- `asset export`：每個資產一列 `assets`（第一個元件寫在該列的 product / version），其餘元件與未辨識產品列在 `components`；產品寫目錄的顯示名稱、版本寫原始字串，因此匯出後直接重匯為「新增 0、更新 0」。不含已退役資產與主機套件（套件仍以 `asset import-packages` 管理）；目的檔已存在時不覆蓋；匯出檔未加密，含資產清單，請妥善保管
- `asset rename <舊編號> <新編號>`：只能更名使用中資產；新編號不可與其他資產（含已退役、不分大小寫）重複，只改大小寫可以（之後匯入的清冊須使用新的寫法：與既有編號只差大小寫的列會被拒絕）。元件、版本歷史、命中與處理紀錄都保留，比對結果不變。**系統不記錄舊編號**：更名後，舊清冊與已匯出的 VEX 中的舊編號不會對應到此資產（匯入舊清冊會把舊編號當成新資產，搭配 `--prune` 時更名後的資產會被標為退役；VEX 的判定列為未對應），舊編號可再被新增使用。已送出的通知與既有 Redmine 議題內容保留舊編號，之後的通知與註記使用新編號
- 匯入會以檔案內容**覆寫**資產欄位與清冊元件（檔案中沒有的清冊元件會被移除）。在視窗版修改過資產後，請依「**先匯出、再編輯、再匯入**」的順序，避免舊清冊蓋掉畫面上的修改（更名後也一樣，舊清冊中的舊編號不會對應到更名後的資產）：

```powershell
cvewatch asset export inventory-new.xlsx         # 1. 匯出目前狀態
#                                                  2. 用 Excel 編輯 inventory-new.xlsx
cvewatch asset import inventory-new.xlsx --dry-run   # 3. 確認差異只有剛才的編輯
cvewatch asset import inventory-new.xlsx
```

## 視窗版（cvewatch-gui）

`cvewatch-gui.exe` 是與 TUI 功能同等的 Windows 視窗介面（不開主控台視窗），與 CLI、TUI 共用同一個加密資料庫與設定檔，可以同時開啟。

- **三個執行檔放在同一資料夾**：`cvewatch.exe`、`cvewatchw.exe`、`cvewatch-gui.exe`。視窗版的同步、匯入清冊與重新安裝排程都是以隱藏視窗執行同資料夾的 `cvewatch.exe`；找不到它時這些按鈕會停用（滑鼠停留時說明原因），其他功能照常
- 設定檔解析順序與 CLI 相同：`--config` → 環境變數 `CVEWATCH_CONFIG` → 使用者設定目錄。指定設定檔時，由視窗啟動的 sync、清冊匯入與排程重新安裝也使用同一個設定檔：

```powershell
.\cvewatch-gui.exe
.\cvewatch-gui.exe --config D:\cvewatch\cvewatch.toml
```

- 繁體中文使用 Windows 內建的微軟正黑體（找不到時依序改用正黑體 Light、細明體；都沒有時頂端顯示警告）
- 繪圖預設用 wgpu（DirectX 12），無法使用時自動改用 OpenGL；兩者都不行（部分遠端桌面或虛擬機）時會跳出訊息框，請改用 `cvewatch tui`

與 TUI 的對照：

| 畫面 | TUI | 視窗版 |
|---|---|---|
| Dashboard | `1` | 側邊導覽或 `Ctrl+1`；點選 P1–P4 或 KEV 數字直接開啟套用篩選的命中畫面 |
| 命中 | `2` | `Ctrl+2`；點欄位標題（等級、資產、CVSS、EPSS、首次發現）排序，`Ctrl+F` 搜尋，「篩選…」開啟篩選面板 |
| 資產 | `3` | `Ctrl+3`；「新增資產」「編輯」「退役…」、「匯入清冊」（或把檔案拖進視窗）、「下載範本」「匯出清冊」、「顯示已退役」 |
| 等待區 | `4` | `Ctrl+4` |
| 同步紀錄 | `5` | `Ctrl+5` |
| 設定 | `6` | `Ctrl+6`；「儲存」按鈕取代 `Ctrl+S`，「測試寄信」「測試 Redmine」「從 Redmine 載入」「重新安裝排程」為按鈕 |

| 操作 | TUI | 視窗版 |
|---|---|---|
| 多選 | `Space` | `Ctrl`＋點選加選、`Shift`＋點選範圍選取 |
| 已處理／忽略／未處理 | `r`／`i`／`u` | 相同按鍵（文字欄位沒有焦點時）、工具列按鈕或右鍵選單 |
| 詳情 | `Enter` | 按兩下或 `Enter`；連結以按鈕開啟（只開 https） |
| 複製 CVE ID | `y` | `y` 或詳情中的「複製 CVE ID」 |
| 匯出 VEX | `E` | `Shift+E` 或「匯出 VEX」（另存新檔對話框，預設「文件」資料夾，不覆蓋既有檔案） |
| 背景同步／重新載入 | `S`／`R` | 「同步」「重新載入」按鈕，或 `Shift+S`／`F5` |
| 等待區套用／不相關 | `a`／`d` | 相同按鍵或按鈕 |
| 編輯資產／退役 | — | 資產清單選取使用中的資產後按 `Enter`／`Delete`（文字欄位沒有焦點、沒有對話框時），或「編輯」「退役…」按鈕 |

匯入清冊（拖放 .xlsx 或 .csv）：

1. 先顯示預覽（等同 `cvewatch asset import <檔案> --dry-run`），資料庫尚未變更
2. 可勾選「將清冊中消失的資產標為退役」（`--prune`）；CSV 清冊可「選擇元件檔」（`--components`）
3. 按「匯入」才寫入，之後自動比對並重新載入畫面；一次只接受一個 .xlsx 或 .csv

匯入預覽頂端會提醒：匯入以檔案內容覆寫資產欄位與清冊元件。在視窗中修改過資產後，請先「匯出清冊」、編輯匯出的檔案，再匯入（見「資產清冊」）。

資產編修（資產畫面）：

- 資產清單以編號、名稱、未處理命中數三欄列出（未處理數大於 0 時以紅字顯示）；詳情中的產品以目錄的顯示名稱呈現（例如「Proxmox VE 8.2.2」，滑鼠停留時顯示產品 key），類別與曝露度以「英文值（中文）」顯示，例如「server（伺服器）」，清冊與 CLI 仍使用英文值
- **新增資產／編輯**：對話框編輯資產欄位（標「*」者為必填；類別、曝露度、重要性為下拉選單；編輯時可修改資產編號＝更名，規則同 `cvewatch asset rename`，欄位下方會提示舊清冊與已匯出的 VEX 不會對應到新編號）與元件（「加入元件」可搜尋產品目錄，開啟後可直接輸入搜尋字，選擇產品後游標移到該元件的版本欄；只能選既有目錄產品）。驗證規則與清冊匯入相同，錯誤以介面用語（例如「必填」）紅字顯示在欄位下方，對話框不關閉；按「取消」不寫入。主機套件元件唯讀（由 `cvewatch asset import-packages` 管理）；未辨識產品可「改為目錄產品」（沿用原版本字串）或刪除
- **退役…**＝標為退役（確認對話框標題「將 <資產編號> 標為退役」）：命中與處理紀錄保留、不再參與比對，可恢復。勾選「顯示已退役」後，已退役資產以灰字與「（已退役）」列出，可「恢復」或「永久刪除」
- **永久刪除**會一併移除該資產的元件、版本歷史、命中與處理紀錄，**無法復原**；需輸入完全相同的資產編號才能確認。請先執行 `cvewatch db backup` 備份資料庫
- 每次編修後（只更名時不需要）在背景執行 `cvewatch match`（狀態列顯示「比對中…」，視窗仍可操作），完成後重新載入；找不到 `cvewatch.exe` 或比對失敗時編修仍保留，請手動執行 `cvewatch match`
- **下載範本**、**匯出清冊**：另存新檔對話框（預設「文件」資料夾，檔名 `cvewatch-inventory-template.xlsx`、`cvewatch-inventory-YYYYMMDD.xlsx`），不覆蓋既有檔案；匯出內容與 `cvewatch asset export` 相同，檔案未加密

啟動錯誤：資料庫尚未建立、Credential Manager 中沒有資料庫金鑰或設定檔有誤時，視窗會顯示原因與對應的指令（例如 `cvewatch init`、`cvewatch db import-key`），按「關閉」後結束。設定檔有未知欄位時照常開啟，頂端以黃色警告列顯示。

仍只在 CLI 提供：`cvewatch init`、`cvewatch db export-key | import-key | rekey | backup | restore`、`cvewatch schedule status | uninstall`、`cvewatch asset import-packages`（主機套件清單）、`cvewatch vex import`。

## 實作前請先確認

- 機構 SMTP 的連接埠與認證方式（實作完成後在 TUI「設定」畫面填寫並測試）
- 03 任務 1.1：各外部端點的 URL 與格式已於 2026-11-07 實測（見 `openspec/changes/03-add-cve-ingestion/design.md`）
