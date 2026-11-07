# Proposal

## Why

比對品質完全取決於資產清冊。目前清冊是手動維護的 Excel/CSV，欄位寫法不一致（例如「PA-460」「PAN-OS 11.1.4-h7」「Palo Alto」混用），無法直接對應到 CVE 資料裡的 vendor/product/CPE。需要一個「產品目錄」把各種寫法正規化，並讓 Excel 匯入可以重複執行、看得到差異。

## What Changes

- 資產模型分兩層：**Asset**（一台實體/虛擬設備）→ **Component**（該設備上要追蹤版本的軟體/韌體，例如 R6515 上的 iDRAC、BIOS、ESXi）
- **產品目錄（product catalog）**：每個產品定義正規名稱、別名、CPE `vendor:product`、CVE.org CNA 的 vendor/product 寫法、版本格式（version scheme）、原廠公告來源；內建種子資料涵蓋 PAN-OS、ESXi、vCenter、Proxmox VE、iDRAC、Dell PowerEdge BIOS、Synology DSM、Debian/Ubuntu/RHEL/Windows Server
- Excel（.xlsx）與 CSV 匯入：欄位對應、`--dry-run` 差異預覽、以 `asset_tag` upsert、`--prune` 才會把清冊中消失的資產標為退役
- SBOM / 套件清單匯入：CycloneDX JSON、SPDX JSON（syft 產出）、`dpkg-query -W` 與 `rpm -qa` 文字輸出，轉成帶 purl 的 package component
- 資產屬性：曝露度（internet / dmz / internal / mgmt）與重要性（1–3），供 04 風險排序使用
- CLI：`cvewatch asset import|list|show|catalog`，以及匯出空白範本 `cvewatch asset template`

## Capabilities

### New Capabilities

- `asset-inventory`: 資產與元件的資料模型、Excel/CSV 匯入、退役與歷史
- `product-catalog`: 產品正規化、別名對應、CPE/CNA 識別、版本格式宣告
- `package-inventory`: 主機 OS 套件清單（SBOM）匯入與更新

### Modified Capabilities

（無）

## Out of scope

- 主動掃描或 SSH 登入主機收集資料
- 從 NetBox/Snipe-IT 等 CMDB 同步
- Windows 個別 KB 修補層級追蹤（Microsoft 修補以 OS build 版本為準，MSRC 對應留待未來）

## Impact

- `crates/core/src/assets/`、`crates/core/src/catalog/`；migration `0002_assets.sql`
- 新相依：calamine（讀 xlsx）、csv、rust_xlsxwriter（匯出範本）、packageurl
- 種子目錄檔 `crates/core/catalog/seed.toml`（內嵌於執行檔，使用者可用 `<data_dir>\catalog.d\*.toml` 覆寫/擴充）
