# ingest fixtures

2026-11-07 以實際請求錄製（03 任務 1.2），再精簡內容（`references` 只留前 2 筆、`versions` 只留前 6 筆、移除 credits 等）。
測試只讀這些檔案，不呼叫外部 API。

| 路徑 | 來源 | 用途 |
|---|---|---|
| `cvelist/records/CVE-2024-3400.json` | cvelistV5 raw，dataVersion 5.1 | PAN-OS、CNA CVSS 3.1、CISA-ADP SSVC |
| `cvelist/records/CVE-2025-0108.json` | cvelistV5 raw，5.2 | PAN-OS、CNA CVSS 4.0 |
| `cvelist/records/CVE-2025-7735.json` | cvelistV5 raw，5.1 | 指派單位 `twcert`（TWCERT/CC） |
| `cvelist/records/CVE-2019-7907.json` | 2026-11-06 end_of_day delta | REJECTED |
| `cvelist/records/CVE-2020-9709.json` | 同上 | 無任何 CVSS |
| `cvelist/records/CVE-2026-108949.json` | 同上，5.2 | `affected[].packageURL`（maven） |
| `cvelist/records/CVE-2026-103479.json` | 同上 | WordPress 外掛（Wordfence），與目錄無關 |
| `cvelist/records/CVE-2099-50001.json` | **合成** | dataVersion 5.0 形狀（repo 已無 5.0 紀錄）、VMware ESXi |
| `cvelist/records/CVE-2099-50002.json` | **合成** | affected 為 n/a、描述含 `Proxmox Virtual Environment`（僅關鍵字相符） |
| `cvelist/releases.json` | GitHub Releases API（前 8 筆） | asset 選擇：baseline、每小時 delta、end_of_day |
| `nvd/page1.json`、`page2.json` | NVD API 2.0，`virtualMatchString=cpe:2.3:o:paloaltonetworks:pan-os` | 分頁（`totalResults` 改為 3） |
| `kev/kev.json` | CISA KEV（3 筆） | KEV 更新 |
| `epss/epss.json` | FIRST EPSS | 分數與百分位 |
| `osv/querybatch.json`、`osv/DEBIAN-CVE-2023-5678.json` | OSV.dev，`openssl` / `Debian:12` | 套件查詢；CVE 對應在 `upstream` |
| `csaf/paloalto/rss.xml` | Palo Alto RSS（前 2 筆 item 與 CVE-2026-0322） | rss 探索 |
| `csaf/paloalto/CVE-2026-0322.json` | `https://security.paloaltonetworks.com/csaf/CVE-2026-0322` | CSAF 2.1、`vers:generic/PAN-OS>=…` 寫法 |
| `csaf/oasis/bsi-2022-0001.json` | OASIS CSAF 2.0 規格範例 | 一般 CSAF 2.0 文件 |
| `csaf/provider/*` | **合成**（格式參考 Intevation ROLIE feed 與 Red Hat `changes.csv`） | provider_metadata 探索、雜湊、TLP、版本取代；`{{BASE}}` 由測試換成 wiremock 位址 |

cvelist 的 zip（含巢狀 baseline）由測試以 `zip::ZipWriter::new_stream` 即時產生，外層 entry 帶 data descriptor，與實際 baseline 相同。

## matching fixtures（04 任務 6.2）

`tests/fixtures/matching/`：端對端比對測試（`crates/core/tests/matching_e2e.rs`）。

| 路徑 | 來源 | 用途 |
|---|---|---|
| `inventory.csv`、`components.csv` | **合成** | FW-01（PA-460 / PAN-OS）、R6515-01（ESXi build + iDRAC9）、PVE-01（Proxmox VE）、NAS-01（Synology DSM） |
| `inventory_upgraded.csv` | **合成** | NAS-01 升版到已修正版本（自動轉為已處理） |
| `cves/CVE-2099-6000{1,4,5}.json` | **合成** CVE 5.1 | DSM 版本區間、CNA changes 與 NVD 矛盾、只列產品沒有版本 |
| `nvd.json` | **合成** NVD API 2.0 | iDRAC（AND 平台條件）、Proxmox 區間、PAN-OS 未區分熱修 |
| `expected.txt` | golden file | 預期命中、矛盾與等待區；`UPDATE_GOLDEN=1` 重新產生後須人工檢查差異 |

另使用上方 ingest fixture 的實際紀錄（CVE-2024-3400、CVE-2025-0108、CVE-2026-0322 CSAF 等）。
