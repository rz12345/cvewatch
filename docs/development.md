# 開發

規格在 `openspec/`，程式碼由 Claude Code 依 change 逐步實作；已實作的 change 封存在 `openspec/changes/archive/`。
專案限制寫在 `openspec/config.yaml` 的 `context`，Claude Code 的工作規則見 [CLAUDE.md](../CLAUDE.md)。

## 前置

Node.js ≥ 20.19、Rust stable（MSVC toolchain）、OpenSpec CLI；SQLCipher 內建 OpenSSL 的編譯另需 Strawberry Perl 與 NASM（`winget install StrawberryPerl.StrawberryPerl NASM.NASM`）。

```powershell
npm install -g @fission-ai/openspec@latest
cd cvewatch
openspec update          # 依你本機的 OpenSpec 版本更新 .claude/ 指令與 skill
openspec list            # 列出進行中的 change
openspec validate --all --strict
```

## 建置（Windows MSVC）

Workspace 由 `crates/core`（資料、比對、通知與執行環境啟動）、`crates/frontend`（TUI 與 GUI 共用、不相依 UI 框架的前端層：畫面資料、命中清單模型、狀態變更等操作與設定表單）、`crates/tui`（ratatui 繪製與按鍵）、`crates/gui`（egui 視窗版）與 `crates/cvewatch`（CLI 執行檔）組成。各 crate 與外部系統的關係見[架構圖](#架構圖)。

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

正式建置：`scripts\build.bat` 以 release 建置 `cvewatch.exe`、`cvewatchw.exe`、`cvewatch-gui.exe`，並複製到 `dist\`（已 gitignore；三個檔案需放在目標電腦的同一資料夾）；
`check` 先執行 `cargo fmt --check`、clippy 與 `cargo test`，`debug` 改用 debug 建置，`nodist` 不複製到 `dist\`。

測試資料在 `tests/fixtures/`，來源與用途見 [tests/fixtures/ingest/README.md](../tests/fixtures/ingest/README.md)。

## 以 Claude Code 實作 change

一次只做一個 change，依編號順序；完成並封存後才開始下一個。

全自動（apply → code review → 架構變更判斷 → commit → 必要時更新架構圖 → archive，流程見 [.claude/commands/opsx-auto.md](../.claude/commands/opsx-auto.md)）：

```
/opsx-auto <change>
```

逐步進行：

```
/opsx:explore <change>      # （選用）先讓 Claude 讀過、提問題
/opsx:apply <change>        # 依 tasks.md 實作
# 檢查、cargo test 全過、commit
/opsx:archive <change>      # 規格併入 openspec/specs/
```

實作過程中若發現規格要改，請 Claude 用 `/opsx:update` 修改該 change，而不是直接改程式碼跳過規格。

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
| 10 | `10-add-gui-asset-editing` | 視窗版資產新增、編修、退役與永久刪除，下載範本，`asset export` |
| 11 | `11-polish-asset-ux` | 視窗版資產畫面與對話框的可用性修正 |
| 12 | `12-rename-asset-tag` | 資產更名（`asset rename`、視窗版編輯對話框） |
| 13 | `13-isms-asset-class` | 資產類別改為 ISMS 子類別、以 C/I/A 取代重要性、ISMS 資產編號 |

01–13 已封存；之後的 change 以 `openspec list` 查看。各 change 的需求背景見[需求決策紀錄](decisions.md)。

## 架構圖

[architecture/architecture.html](architecture/architecture.html)：以 [Archify](https://github.com/tt-a1i/archify)（專案層級 skill，位於 `.claude/skills/archify`）產生的互動式系統架構圖，以瀏覽器開啟。來源是 `docs/architecture/architecture.json`，每個節點都附上釘在特定 commit 的原始碼連結。
`/opsx-auto` 會在 change 涉及 crate 結構與相依或外部整合時自動更新這張圖。要手動重產時，在 repo 根目錄執行：

```powershell
node .claude/skills/archify/bin/archify.mjs finalize architecture docs/architecture/architecture.json docs/architecture/architecture.html --repo-root . --quality showcase --json
```
