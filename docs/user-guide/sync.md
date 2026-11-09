# 同步 CVE（`cvewatch sync`）

```powershell
cvewatch sync                      # 依序：cvelist → nvd → csaf:* → kev → epss → osv → eol
cvewatch sync --source kev         # 只更新指定來源（可重複；csaf 或 csaf:<id>）
cvewatch sync --source eol         # 只更新 endoflife.date 的軟體版本分支 EoL（不影響比對）
cvewatch sync --rescan             # 目錄（catalog.d）變更後，以最新 baseline 重新篩選
cvewatch sync --history            # 最近 20 次各來源的執行結果
cvewatch cve show CVE-2024-3400    # 整合後的 CVE 資料；本機沒有時加 --fetch 直接向 CVE.org 取得
cvewatch cve search proxmox        # 在 baseline 索引中搜尋（含未存入本機者），找出漏掉的別名
```

- **首次同步會下載 cvelistV5 baseline（約 650 MB）**，串流解壓不落地，依網路速度約需數分鐘到十數分鐘；之後每次只下載當日 delta（數 KB～數 MB）。超過 25 天沒同步會重新下載 baseline
- 只保留與產品目錄或資產套件相關的 CVE；另在 `<data_dir>\cache\cvelist\search-index.jsonl.zst` 保留全部 CVE 的精簡索引供 `cve search` 使用
- 任一來源失敗時其他來源照常更新，失敗來源的資料與游標不變，結束碼為 4
- NVD API key（選用，速率由 5 次/30 秒提高到 50 次/30 秒）：`cvewatch config set-secret nvd_api_key`
- `eol` 來源取得 endoflife.date 的版本分支 EoL，只查詢使用中資產的清冊元件所屬、且目錄宣告了代稱的產品；資料變更不觸發重新比對，設定檔 `[sources] eol_enabled = false` 可停用，見[資產生命週期](lifecycle.md)
- 新增提供 CSAF 2.0 公告的原廠：在設定檔加一筆 `[[sources.csaf]]`（見範例設定檔），不需更新程式
- sync 結束後若資料有變動會自動比對，見[比對、命中與等待區](matching.md)；定期執行見[首次設定與通知](setup.md)的 `cvewatch schedule install`
