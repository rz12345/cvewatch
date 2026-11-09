# Tasks

## 1. 準備

- [x] 1.1 在本機建立 `data\scrub-map.tsv`（真實型號 → 虛構寫法，依 design D1），並把真實型號補進 `data\sensitive-patterns.txt`；完成條件：`git status` 不出現這兩個檔，`git check-ignore` 涵蓋它們
- [x] 1.2 以本機清單掃描 `git ls-files` 全部內容（xlsx 解壓後比對），列出命中的檔案與次數作為基準；完成條件：基準清單存在 scratch 目錄，涵蓋 design Context 所列四類

## 2. 種子目錄

- [x] 2.1 移除 `seed.toml` 中的機型別名（design D3），並把使用者實際用到的機型寫法寫入本機 `catalog.d\site.toml`；完成條件：`cvewatch asset catalog show paloalto-panos` 不含機型別名，本機清冊重新匯入的預覽為「新增 0、更新 0」
- [x] 2.2 測試：`crates/core/tests/catalog.rs` 與 `crates/core/src/catalog/normalize.rs` 的測試改用 spec 的「Palo Alto PAN-OS」與虛構型號，以 `catalog.d` 加別名的情境改用 `xyz-200 esxi`；`cargo test -p cvewatch-core` 通過

## 3. 測試、fixture 與快照

- [x] 3.1 依 `scrub-map.tsv` 替換 `crates/core`（tests、src 中的測試與註解、examples）、`crates/cvewatch/tests` 與 `tests/fixtures` 的文字檔（design D2 的完整詞比對與排除清單）；完成條件：逐檔檢視 diff 只有字串替換
- [x] 3.2 重新產生 `tests/fixtures/inventory_*.xlsx`（design D4）；完成條件：解壓比對無命中，`cargo test -p cvewatch-core` 通過
- [x] 3.3 替換 `crates/frontend`、`crates/gui`、`crates/tui` 的測試與測試支援；字寬相關測試改用長度相同的虛構字串；完成條件：逐檔檢視 diff 只有字串替換
- [x] 3.4 以 `cargo insta test` 重新產生 TUI 快照並逐一確認差異只有編號與對齊空白後接受；完成條件：`crates/tui/tests/snapshots` 無命中
- [x] 3.5 測試：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 全部通過，且測試數量與替換前相同

## 4. OpenSpec 與文件

- [x] 4.1 替換 `openspec/changes/archive/*` 中的型號（只改字串，design D5），`openspec/config.yaml` 的資產範圍改為只寫產品名稱；完成條件：`openspec validate --all --strict` 通過
- [x] 4.2 在 `docs/user-guide/assets.md` 的升級注意事項說明：種子目錄不再收機型寫法，原本以機型寫法填寫的清冊需在 `catalog.d` 加別名；完成條件：文件只用虛構型號示範
- [x] 4.3 驗證：以本機清單重新掃描 `git ls-files` 全部內容（含 xlsx）沒有命中；暫存全部變更後 `scripts/check-sensitive.ps1` 無命中

## Workflow follow-up

- 依 `/opsx-auto` 流程：code-review 一輪、判斷架構變更（預期無）、commit 前執行 `scripts/check-sensitive.ps1`、archive（主 spec 同步本 change 的 MODIFIED 情境）
- archive 後 propose 16-isms-admin-fields
