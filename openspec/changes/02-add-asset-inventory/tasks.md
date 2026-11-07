# Tasks

## 1. 資料表與模型

- [ ] 1.1 `0002_assets.sql`：asset、component、component_history、catalog_product、catalog_alias、catalog_cpe、catalog_cna、catalog_csaf — 完成條件：migration 測試通過
- [ ] 1.2 Rust 模型與 repository（`AssetRepo`、`ComponentRepo`），含 upsert 與 history 寫入
- [ ] 1.3 測試：upsert 冪等、版本變更寫 history、retired 資產不出現在 `active_components()`

## 2. 產品目錄

- [ ] 2.1 定義目錄 TOML 格式與 `seed.toml`（PAN-OS、ESXi、vCenter、Proxmox VE、iDRAC9、Synology DSM、Debian、Ubuntu、RHEL、Windows Server）
- [ ] 2.2 名稱正規化函式（NFKC、小寫、去符號）與別名查詢
- [ ] 2.3 載入順序：種子 → `catalog.d\*.toml`；同 key 覆寫、別名衝突時報錯
- [ ] 2.4 `VersionScheme` trait 的空殼與 `parse()`（只做可否解析；比較邏輯在 04）
- [ ] 2.5 未辨識名稱的相似建議（strsim Jaro-Winkler ≥ 0.85，最多 3 個）
- [ ] 2.6 `cvewatch asset catalog [list|show <key>|--unresolved]`
- [ ] 2.7 測試：別名命中、衝突、覆寫、相似建議、各 scheme 的可解析/不可解析樣本

## 3. Excel / CSV 匯入

- [ ] 3.1 calamine 讀取 `assets` / `components` 工作表；csv 讀取兩個檔
- [ ] 3.2 `[import.columns]` 欄名對應
- [ ] 3.3 列驗證（必填、exposure、criticality 範圍），錯誤彙整列號
- [ ] 3.4 差異計算（新增/更新/版本變更/未辨識/消失），`--dry-run` 輸出表格
- [ ] 3.5 單一交易寫入；`--prune` 退役處理
- [ ] 3.6 `cvewatch asset template <path>`（rust_xlsxwriter）
- [ ] 3.7 `cvewatch asset list [--category] [--unresolved]`、`asset show <tag>`
- [ ] 3.8 測試：以 `tests/fixtures/inventory_*.xlsx` 驗證冪等、中文欄名、缺欄中止、prune

## 4. 套件清單匯入

- [ ] 4.1 格式自動判斷（JSON 的 `bomFormat` / `spdxVersion`；文字的 tab 分隔欄數）
- [ ] 4.2 CycloneDX、SPDX parser（取 purl）
- [ ] 4.3 dpkg / rpm parser，依 `--distro` 產生 purl
- [ ] 4.4 整批取代與 history
- [ ] 4.5 測試：四種格式 fixture、缺 distro、未知資產
