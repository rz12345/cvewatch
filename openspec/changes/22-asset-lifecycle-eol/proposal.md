# Proposal

## Why

cvewatch 目前只回答「這個版本有沒有已知漏洞」，不回答「這個版本或設備還有沒有原廠支援」。軟體版本分支 EoL 之後不再有修補，CVE 比對結果「沒有命中」反而容易讓人誤以為安全；硬體停止支援、保固、維護合約與軟體授權到期，也都是 ISMS 年度覆核要檢查的項目，使用者目前另外用試算表追蹤。20-asset-identity-network 加上廠牌與型號之後，cvewatch 已經有判斷硬體 EoL 需要的資料，可以把生命週期一起放進資產畫面與每日摘要。

## What Changes

- 資產新增三個選填日期：保固到期、維護合約到期、軟體授權到期。清冊 `assets` 工作表、範本、匯出、匯入（缺欄時保留既有值）、GUI 新增／編修與詳情、TUI 詳情、CLI `asset show` 都支援
- 軟體 EoL：
  - `cvewatch sync` 新增來源 `eol`：從 endoflife.date API v1 取得產品的版本分支（release cycle）與 EoL 日期，可用 `--source eol` 單獨執行；失敗時與其他來源一樣不影響其他來源，也不觸發重新比對
  - 產品目錄可為產品宣告 endoflife.date 的產品代稱（slug），元件版本依 design 的規則對應到版本分支
  - endoflife.date 沒有涵蓋的軟體／韌體（例如 Synology DSM），由產品目錄宣告各版本分支的 EoL 日期與來源網址；`catalog.d` 可增修。種子資料依原廠公告建立（tasks 中列為查證任務）
- 硬體 EoL：依資產的廠牌＋型號（20 的欄位）對應目錄的硬體型號清單（廠牌、型號與別名、EoL 日期、來源網址）。種子目錄**不含**任何硬體型號；版控只放虛構範例（例如 `Aruba XYZ-100`），真實型號由使用者寫在本機 `data/catalog.d/`
- 手動覆寫：資產可覆寫硬體 EoL（日期＋來源），清冊元件可覆寫軟體 EoL（日期＋來源）；覆寫優先於目錄與 endoflife.date。清冊新增選填的 `eol` 工作表記錄覆寫，範本與匯出都包含
- 生命週期狀態：已 EoL、即將 EoL、保固／合約／授權即將到期或已到期。提前天數預設 90 天，可在設定檔 `[lifecycle] reminder_days` 調整；retired 資產不提醒
- 畫面：GUI 與 TUI 的資產清單標示狀態並可篩選，資產詳情列出每一項日期、來源與剩餘天數；Dashboard 顯示提醒數並可跳到篩選後的資產清單
- 每日摘要新增「生命週期提醒」段落：提醒期限內的項目、摘要期間內到期的項目，以及已 EoL／已到期的資產數

## Capabilities

### New Capabilities

- `asset-lifecycle`：EoL 資料來源（endoflife.date 同步）、版本分支對應、軟體與硬體 EoL 的判定順序、生命週期狀態與提醒天數、CLI 顯示

### Modified Capabilities

- `asset-inventory`：新增生命週期日期欄位、清冊欄位與 `eol` 覆寫工作表、新增與編修時的生命週期欄位（皆為 ADDED，不修改既有 requirement）
- `product-catalog`：新增 endoflife.date 代稱宣告、目錄內的軟體 EoL 日期、硬體型號清單、種子 EoL 涵蓋範圍（皆為 ADDED）
- `gui-console`：新增生命週期欄位輸入、清單標示與篩選、詳情、Dashboard 提醒（皆為 ADDED）
- `tui-console`：新增資產清單與詳情的生命週期標示、篩選與 Dashboard 提醒（ADDED）
- `email-alerts`：新增每日摘要的生命週期提醒（ADDED）

## Out of scope

- 硬體的停售日（End of Sale）、停止新功能開發日、延伸支援（ELS／ESU）購買狀態；endoflife.date 的 `eoasFrom`、`eoesFrom` 只存不判定（詳情可顯示）
- 依 EoL 狀態調整命中的風險等級或優先序（比對與評分不變）
- 主機套件元件（dpkg／rpm／SBOM）與未辨識產品的 EoL
- 自動從原廠網站抓取硬體或 endoflife.date 未涵蓋產品的 EoL（只由目錄與手動覆寫提供）
- 保固／合約資料從原廠 API 自動查詢（例如以序號查保固）
- 立即 email 或 Redmine 議題（只在每日摘要提醒）
- 23-isms-export 的匯出欄位（ISMS 清冊匯出是否帶入生命週期日期由 23 決定）
- TUI 編修生命週期欄位（TUI 只顯示）

## Impact

- core：新模組 `lifecycle/`（狀態判定、版本分支對應、摘要計數）；`ingest/eol.rs` 新來源與 `Endpoints.endoflife`；`catalog/file.rs`、`catalog/mod.rs` 新欄位與 `[[hardware]]`；`assets/` 的 model、record、columns、sheet、validate、plan、import、export、template；migration（20 之後的下一號）新增資產與元件欄位、`eol_product`、`eol_release` 表；`config.rs` 新增 `[lifecycle]` 與 `[sources] eol_enabled`；`notify/digest.rs`、`notify/content.rs`
- 種子目錄 `crates/core/catalog/seed.toml`：endoflife.date 代稱與 DSM 的版本分支 EoL（依原廠公告；iDRAC9 不建，EoL 依伺服器世代訂定）
- frontend：`AssetListFilter` 新增生命週期篩選、快照載入 EoL 資料、表單欄位；GUI 資產清單、詳情、對話框與 Dashboard；TUI 資產畫面、詳情與 Dashboard；CLI `asset show`、`sync --source eol`
- 外部整合：新增 endoflife.date API（HTTPS GET，無驗證、無秘密）；不新增 crate 相依
- 依賴 20-asset-identity-network 的廠牌、型號欄位
- tests：`tests/fixtures/endoflife/` 錄製回應、wiremock；只用虛構型號與資產編號（FW-EDGE、SRV-01）
