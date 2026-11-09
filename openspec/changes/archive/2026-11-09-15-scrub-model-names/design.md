# Design

## Context

- 型號出現在四類地方（2026-11-09 盤點，數字為出現次數的量級）：
  - 種子目錄：PAN-OS 與 DSM 的別名含機型寫法
  - 測試與測試支援：以機型當資產編號（上百處，含 TUI 快照、frontend 單元測試、GUI／TUI 測試支援的 seeded world）、以機型當產品寫法或未辨識產品原文、`normalize` 的單元測試
  - fixture：`tests/fixtures/matching/*.csv`、`tests/fixtures/ingest/README.md`，以及由 `crates/core/examples/gen_inventory_fixtures.rs` 產生的 3 個 `inventory_*.xlsx`（`git grep` 不掃 xlsx，需解壓比對）
  - OpenSpec：兩份主 spec（4 處）、5 份已封存 change、`openspec/config.yaml` 的專案 context
- 14-expand-product-catalog 已建立：`data\sensitive-patterns.txt`（本機、gitignore）與 `scripts/check-sensitive.ps1`；種子目錄別名不得含型號的需求；本機 `catalog.d\site.toml`。
- 實際的型號清單與「真實 → 虛構」對照只存在本機，本文件不列出。

## Goals / Non-Goals

**Goals:**
- 完成後，以本機敏感字詞清單比對 `git ls-files` 的全部內容（含解壓後的 xlsx）沒有命中
- 測試的語意不變：只換字串，不刪測試、不放寬斷言

**Non-Goals:**
- 不改寫 git 歷史
- 不更動使用者資料目錄中的資料庫與清冊

## Decisions

### D1 虛構寫法的命名規則
- 資產編號改用角色＋序號：防火牆 `FW-EDGE`（原本已有 `FW-01`、`FW-CORE`，新名稱避開它們）、伺服器 `SRV-01`、儲存 `NAS-0N` 等；更名測試的目標編號改為原編號加尾碼（例如 `FW-EDGE2`；實作改為 `FW-BORDER`，見 A3）
- 產品寫法改用「廠牌＋`XYZ-<三位數>`」，同一廠牌在全 repo 用同一個虛構型號（例如 Palo Alto `XYZ-100`、Dell `XYZ-200`、Synology `XYZ-300+`，保留 `+` 以維持測試對符號正規化的涵蓋）
- `XYZ-` 前綴不是任何已知廠商的型號系列，並與 14 的 spec 範例一致
- 替代方案：隨機字串。可讀性差，測試失敗時不易判斷是哪一類設備，因此不採用

### D2 對照表只放本機，替換以腳本執行
- 「真實 → 虛構」對照寫在 `data\scrub-map.tsv`（gitignore），由一次性腳本在暫存區外執行替換；腳本放 scratch 目錄、不 commit，因為它的用途只有這一次
- 替換範圍以 `git ls-files` 為準，排除 `Cargo.lock`（雜湊值中可能碰巧出現相同數字串）與 vendored schema（`tests/fixtures/vex/schema/`，其中的 `RS256` 等是演算法名稱）
- 替換以完整詞比對（前後不是英數字），避免誤改雜湊或其他字串；替換後逐檔檢視 diff

### D3 種子目錄只留廠牌＋產品別名
- 移除 PAN-OS 與 DSM 的機型別名，保留 `palo alto`、`palo alto networks`、`pa-series` 等廠牌與產品線寫法
- 使用者實際用到的機型寫法寫入本機 `catalog.d\site.toml`，讓本機清冊升級後仍能辨識
- 這使種子目錄符合 14 新增的「Seed catalog excludes hardware models」

### D4 產生的檔案重新產生，不手改
- xlsx fixture：修改 `gen_inventory_fixtures.rs` 後執行 `cargo run -p cvewatch-core --example gen_inventory_fixtures` 重新產生
- TUI 快照：資產編號長度改變會影響欄寬，以 `cargo insta test` 產生新快照，逐一確認差異只有編號與對應的空白，再接受

### D5 已封存 change 直接改文字
已封存 change 是歷史紀錄，但仍在版控中。只做字串替換、不改任何決策內容；`openspec validate --all --strict` 仍須通過。

## 實作時的決定（apply，依建議做法）

