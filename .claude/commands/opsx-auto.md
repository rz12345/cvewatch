---
name: "OPSX: Auto"
description: "自動完成一個 change：apply（決策依建議）→ code-review 一輪 → 架構變更判斷 → commit →（必要時）更新架構圖並 commit → archive（同步主 spec）→ commit"
category: "Workflow"
tags: ["workflow", "apply", "review", "archive"]
---

全自動完成 OpenSpec change `$ARGUMENTS`（未指定時依 CLAUDE.md 的編號順序取下一個進行中的 change，並宣告所選名稱）。
整個流程不向使用者提問：凡是需要調整或確認的地方，一律採用「建議」選項（標示 Recommended 者，沒有標示時選最保守、最不會漏報的做法），
並把決定記入該 change 的 `design.md`。最後回報時列出所有自行決定的事項。

## 1. 實作（apply）

- 以 Skill 工具執行 `opsx:apply`，參數為 change 名稱。
- skill 指示「暫停詢問」的情況（任務不明確、發現設計問題、超出 spec 的範圍）：不暫停、不呼叫 AskUserQuestion，
  依建議做法處理，同步更新 design.md／spec delta，然後繼續。
- 每完成一個任務群組：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test`，全部通過才勾選 tasks.md。
- 全部任務完成後執行 `openspec validate <change> --strict`，必須通過。

## 2. Code review（一輪）

執行以下步驟一次。review 沒有任何發現時直接進入第 3 節：

1. 以 Skill 工具執行 `code-review`。
2. 修正全部發現；需要取捨時依建議處理（不提問）。
3. 行為有變更時更新 design.md 與 spec delta，並補上對應測試。
4. `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`openspec validate <change> --strict` 全部通過。

修正後不再重跑 review；仍有未處理的發現時列入最後的回報。

## 3. 架構變更判斷（commit 前）

架構圖在 `docs/architecture/`：`architecture.json`（Archify 來源）與 `architecture.html`（產出），
範圍是整體系統架構。以 `git status --short` 與 `git diff HEAD` 檢查本 change 是否涉及下列任一項：

- **crate 結構與相依**：workspace `members` 增刪或改名、`crates/*/Cargo.toml` 中 workspace 內部的 `path =` 相依改變、
  新增或移除執行檔（`[[bin]]`、`src/bin/*.rs`）。
- **外部整合**：新增、移除或改換外部系統或本機系統服務的互動，例如新的 CVE 來源或 HTTP 端點
  （`ingest/endpoints.rs`、`config.rs` 的 sources）、通知管道（`notify/`）、Credential Manager、工作排程器、
  以子程序啟動其他執行檔；圖上既有外部節點的協定改變（例如 SMTP 改為其他寄信 API）也算。

core 內部模組、資料庫 schema、UI 畫面或欄位的變更不算架構變更。

把判斷結果與理由記入該 change 的 `design.md`（例如「架構圖：需更新，新增 crate X」或「架構圖：不需更新，只改 core 內部模組」），
這段紀錄跟著第 4 節的實作 commit 一起進版。

## 4. Commit 實作

- 依 repo 慣例在目前分支 commit（不加 attribution 行）：
  `feat: <功能摘要>（<change 名稱>）`，內文以條列說明 migration、主要模組、CLI 與測試。
- commit 前確認 `git status` 中沒有暫存檔或秘密。

## 5. 更新架構圖（僅在第 3 節判定需要時）

Archify 只驗證「已 commit」的原始碼引用，所以架構圖在實作 commit 之後更新，並以獨立 commit 進版。

1. 以 Skill 工具執行 `archify`，依其 repository-backed architecture 流程，**直接修改** `docs/architecture/architecture.json`（不另開 `.archify/` 資料夾）：
   - `meta.repository.revision` 改為實作 commit 的 `git rev-parse HEAD`。
   - 未受影響的節點保留原 `id`、位置與尺寸；只新增、刪除或調整受影響的節點、連線與 cards。
   - 修訂後重新核對**所有**節點 `sources` 的行號範圍，確認在新 revision 仍指向原本描述的程式碼。
2. 在 repo 根目錄執行：
   `node .claude/skills/archify/bin/archify.mjs finalize architecture docs/architecture/architecture.json docs/architecture/architecture.html --repo-root . --quality showcase --json`
   失敗時依 skill 的 repair 指引修正並重跑。超過 skill 的修正上限仍失敗時，還原 `docs/architecture/` 的變更、不 commit 架構圖，
   繼續第 6 節，並在最後回報中列為未處理項目。
3. commit：`docs: 更新架構圖（<change 名稱>）`，內文條列新增、刪除或調整的節點與連線。只 commit `architecture.json` 與
   `architecture.html`（finalize 的收據檔已列入 `.gitignore`）。

## 6. 封存（archive）

- 以 Skill 工具執行 `opsx:archive`，參數為 change 名稱；遇到同步提問時選「先同步再封存」。
- 封存後執行 `openspec validate --specs --strict` 與 `openspec validate --all --strict`，有問題就修到全部通過。
- commit：`docs: 封存 <change 名稱> 並同步主 spec`，內文列出新增或修改的主 spec。

## 停止條件

以下情況停止並回報，不要硬做：無法修好的建置或測試失敗、需要真實外部 API 或秘密才能繼續、
建議做法會刪除使用者資料或改寫既有 commit。

## 最後回報（繁體中文）

- 各 commit 的雜湊（實作、架構圖〔若有〕、封存）、封存位置、同步的主 spec
- 架構圖是否更新及理由（第 3 節的判斷）
- review 修了什麼
- 依建議自行決定的事項（含理由），以及留下未處理的項目
- 不 push

## 7. 清除對話（/clear）

`/clear` 是 Claude Code 內建指令，無法由流程自行執行。所有 commit 都完成且回報結束後，回報的最後一行固定為：

> 本 change 已完成並封存。請執行 `/clear` 清除對話，再以 `/opsx-auto` 開始下一個 change。

流程中途停止（見「停止條件」）時不提示 `/clear`，保留對話脈絡以便處理問題。
