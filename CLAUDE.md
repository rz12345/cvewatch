# CLAUDE.md

- 本專案以 OpenSpec 規格驅動：規格在 `openspec/`，專案限制寫在 `openspec/config.yaml` 的 `context`，實作前務必讀過。
- 依 change 編號 01 → 07 依序實作；一次只處理一個 change，完成並 archive 後才開始下一個。
- 每完成一個任務群組：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test`，並勾選 tasks.md。
- 測試不得呼叫真實外部 API；使用 `tests/fixtures/` 與 wiremock。
- 秘密只能透過 Credential Manager（keyring）讀取，不得出現在設定檔、資料庫、日誌或測試 fixture。
- 與使用者對話使用繁體中文。
