# Tasks

## 1. 對應層

- [ ] 1.1 `vex::mapping`：cvewatch 狀態 ↔ CycloneDX `analysis`（state / justification / response / detail）雙向函式
- [ ] 1.2 bom-ref 產生與解析（`cvewatch:<asset_tag>:<key|purl>`）
- [ ] 1.3 測試：每一列對應表的單元測試，含反向的不完整情況

## 2. 匯出

- [ ] 2.1 CycloneDX 1.6 序列化（serde 結構，只含需要的欄位）：metadata、components（cpe 或 purl、asset_tag property）、vulnerabilities（ratings、affects、analysis、properties）
- [ ] 2.2 `cvewatch vex export <file> [--asset] [--status] [--priority] [--encrypt]`；未加密提示；`--encrypt` 以 age 口令加密
- [ ] 2.3 測試：以內嵌 CycloneDX 1.6 JSON schema 驗證輸出；golden file

## 3. 匯入

- [ ] 3.1 解析 CycloneDX 1.4–1.6 JSON VEX（容忍未知欄位）；`.age` 檔提示輸入口令
- [ ] 3.2 元件對應（bom-ref → purl → cpe → 名稱 + 版本經目錄），記錄對應方式
- [ ] 3.3 衝突判斷（本機最後使用者事件 vs `lastUpdated` / `metadata.timestamp`）與 `--force`
- [ ] 3.4 套用（呼叫 04 triage 函式，actor = import）、`--dry-run` 四類報告
- [ ] 3.5 測試：往返無損；其他工具樣本（Dependency-Track、cve-bin-tool 產出的 VEX fixture）；未對應不新建；本機較新不覆蓋；匯入後 KEV 重新開啟

## 4. TUI

- [ ] 4.1 Findings 畫面 `E`：路徑對話框（預設文件資料夾）、依目前篩選匯出、狀態列提示
- [ ] 4.2 測試：TestBackend 快照
