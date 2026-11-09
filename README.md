# cvewatch

單人使用的「資產 × CVE」比對工具：匯入 Excel 資產清冊 → 同步相關 CVE → 規則比對 → 風險排序 → TUI 或視窗版處理 → Email / Redmine 通知。

三個執行檔放在同一資料夾使用：

| 執行檔 | 用途 |
|---|---|
| `cvewatch.exe` | CLI 與 TUI（`cvewatch tui`） |
| `cvewatch-gui.exe` | Windows 視窗版，功能與 TUI 同等 |
| `cvewatchw.exe` | 工作排程器使用的無主控台包裝程式 |

## 快速開始

以 `scripts\build.bat` 建置（見[開發](docs/development.md#建置windows-msvc)），把 `dist\` 中的三個執行檔放到同一資料夾後：

```powershell
cvewatch init                                    # 建立資料目錄、範例設定檔、加密資料庫
cvewatch db export-key                           # 復原金鑰存入密碼管理器
cvewatch asset template assets.xlsx              # 取得空白清冊範本，填好後匯入
cvewatch asset import assets.xlsx
cvewatch sync                                    # 首次同步（約 650 MB baseline）並自動比對
.\cvewatch-gui.exe                               # 或 cvewatch tui
```

> 首次 `cvewatch init` 後請立刻執行 `cvewatch db export-key`，把復原金鑰存進密碼管理器。Windows 重灌或換帳號時，沒有它就打不開資料庫。

通知與排程的設定見[首次設定與通知](docs/user-guide/setup.md)。

## 文件

| 文件 | 內容 |
|---|---|
| [首次設定與通知](docs/user-guide/setup.md) | 初始化、TUI 設定畫面（Email、Redmine、排程）、通知行為 |
| [同步 CVE](docs/user-guide/sync.md) | `cvewatch sync`、CVE 來源、`cve show` / `cve search` |
| [比對、命中與等待區](docs/user-guide/matching.md) | `match`、`finding`、`pending`、風險等級 P1–P4 |
| [資產清冊](docs/user-guide/assets.md) | 清冊欄位、ISMS 子類別與 C/I/A、從舊版升級、匯出與更名 |
| [視窗版](docs/user-guide/gui.md) | `cvewatch-gui.exe` 的畫面與操作、與 TUI 的對照、匯入與資產編修 |
| [開發](docs/development.md) | 建置、測試、以 Claude Code 實作 change、實作順序、架構圖 |
| [需求決策紀錄](docs/decisions.md) | 2026-11-07 訪談的決定與後續變更 |
| [架構圖](docs/architecture/architecture.html) | 互動式系統架構圖（以瀏覽器開啟） |

正式規格在 [openspec/specs/](openspec/specs/)。
