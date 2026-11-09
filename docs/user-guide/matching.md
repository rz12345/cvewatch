# 比對、命中與等待區

```powershell
cvewatch match                                   # 全量重算（sync 結束、asset import / import-isms / import-packages 後會自動執行）
cvewatch finding list --status open --priority P1,P2   # 依風險排序；? = probable、! = 已處理但仍在受影響範圍
cvewatch finding show 42                         # 各來源證據、修正版本、來源矛盾與時間軸
cvewatch finding set 42 ignored --reason not_affected --justification requires_configuration --note "未啟用 GlobalProtect"
cvewatch finding conflicts                       # 最終判定不受影響、但有來源判定受影響的組合
cvewatch pending list --stale                    # 等待區（KEV 置頂）；--stale 只列逾期
cvewatch pending resolve CVE-2099-0001 --apply PVE-01   # 或 --dismiss；資產有多個相關元件時用 PVE-01:proxmox-ve
```

- 版本待補（版本空白）的元件不參與比對，不產生任何命中；既有命中保留但不再更新，見 [資產清冊](assets.md) 的「版本待補」
- 判定優先序：原廠 CSAF 公告 > CNA affected > NVD CPE > CISA ADP；套件元件以 OSV 判定。版本以產品目錄的 `version_scheme` 比較，無法解析一律視為未知（建立 probable 命中，不判定為不受影響）
- 風險等級門檻在設定檔 `[scoring]`、等待區逾期天數在 `[pending] stale_days`；修改後下次比對重算
- 清冊只填 ESXi build 編號時，需要種子目錄或 `catalog.d` 的 `[[esxi_build]]` 對照表才能與 Update 名稱比較；對照不到時判定為未知

## 風險等級（P1–P4）

每筆命中依下表由上而下判定基本等級，符合第一條即停（括號內為預設門檻）：

| 等級 | 條件 |
|---|---|
| P1 | CVE 列入 CISA KEV（已知遭利用） |
| P2 | EPSS ≥ `epss_high`（0.10）或 CVSS ≥ `cvss_high`（9.0） |
| P3 | EPSS ≥ `epss_medium`（0.01）或 CVSS ≥ `cvss_medium`（7.0） |
| P4 | 其他，含沒有 EPSS 與 CVSS 的 CVE |

- 資產曝露度在 `exposure_bump`（預設 `internet`、`dmz`）內時再上調一級，例如對外防火牆的 P3 命中成為 P2；P1 維持不變
- 同一等級內依 KEV 加入日期（新到舊）、EPSS、CVSS、資產價值 MAX(C, I, A)（皆由高到低）排序
- 門檻與上調清單在設定檔 `[scoring]` 調整，下次 `cvewatch match` 重算全部等級
- 緊急通知（立即寄信、Redmine 開案）另以「KEV 或 CVSS ≥ `[notify] urgent_cvss`」判定，與 P1–P4 無關
- 正式規格見 [openspec/specs/risk-scoring/spec.md](../../openspec/specs/risk-scoring/spec.md)
