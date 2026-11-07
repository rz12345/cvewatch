# Design

## Context

04 已讓每筆命中具備：狀態（open / resolved / ignored）、忽略原因（not_affected / risk_accepted / false_positive）、not_affected 時的 CycloneDX justification、備註與時間軸。本 change 只做格式轉換與匯入規則。

## Goals / Non-Goals

**Goals:**
- 匯出 → 匯入同一個 cvewatch 資料庫，狀態、原因、理由、備註完全還原（往返無損）
- 產出的檔案能被 Dependency-Track、cve-bin-tool 等支援 CycloneDX VEX 的工具讀取

**Non-Goals:**
- 雙向同步

## Decisions

### 文件結構（CycloneDX 1.6 JSON）

```json
{
  "bomFormat": "CycloneDX", "specVersion": "1.6",
  "serialNumber": "urn:uuid:…", "version": 1,
  "metadata": { "timestamp": "…", "tools": { "components": [{ "type": "application", "name": "cvewatch", "version": "x.y.z" }] } },
  "components": [
    { "bom-ref": "cvewatch:FW-01:paloalto-panos", "type": "operating-system",
      "name": "PAN-OS", "version": "11.1.4-h7", "supplier": { "name": "Palo Alto Networks" },
      "cpe": "cpe:2.3:o:paloaltonetworks:pan-os:11.1.4-h7:*:*:*:*:*:*:*",
      "properties": [{ "name": "cvewatch:asset_tag", "value": "FW-01" }] }
  ],
  "vulnerabilities": [
    { "id": "CVE-2026-0001", "source": { "name": "CVE", "url": "https://www.cve.org/CVERecord?id=CVE-2026-0001" },
      "ratings": [{ "score": 9.8, "method": "CVSSv31", "vector": "…", "source": { "name": "CNA" } }],
      "affects": [{ "ref": "cvewatch:FW-01:paloalto-panos" }],
      "analysis": { "state": "not_affected", "justification": "requires_configuration",
                    "detail": "未啟用 GlobalProtect", "firstIssued": "…", "lastUpdated": "…" },
      "properties": [{ "name": "cvewatch:priority", "value": "P2" }, { "name": "cvewatch:confidence", "value": "confirmed" }] }
  ]
}
```

- 每筆命中（元件 × CVE）輸出一個 vulnerability 物件（同一 CVE 在不同元件可能有不同判斷，CycloneDX 允許相同 `id` 出現多次）
- 套件元件以 `purl` 取代 `cpe`，`type = library`
- `bom-ref` 格式：`cvewatch:<asset_tag>:<catalog key 或 purl>`，匯入時優先用它對應

### 狀態對應

| cvewatch | VEX analysis |
|---|---|
| open | `state: in_triage` |
| resolved | `state: resolved`，`response: [update]` |
| ignored / not_affected | `state: not_affected`，`justification: <值>` |
| ignored / risk_accepted | `state: exploitable`，`response: [will_not_fix]` |
| ignored / false_positive | `state: false_positive` |

反向（匯入）：

| VEX analysis | cvewatch |
|---|---|
| `in_triage`；`exploitable` 且 response 不含 will_not_fix / can_not_fix | open |
| `resolved`、`resolved_with_pedigree` | resolved |
| `not_affected` + justification | ignored / not_affected |
| `not_affected` 但缺 justification | 不套用，報告為「缺少理由」 |
| `false_positive` | ignored / false_positive |
| `exploitable` 且 response 含 `will_not_fix` 或 `can_not_fix` | ignored / risk_accepted |
| 無 `analysis` | 不套用 |

`detail` ↔ 備註；`lastUpdated` 用於衝突判斷。

### 匯入對應與衝突

1. 元件對應順序：`bom-ref`（cvewatch 格式）→ `purl` → `cpe`（vendor:product + version）→ `name` + `version` 經產品目錄別名
2. 命中 = (元件, `id` 為 CVE 的漏洞)；`id` 非 CVE 時檢查 `references` 中的 CVE 別名
3. 本機沒有該命中 → 列為「未對應」，不新建
4. 衝突：本機命中最後一筆**使用者**事件時間晚於 VEX `lastUpdated`（缺少時以文件 `metadata.timestamp`）→ 不套用，列為「本機較新」；`--force` 才覆蓋
5. 套用時寫入 `finding_event(actor = 'import', note = "<檔名>：<detail>")`
6. 匯入不影響 04 的自動規則：例如匯入為 ignored 的命中，之後 CVE 列入 KEV 仍會重新開啟

`--dry-run` 輸出四類計數與明細：將套用、未對應、本機較新、格式不完整。

### TUI

Findings 畫面按 `E`：以目前篩選條件匯出，預設檔名 `cvewatch-vex-YYYYMMDD-HHMM.json` 存在使用者「文件」資料夾，可在對話框修改路徑。

### 敏感資訊

匯出檔含資產清單與漏洞處理狀態，與資料庫同等敏感：匯出完成時提示「此檔未加密」，並提供 `--encrypt`，以 age 格式口令加密（`.json.age`）。

## Risks / Trade-offs

- 其他工具對 CycloneDX VEX 欄位的支援程度不一（例如部分工具忽略 `properties`）→ 只依賴標準欄位做狀態還原，`properties` 僅為輔助資訊
- 名稱 + 版本的對應可能誤配 → 僅在 bom-ref、purl、cpe 都沒有時才使用，並在 dry-run 中標示對應方式
