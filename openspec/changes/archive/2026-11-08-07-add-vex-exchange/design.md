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

## 實作時自行決定的事項（/opsx-auto，依建議做法）

1. **事件操作者 import 與來源檔名、來源時間**：migration `0006_vex` 以重建資料表方式放寬 `finding_event.actor`（新增 `import`），並新增 `import_file`（來源檔名）與 `decided_at`（來源檔中的判定時間：VEX `lastUpdated`，缺少時為 `metadata.timestamp`）兩欄，僅 import 事件有值。匯入事件備註為「<檔名>：<detail>」，匯出時依 `import_file` 去掉前綴還原 detail（code review 第 2 輪：bom-ref 解析時 `:pkg:` 之後必須是合法 purl、產品 key 必須符合目錄 key 寫法，資產編號含 `:pkg:` 也能正確還原），確保匯出 → 匯入 → 再匯出的 detail 不變（往返無損）。檔名只存 basename。
2. **detail 與 lastUpdated 的來源**：detail 取最後一筆人為判定（使用者或匯入）事件的備註；其後若系統又改變過狀態（例如列入 KEV 重新開啟）則不輸出，避免舊備註描述新狀態。`lastUpdated` 依事件順序取最後一筆人為判定或狀態變化事件的時間（系統的註記類事件、狀態未變者不算），匯入事件以 `decided_at` 為準；`firstIssued` 為命中的 `first_seen`；時間以 RFC 3339 UTC 輸出並保留小數秒。（code review 第 1 輪修正：原本取所有事件的最大時間，會讓舊判斷看起來較新。）
3. **衝突判斷**：「本機最後使用者事件」包含使用者與先前的匯入（兩者都是人為判定），匯入的判定以來源檔中的時間（`decided_at`）計算，不以匯入當下的時間計算，避免舊判斷擋住之後較新的檔案。VEX 沒有 `lastUpdated` 也沒有 `metadata.timestamp`、而本機有人為判定時，保守地視為本機較新。時間相等時不算本機較新。本機時間依事件順序取自最後一筆人為判定起、最後一筆判定或系統狀態變化（例如列入 KEV 重新開啟）的時間：被 `--force` 匯入取代的較新使用者判定不再算數；判定後系統又改變狀態時以該時間為準，舊檔不會再次忽略已因 KEV 重新開啟的命中。來源沒有時間的匯入判定視為時間未知（最舊），不擋住之後有時間的檔案。（code review 第 2 輪修正）來源時間晚於匯入當下超過 5 分鐘（時鐘偏差容許）時視為未知，避免錯誤的未來時間永遠勝過本機之後的判斷；記錄的來源時間亦不晚於匯入當下。（code review 第 3 輪）
3a. **in_triage 不覆蓋本機狀態**：VEX 的 `in_triage` 表示尚未判定；本機狀態不是未處理時（不論是人為判定或系統自動處理）不套用，列為本機較新，`--force` 時才覆蓋。避免匯入同事剛同步、從未處理過的完整匯出檔時，把本機已處理的命中全部重新開啟。（code review 第 1 輪新增）
4. **與本機相同的項目**：狀態、原因、理由都相同且 detail 沒有新內容時略過、不寫入事件，報告另列「與本機相同」（四類之外的資訊）；避免重新匯入自己的匯出檔時產生大量重複事件。
5. **同一命中在檔案中有不同判斷**：不猜測，列為格式不完整、不套用；判斷相同的重複項目只套用一次（時間取最新）。
6. **cvewatch bom-ref 找不到元件時不改用其他方式**：資產編號完全相同者優先，沒有時才不分大小寫，且只在恰好對應一個資產時採用（資料庫的資產編號區分大小寫）。cvewatch bom-ref 指向特定資產，若改用 CPE／名稱對應會把某台設備的判斷套到其他資產，因此列為未對應。bom-ref 對應含已退役資產與已移除元件（命中保留）；purl、CPE、名稱 + 版本只對應使用中元件，且必須有版本並與本機版本相同（不分大小寫），可能對應到多個資產。
7. **purl 比對**：類型、命名空間、名稱相同，版本相同（epoch qualifier 併入版本，與套件清單匯入相同），兩邊都有的 qualifier（如 `distro`）必須相同；只有一邊有的（如 `arch`）忽略。
8. **affects 參照**：先找文件中的元件（含巢狀與 `metadata.component`）；bom-link（`urn:cdx:<serial>/<version>#<bom-ref>`）取片段再找；都沒有時把參照本身當成 bom-ref，形如 purl／CPE 時一併作為識別。
9. **支援版本**：只接受 `specVersion` 1.4–1.6，其他版本明確拒絕（不以未知格式猜測）；檔案上限 64 MB；未知欄位與未知 response 值忽略，未知 state／justification 列為格式不完整。
9a. **dry-run 不取得寫入鎖**：以 deferred 交易讀取，避免預覽時擋住排程 sync；實際匯入以 IMMEDIATE 交易規劃與套用。
10. **匯出不覆蓋既有檔案**：CLI 與 TUI 皆以 create-new 寫入，檔案已存在時報錯；CLI 在要求口令前先檢查。寫入失敗時移除不完整的檔案。
11. **加密**（code review 第 3 輪：匯入也辨識 ASCII armor 格式 `age -a`）：`--encrypt` 以 age scrypt 口令加密，口令輸入兩次確認，輸出 `<檔名>.age`（已是 `.age` 時沿用），不產生明文檔。匯入依檔頭 `age-encryption.org/v1` 判斷是否需要口令（不只看副檔名）。TUI 匯出不提供加密，狀態列提示「此檔未加密」。
11a. **匯出順序**：CLI 依篩選條件以輕量查詢取得命中 id，依命中編號排序（VEX 項目順序沒有意義，不需計算風險排序）；TUI 依畫面順序。
12. **CLI 篩選結果為空**：仍輸出合法的空 VEX（0 筆），方便腳本處理；TUI 沒有可見命中時不開對話框，只提示。
13. **TUI 匯出範圍**：Findings 目前可見的列（篩選面板 + `/` 搜尋），不看多選；預設檔名依顯示時區，格式維持 `cvewatch-vex-YYYYMMDD-HHMM.json`；同一分鐘內已有同名檔案時改用 `-2`、`-3`…（code review 第 2 輪）。使用者輸入的路徑已存在時不覆蓋、報錯。說明畫面合併 `o`／`y` 兩列以容納 `E`，避免覆蓋分頁列。
13a. **只更新 detail 時保留等待區標記**：判定（狀態、原因、理由）不變、只更新 detail 的匯入保留命中的「等待區判定不相關」標記（`dismissed`），資料確定受影響時仍會自動重新開啟；判定改變時與手動變更相同，清除標記。（code review 第 3 輪）
14. **Redmine 註記**：匯入實際變更狀態／原因／理由的命中與 `finding set` 相同，在同一交易排入 Redmine 註記（已有議題且 `add_note_on_triage` 開啟時）；只更新 detail 的不通知；同一次匯入中同一 CVE 只排入一則註記（各資產各一行，含各自的 detail），與 `finding set` 一致；排入失敗整批回復。
15. **時間軸顯示**：import 事件在 CLI 與 TUI 時間軸顯示為「匯入」。
16. **其他工具樣本**：`tests/fixtures/vex/dependency-track.json` 與 `cve-bin-tool.json` 依兩個工具公開的 CycloneDX VEX 輸出結構手工製作（DT：UUID bom-ref、purl 含 qualifier、無 `lastUpdated`、legacy `tools` 陣列；cve-bin-tool／lib4vex：bom-link affects、CPE 與名稱識別），不是實際執行工具擷取的檔案。
17. **schema 驗證**：測試內嵌 CycloneDX 1.6 官方 schema（`bom-1.6`、`spdx`、`jsf-0.82`，取自 CycloneDX/specification 1.6 tag），以 `jsonschema`（dev-dependency，關閉 HTTP 解析）驗證；golden file 以 `UPDATE_GOLDEN=1` 重新產生。
17a. **匯出效能與一致性**：CVSS 分數以一次查詢批次載入，產品 CPE 依產品快取，命中查詢使用 prepared statement 快取；匯出在單一讀取交易中進行，排程 sync 同時寫入時狀態與時間軸不互相矛盾。匯入比對 purl 時本機 purl 只解析一次，對應方式依序嘗試、前一種有結果就不計算後面的；purl 版本與 qualifier 規則與套件清單匯入共用同一組函式。（code review 第 2、3 輪）
17b. **時間軸事件的操作者型別化**：`record_event` 只接受系統或使用者（`Actor`）；import 事件只能經 `apply_decision` 寫入並必帶來源檔名，與資料庫 CHECK 條件一致。（code review 第 3 輪）
17c. **TUI 鍵位提示**：Findings 提示列縮短為「… r/i/u 狀態  o 連結  E 匯出」，80 欄終端機也看得到 `E`。（code review 第 3 輪）
