# Proposal

## Why

版控中仍有真實的硬體型號：種子目錄的別名、測試與 fixture 的資產編號和產品寫法、TUI 快照、OpenSpec 主 spec、已封存的 change 與 `openspec/config.yaml`。這違反 14-expand-product-catalog 建立的規則（版控只寫廠牌與產品名稱，種子目錄不收硬體型號），也可能洩露機構環境。現在清理，之後新增的內容就能以 commit 前檢查維持乾淨。

## What Changes

- 種子目錄移除硬體型號別名（PAN-OS、DSM 等產品目前帶有的機型寫法），只保留廠牌＋產品寫法；使用者實際用到的機型寫法改寫在本機 `catalog.d\site.toml`
- 測試、fixture 產生器與 fixture（含 xlsx）、TUI 快照、程式內的範例與註解中的真實型號，一律改為虛構寫法：
  - 資產編號改用角色命名（例如 `FW-EDGE`、`SRV-01`）
  - 產品寫法改用「廠牌＋`XYZ-<數字>`」（例如「Palo Alto XYZ-100」），測試只用虛構型號
- OpenSpec 主 spec、已封存 change、`openspec/config.yaml` 中的型號改為虛構寫法或只寫產品名稱
- 本機敏感字詞清單（`data\sensitive-patterns.txt`）補入這些型號；完成後版控中的檔案以清單檢查應無命中
- 原定 ISMS 對齊系列順延為 16–20（16-isms-admin-fields … 20-isms-export）

## Capabilities

### New Capabilities

（無）

### Modified Capabilities

- `product-catalog`：「Alias resolution」「User catalog extensions」的情境改用虛構型號
- `asset-inventory`：「Asset and component model」「Editing components」的情境改用虛構資產編號；「ISMS asset ID」的範例改用虛構系統代碼（`SYS-HW-001`）
- `gui-console`：「GUI asset class and rating input」「GUI asset presentation」的 ISMS 編號範例改用虛構系統代碼

## Out of scope

- 改寫 git 歷史（舊 commit 仍保有原內容）
- 產品名稱（PAN-OS、iDRAC9、DiskStation Manager 等）與產品線名稱（PA-Series）：不是型號，保留
- 來自公開 CVE 記錄的 fixture 內容（CVE 原文提到的機型屬公開資料，與使用者環境無關）；但若與使用者環境的型號相同，仍改用其他記錄
- 使用者資料目錄中的資料庫與清冊（不在版控，內容不變）

## Impact

- `crates/core/catalog/seed.toml`：移除型號別名
- 測試與測試支援：`crates/core/tests`、`crates/core/src`（單元測試與註解）、`crates/core/examples/gen_inventory_fixtures.rs`、`crates/frontend`、`crates/gui/tests`、`crates/tui`（含 `tests/snapshots`）、`crates/cvewatch/tests`
- fixture：`tests/fixtures/matching/*.csv`、`tests/fixtures/ingest/README.md`、`tests/fixtures/inventory_*.xlsx`（由產生器重新產生）
- OpenSpec：`openspec/specs/product-catalog`、`openspec/specs/asset-inventory`、`openspec/changes/archive/*` 中提到型號的檔案、`openspec/config.yaml`
- 使用者升級影響：清冊若以機型寫法（例如原本靠種子別名辨識的防火牆型號）填產品，升級後會變成未辨識，需在 `catalog.d\site.toml` 補別名；使用說明與版本說明須提醒
