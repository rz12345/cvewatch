# Spec Delta

## Purpose

把緊急命中自動建立為 Redmine 議題，讓漏洞處理進入使用者既有的案件流程，並在後續變化時補充註記。

## ADDED Requirements

### Requirement: One issue per urgent CVE
緊急事件發生時，若該 CVE 尚無 Redmine 議題，系統 SHALL 在設定的專案建立一張議題，內容包含 CVE 摘要、CVSS、EPSS、KEV 與到期日、所有受影響資產與元件版本、判定依據、原廠修正版本與參考連結。

#### Scenario: Urgent PAN-OS CVE
- **WHEN** 一筆 KEV CVE 命中 `FW-01`，且該 CVE 尚無議題
- **THEN** Redmine 新增一張議題，主旨含等級、KEV 標記、CVE ID、產品與 `FW-01`

#### Scenario: Non-urgent finding
- **WHEN** 一筆 CVSS 7.5 且不在 KEV 的新命中產生
- **THEN** 不建立 Redmine 議題

### Requirement: Additional assets are appended
已有議題的 CVE 又影響新的資產時，系統 SHALL 在原議題新增註記列出新資產，MUST NOT 建立第二張議題。

#### Scenario: Second firewall affected later
- **WHEN** 已開案的 CVE 在之後的比對中也命中 `FW-02`
- **THEN** 原議題新增一筆註記說明 `FW-02` 受影響

### Requirement: Triage notes
設定啟用時，cvewatch 中命中狀態變更 SHALL 在對應議題加上註記（資產、新狀態、備註）；系統 MUST NOT 變更議題狀態。

#### Scenario: Marking resolved in cvewatch
- **WHEN** 使用者將 `FW-01` 的該命中設為 resolved 並備註「已升至 11.1.6」
- **THEN** 議題新增註記「FW-01：resolved（已升至 11.1.6）」，議題狀態不變

### Requirement: Redmine failures are retried
Redmine 暫時性錯誤 SHALL 進入送出佇列重送；驗證類錯誤（HTTP 422、401、403）SHALL 不重試並標為無法送出。

#### Scenario: Redmine down during sync
- **WHEN** 建立議題時 Redmine 回應 502
- **THEN** 請求保留於佇列，下次執行時重送，成功後記錄議題編號

#### Scenario: Invalid tracker
- **WHEN** Redmine 回應 422 指出 tracker 無效
- **THEN** 請求標為無法送出，`notify outbox` 顯示 Redmine 的錯誤訊息

### Requirement: HTTPS-only Redmine
Redmine 網址 SHALL 為 https 並驗證伺服器憑證（可透過設定的 CA 檔信任內部 CA）；設定檔未手動啟用 `allow_insecure_transport` 時，http 網址 SHALL 被拒絕。即使啟用例外，系統 MUST NOT 經 http 傳送 API key。

#### Scenario: http URL without exception
- **WHEN** Redmine 網址設為 `http://redmine.local`
- **THEN** 設定驗證失敗，提示必須使用 https

#### Scenario: http URL with exception enabled
- **WHEN** 例外開關已啟用且網址為 http
- **THEN** 系統拒絕送出任何帶 API key 的請求，並說明原因

### Requirement: Redmine configuration test
系統 SHALL 提供 `cvewatch notify test-redmine`，驗證 API key、專案與 tracker 可用，且不建立議題。

#### Scenario: Valid configuration
- **WHEN** 設定與 API key 皆正確
- **THEN** 指令回報可存取專案名稱與 tracker 名稱
