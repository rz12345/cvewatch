---
name: "OPSX: Auto"
description: "自動完成一個 change：apply（決策依建議）→ code-review 一輪 → commit → archive（同步主 spec）→ commit"
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

## 3. Commit 實作

- 依 repo 慣例在目前分支 commit（不加 attribution 行）：
  `feat: <功能摘要>（<change 名稱>）`，內文以條列說明 migration、主要模組、CLI 與測試。
- commit 前確認 `git status` 中沒有暫存檔或秘密。

## 4. 封存（archive）

- 以 Skill 工具執行 `opsx:archive`，參數為 change 名稱；遇到同步提問時選「先同步再封存」。
- 封存後執行 `openspec validate --specs --strict` 與 `openspec validate --all --strict`，有問題就修到全部通過。
- commit：`docs: 封存 <change 名稱> 並同步主 spec`，內文列出新增或修改的主 spec。

## 停止條件

以下情況停止並回報，不要硬做：無法修好的建置或測試失敗、需要真實外部 API 或秘密才能繼續、
建議做法會刪除使用者資料或改寫既有 commit。

## 最後回報（繁體中文）

- 兩個 commit 的雜湊、封存位置、同步的主 spec
- review 修了什麼
- 依建議自行決定的事項（含理由），以及留下未處理的項目
- 不 push

## 5. 清除對話（/clear）

`/clear` 是 Claude Code 內建指令，無法由流程自行執行。兩個 commit 都完成且回報結束後，回報的最後一行固定為：

> 本 change 已完成並封存。請執行 `/clear` 清除對話，再以 `/opsx-auto` 開始下一個 change。

流程中途停止（見「停止條件」）時不提示 `/clear`，保留對話脈絡以便處理問題。
