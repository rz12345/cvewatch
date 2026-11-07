# Proposal

## Why

處理狀態與「為什麼不受影響」的判斷是 cvewatch 裡最花人力的資料。如果只存在 cvewatch 的資料庫裡，日後換工具、交接給同事或要給稽核看時就帶不走。Dependency-Track 與 cve-bin-tool 都以 CycloneDX VEX（Vulnerability Exploitability eXchange，標準化的「是否受影響」聲明）交換處理結果；cvewatch 支援同一格式，判斷就能在工具之間移轉，也能匯入別人（或原廠）提供的 VEX。

## What Changes

- `cvewatch vex export <file.json>`：將命中與處理狀態匯出為 CycloneDX 1.6 JSON VEX，可依資產、狀態、等級篩選
- `cvewatch vex import <file.json> [--dry-run] [--force]`：匯入 CycloneDX VEX，依元件與 CVE 對應到本機命中並套用狀態；預設不覆蓋本機較新的決定
- 狀態對應（三態 + 忽略原因 ↔ VEX `analysis.state` / `justification` / `response`），忽略原因已由 04 定義
- TUI Findings 畫面新增「依目前篩選匯出 VEX」
- 匯出檔不含 cvewatch 內部 ID 以外的敏感設定（不含 SMTP/Redmine 資訊）

## Capabilities

### New Capabilities

- `vex-exchange`: CycloneDX VEX 的匯出、匯入、狀態對應與衝突處理

### Modified Capabilities

- `tui-console`: 新增依目前篩選匯出 VEX 的操作

## Out of scope

- OpenVEX、CSAF VEX、SPDX VEX 格式（之後可在同一對應層上加）
- 匯出完整 SBOM（只輸出 VEX 所需的元件）
- 匯入時新建本機不存在的命中（只報告為未對應）

## Impact

- `crates/core/src/vex/{export.rs,import.rs,mapping.rs}`、CLI `vex` 子命令、`crates/tui` Findings 畫面
- 測試時內嵌 CycloneDX 1.6 JSON schema（jsonschema crate，dev-dependency）驗證輸出
- `finding_event.actor` 新增值 `import`