- **A1 範圍擴及 ISMS 編號與表單編號**：Goals 要求「以本機清單比對沒有命中」，而清單也含機構的 ISMS 系統代碼與內部表單編號。因此 ISMS 編號範例一律改用虛構系統代碼 `SYS`（`SYS-HW-001`、`sys-hw-005`、格式錯誤範例 `SYSHW001`），已封存 13 的 proposal／design 刪去表單編號，只寫「ISMS『資訊資產清冊』」。程式中的格式提示（`ISMS_FORMAT`、匯入錯誤訊息）也改為 `SYS-HW-001`。
- **A2 補 spec delta 並同步主 spec**：主 spec 中含 ISMS 編號範例的需求一併列為 MODIFIED——`asset-inventory`「ISMS asset ID」、`gui-console`「GUI asset class and rating input」「GUI asset presentation」（只換範例字串）。為了讓實作 commit 本身就不含這些字串（review 建議），實作時已把全部 MODIFIED 需求的內容先寫進主 spec；archive 的同步以相同內容取代，結果不變。
- **A3 防火牆型號依語意分別替換**：作資產編號時改為 `FW-EDGE`；作產品寫法時，需要被辨識的 fixture 改用種子別名「Palo Alto PAN-OS」（spec 的寫法），純正規化測試與已封存 change 中的機型範例用「Palo Alto XYZ-100」。`FW-CORE` < `FW-EDGE` < `PVE-01` 與原本的排序相同。更名測試的目標編號**不採** D1 的「原編號加尾碼」，改為 `FW-BORDER`（名稱欄 `fwborder`）：GUI 測試以「畫面含有字串」判斷標題，目標若包含原編號（例如 `FW-EDGE2`），「標題維持原編號」的斷言就分辨不出新舊編號而被放寬（review 發現）。
- **A4 伺服器型號**：資產編號改為 `SRV-01`（與 `matching_pending_apply.rs` 既有的 `SRV-01` 不在同一個測試世界，不衝突）；機型寫法改為 `XYZ-200`；fixture 中依賴種子別名的「PowerEdge <機型> BIOS」改為種子別名「PowerEdge BIOS」；NVD fixture 的硬體 CPE 改為 `poweredge_xyz-200`。
- **A5 其他真實型號**：測試中的 Synology 未辨識產品原文（非使用者環境，但為真實型號）依 D1 改為 `XYZ-300+`；種子目錄 BIOS 註解改為 `poweredge_<機型>_firmware`。
- **A6 本機 `site.toml`**：補上原本內建的防火牆、DSM 機型寫法與三種「<機型> BIOS」寫法；`cvewatch asset catalog show paloalto-panos` 會列出這些本機別名（來源是 `catalog.d`，種子目錄本身已不含）。本機清冊以 `asset export` 匯出後 `--dry-run` 重匯為「新增 0、更新 0、未辨識 0」。
- **A7 敏感字詞清單補強**：本機清單的 `\b` 規則漏掉黏在其他字元後的寫法（正規化後的別名、CPE 產品名中的機型）與不含連字號的 ISMS 編號，已補上無邊界的規則、相鄰編號與 Synology 型號。替換腳本本身以「前後不是英數字」判斷，不受此影響。本文件也只以「防火牆型號」「伺服器型號」等描述指稱，不寫出型號。
- **A8 快照**：本機沒有安裝 `cargo-insta`，改以 `INSTA_UPDATE=new cargo test -p cvewatch-tui` 產生 `.snap.new`，將舊快照套用相同替換後以忽略空白的 diff 比對，確認差異只有字串、對齊空白與 multi-width 索引位移，再移除 `assertion_line` 標頭後取代舊檔。
- **A9 xlsx**：重新產生後，內容未變的 3 個 xlsx（只有 `docProps/core.xml` 時間戳不同）還原為原檔，只留下內容有變的 `inventory_basic`／`inventory_legacy`／`inventory_v2`。
- **A10 測試數量**：替換前後皆為 1069 passed、2 ignored。

### Code review 修正（一輪）

- **R1** 本文件原本在 A3–A7 寫出了真實型號與 ISMS 系統代碼，已改為描述性寫法（見 A7）
- **R2** 更名測試的目標編號改為 `FW-BORDER`，不包含原編號，「標題維持原編號」與其他以子字串判斷的斷言恢復原本的鑑別力（見 A3）；已封存 12 的 tasks 同步改寫
- **R3** 主 spec 在實作 commit 就先套用 MODIFIED 需求（見 A2），不必等到 archive
- **R4** 已封存 04 的任務 6.2 原本會被改成「XYZ 型號」，與實際 fixture（PAN-OS、ESXi/iDRAC）不符，已改為產品描述；已封存 02 design 的種子別名範例是當時的決定，維持虛構型號
- **R5** 使用說明的升級注意事項補充：`vex import` 也透過目錄別名辨識產品，以機型寫法稱呼產品的 VEX 也需要在 `catalog.d` 補上別名
- **R6** `catalog.rs` 的拼寫測試改用 `paloaltopanos`（去除分隔符號的輸入對上含分隔符號的別名 `palo alto pan-os`），保留原本測試涵蓋的情況（原本的輸入是去除連字號的機型寫法）
- **R7** `scripts/check-sensitive.ps1` 會解開暫存區的 xlsx／xlsm／docx／pptx，比對其中 XML 的文字，命中時以「檔名!內部檔名」回報；以含舊型號的 xlsx 驗證確實會回報命中

## 架構圖

不需更新：只改測試、fixture、種子目錄資料與文件，未涉及 crate 結構、相依或外部整合。

## Risks / Trade-offs

- [替換誤中無關字串（雜湊、演算法名）] → D2 的完整詞比對與排除清單；逐檔看 diff
- [快照或斷言因字串長度改變而失敗] → 依 D4 重新產生快照；字寬相關測試（例如截斷測試）改用長度相同的虛構字串，維持原本要驗證的邊界
- [使用者升級後清冊中的機型寫法變成未辨識] → D3 的本機 `site.toml`；使用說明與版本說明提醒
- [git 歷史仍保有原內容] → 已知限制（proposal 的 Out of scope）；之後若要改寫歷史需另外決定

## Migration Plan

1. 先完成並封存 14（`site.toml` 與檢查腳本已存在）
2. 本 change 實作後，使用者的 `catalog.d\site.toml` 已含原本靠種子別名辨識的機型寫法，重新匯入清冊應為「新增 0、更新 0」
3. 回復：還原本 change 的 commit 即可，資料庫無 schema 變更
