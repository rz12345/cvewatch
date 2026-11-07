# Spec Delta

## Purpose

以 CycloneDX VEX 格式匯出與匯入命中的處理判斷，讓「是否受影響、為什麼」可以在工具之間移轉、交接與稽核，不鎖在 cvewatch 資料庫中。

## ADDED Requirements

### Requirement: Export findings as CycloneDX VEX
`cvewatch vex export <file>` SHALL 產出符合 CycloneDX 1.6 JSON schema 的 VEX 文件，每筆命中輸出一個漏洞項目並引用對應元件，且 SHALL 可依資產、狀態與等級篩選。

#### Scenario: Exporting one asset
- **WHEN** 使用者執行 `cvewatch vex export fw01.json --asset FW-01`
- **THEN** 檔案通過 CycloneDX 1.6 schema 驗證，只包含 FW-01 的元件與其命中

#### Scenario: Package component
- **WHEN** 匯出的命中屬於套件元件
- **THEN** 該元件以 purl 識別

### Requirement: Status mapping on export
匯出時系統 SHALL 依下列規則對應：未處理為 in_triage；已處理為 resolved 並附 update 回應；忽略且不受影響為 not_affected 並附理由；忽略且接受風險為 exploitable 並附 will_not_fix 回應；忽略且誤判為 false_positive。備註 SHALL 輸出為 detail。

#### Scenario: Risk accepted finding
- **WHEN** 匯出一筆原因為 risk_accepted 的忽略命中
- **THEN** 其 analysis 的 state 為 exploitable，response 含 will_not_fix

#### Scenario: Not affected finding
- **WHEN** 匯出一筆原因為 not_affected、理由為 requires_configuration、備註為「未啟用 GlobalProtect」的命中
- **THEN** analysis 的 state 為 not_affected，justification 為 requires_configuration，detail 為該備註

### Requirement: Lossless round trip
將 cvewatch 匯出的 VEX 匯入同一資料庫時，系統 SHALL 還原每筆命中的狀態、忽略原因、理由與備註。

#### Scenario: Restore after reset
- **WHEN** 使用者匯出全部命中，將若干命中改回未處理後再匯入該檔並加 `--force`
- **THEN** 那些命中的狀態、原因、理由與備註與匯出時相同

### Requirement: Import with component matching
`cvewatch vex import <file>` SHALL 依 cvewatch bom-ref、purl、CPE、名稱加版本的順序對應元件，並以 CVE ID 對應命中；本機不存在的命中 MUST NOT 被新建，SHALL 列為未對應。

#### Scenario: VEX from another tool
- **WHEN** 匯入一份由其他工具產生、以 purl 識別套件的 VEX
- **THEN** 系統以 purl 對應到本機套件元件並套用狀態

#### Scenario: Unknown finding
- **WHEN** VEX 中某 CVE 在本機沒有對應的命中
- **THEN** 該項列為未對應，資料庫不新增命中

#### Scenario: cvewatch bom-ref for an unknown asset
- **WHEN** 匯入項目以 cvewatch 格式的 bom-ref 指向本機不存在的資產元件
- **THEN** 該項列為未對應，不改以 CPE 或名稱對應到其他資產

### Requirement: Import status mapping and validation
匯入時系統 SHALL 依匯出規則的反向對應狀態；not_affected 缺少 justification 或缺少 analysis 的項目 MUST NOT 被套用，SHALL 列為格式不完整。

#### Scenario: Not affected without justification
- **WHEN** VEX 某項 state 為 not_affected 但沒有 justification
- **THEN** 該項不被套用，報告中列為缺少理由

### Requirement: Newer local decisions win
本機命中最後一次使用者變更晚於 VEX 項目的更新時間時，匯入 SHALL 不套用該項並列為本機較新；只有使用 `--force` 時才覆蓋。先前匯入的判定 SHALL 以其來源檔中的更新時間參與比較。VEX 項目為 in_triage（尚未判定）而本機狀態不是未處理時，SHALL 同樣不套用並列為本機較新，除非使用 `--force`。

#### Scenario: Local change after export
- **WHEN** 使用者匯出後又在本機將某命中改為已處理，再匯入舊檔
- **THEN** 該命中維持已處理，報告列為本機較新

#### Scenario: KEV reopen after export
- **WHEN** 使用者匯出後，某忽略命中的 CVE 列入 KEV 而被自動重新開啟，再匯入匯出時的舊檔
- **THEN** 該命中維持未處理，報告列為本機較新

#### Scenario: Untriaged item from a colleague
- **WHEN** 同事在本機判定之後才同步、從未處理某命中，匯出的 VEX 中該項為 in_triage 且更新時間較晚
- **THEN** 匯入時該命中維持本機的判定，報告列為本機較新

#### Scenario: Older imported decision does not block newer file
- **WHEN** 使用者先匯入一份 10 月 1 日判定的檔案，再匯入另一份 10 月 15 日判定、內容不同的檔案
- **THEN** 第二份檔案的判定被套用

#### Scenario: Future-dated VEX
- **WHEN** 匯入的 VEX 項目更新時間晚於匯入當下（來源電腦時鐘錯誤），之後使用者又在本機變更該命中，再匯入同一檔案
- **THEN** 該命中維持使用者的變更，報告列為本機較新

#### Scenario: VEX without timestamps
- **WHEN** VEX 項目沒有 lastUpdated、文件也沒有 metadata.timestamp，而本機命中有使用者或匯入的判定
- **THEN** 該項不套用，列為本機較新

### Requirement: Import dry-run and audit trail
匯入 SHALL 支援 `--dry-run` 只報告將套用、未對應、本機較新、格式不完整的數量與明細；實際套用的每筆變更 SHALL 記錄操作者為 import 並附來源檔名的事件。判定與本機相同的項目 SHALL 略過、不寫入事件，並另列為與本機相同；同一命中在檔案中有不同判斷時 MUST NOT 套用，SHALL 列為格式不完整。

#### Scenario: Dry run
- **WHEN** 使用者執行 `cvewatch vex import team.json --dry-run`
- **THEN** 輸出四類計數與明細，資料庫不變

#### Scenario: Applied change is traceable
- **WHEN** 匯入將某命中改為 ignored
- **THEN** 時間軸新增一筆操作者為 import、備註含檔名的事件

#### Scenario: Re-importing own export
- **WHEN** 使用者匯入剛匯出、內容未改的檔案
- **THEN** 所有項目列為與本機相同，時間軸不新增事件

### Requirement: Automatic rules still apply after import
匯入的狀態 SHALL 與手動設定相同受 04 自動規則約束。

#### Scenario: Imported ignore then KEV
- **WHEN** 一筆經匯入設為 ignored 的命中，其 CVE 之後列入 KEV
- **THEN** 該命中被重新開啟

### Requirement: Export file protection
匯出完成時系統 SHALL 提示檔案未加密，並 SHALL 提供 `--encrypt` 以口令加密輸出檔。匯出 MUST NOT 覆蓋已存在的檔案。

#### Scenario: Encrypted export
- **WHEN** 使用者執行 `cvewatch vex export all.json --encrypt` 並輸入口令
- **THEN** 產生 `all.json.age`，不產生明文檔

#### Scenario: Existing output file
- **WHEN** 輸出路徑已有檔案
- **THEN** 匯出失敗並說明檔案已存在，原檔內容不變
