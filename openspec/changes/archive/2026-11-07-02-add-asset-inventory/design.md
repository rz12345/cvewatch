# Design

## Context

使用者的 Excel 清冊是唯一真實來源（source of truth）。cvewatch 不應成為第二份需要手動維護的清冊，所以匯入必須能反覆執行、結果可預期，並保留比對所需但 Excel 沒有的資訊（處理狀態、歷史）。

## Goals / Non-Goals

**Goals:**
- 同一份 Excel 重複匯入結果相同（冪等）
- 任何無法辨識的產品寫法在匯入時就被指出，而不是比對時默默漏掉
- 一台設備多個要追蹤的版本（硬體韌體 + hypervisor）

**Non-Goals:**
- 在 cvewatch 內編輯資產（MVP 只讀；修改請改 Excel 後重匯）

## Decisions

### 資料模型

```
asset(id, asset_tag UNIQUE, name, category, location, owner,
      exposure CHECK IN ('internet','dmz','internal','mgmt'),
      criticality CHECK IN (1,2,3), notes, status CHECK IN ('active','retired'),
      first_seen, last_seen, retired_at)

component(id, asset_id → asset, kind CHECK IN ('product','package'),
          product_id → catalog_product NULL,   -- kind=product
          purl NULL,                           -- kind=package：不含版本、不含 epoch/upstream qualifier 的識別
          purl_full NULL,                      -- kind=package：含版本的完整 purl
          version_raw, version_norm NULL, source ('excel'|'sbom'|'dpkg'|'rpm'),
          updated_at, UNIQUE(asset_id, product_id), UNIQUE(asset_id, purl))

component_history(component_id, version_raw, changed_at)   -- 版本變更軌跡（記錄舊版本）

asset_unresolved(asset_id, product_raw, version_raw, seen_at)  -- 未辨識的產品寫法，每次匯入該資產時整批取代

catalog_product(id, key UNIQUE, display_name, vendor_display,
                version_scheme, advisory_source NULL)
catalog_alias(product_id, alias_norm UNIQUE)             -- 小寫、去空白與符號後比較
catalog_cpe(product_id, cpe_vendor, cpe_product, cpe_part)
catalog_cna(product_id, cna_vendor_norm, cna_product_norm)  -- CVE 5.x affected[].vendor/product
catalog_csaf(product_id, csaf_vendor_norm, csaf_product_norm) -- CSAF product_tree 名稱
```

### Excel 範本欄位

工作表 `assets`（一列一台設備，帶一個主要元件）：

| 欄位 | 必填 | 說明 |
|---|---|---|
| asset_tag | ✓ | 唯一識別，例如財產編號 |
| name | ✓ | 主機名稱 |
| category | ✓ | network / security / virtualization / server / storage / host |
| product | ✓ | 產品名稱，可用任何別名（「PAN-OS」「Palo Alto XYZ-100」） |
| version | ✓ | 原樣填寫，例如 `11.1.4-h7`、`8.0 U3c`、`7.2.2-72806 Update 3` |
| exposure |  | 預設 internal |
| criticality |  | 1 低 / 2 中 / 3 高，預設 2 |
| location / owner / notes |  | |

工作表 `components`（選填，一台設備的其他元件）：`asset_tag, product, version`。
CSV 匯入時兩者分兩個檔：`--components components.csv`。
欄位名稱可在設定檔 `[import.columns]` 中對應（例如 `財產編號 = "asset_tag"`），因為使用者現有 Excel 多半是中文欄名。

### 產品辨識

1. `product` 欄位正規化（Unicode NFKC、小寫、移除空白與 `-_./`）
2. 查 `catalog_alias`，完全相符即命中
3. 不相符 → 該列標為 `unresolved`，匯入報表列出並給出最接近的 3 個別名建議（strsim Jaro-Winkler ≥ 0.85）
4. `unresolved` 列仍建立 asset，但不建立 component，04 比對時不會處理（報表會持續提醒）

### 種子目錄（節錄，實作時補齊）

```toml
[[product]]
key = "paloalto-panos"
display_name = "PAN-OS"
vendor_display = "Palo Alto Networks"
version_scheme = "panos"            # 11.1.4-h7
advisory_source = "paloalto"        # 對應 [[sources.csaf]] 的 id
aliases = ["pan-os", "panos", "palo alto", "xyz-100", "xyz100", "palo alto networks"]
cpe = [{ part = "o", vendor = "paloaltonetworks", product = "pan-os" }]
cna = [{ vendor = "palo alto networks", product = "pan-os" }]
csaf = [{ vendor = "palo alto networks", product = "pan-os" }]   # CSAF product_tree 的 vendor / product_name

[[product]]
key = "synology-dsm"
version_scheme = "dsm"              # 7.2.2-72806 Update 3
aliases = ["dsm", "diskstation manager", "synology", "xyz-300+"]
cpe = [{ part = "o", vendor = "synology", product = "diskstation_manager" }]
cna = [{ vendor = "synology", product = "diskstation manager" }]
```

其餘：`vmware-esxi`（scheme `esxi`，可吃 `8.0 U3c` 或 build `24414501`）、`vmware-vcenter`（`esxi`）、`proxmox-ve`（`dotted`）、`dell-idrac9`（`dotted`）、`dell-poweredge-bios`（`dotted`；CPE 依機型而異，種子不列）、`debian`、`ubuntu`（`ubuntu`，可吃 `22.04.4 LTS`）、`rhel`、`windows-server`（`dotted`，以 build 比較）。

`key` 與 `display_name` 自動視為別名。使用者檔與既有產品同 `key` 時：有寫的欄位覆寫、沒寫的沿用，`aliases` 與既有別名合併（只想加別名時寫 `key` + `aliases` 即可）；新 `key` 必須提供 `display_name`、`vendor_display`、`version_scheme`。`catalog.d` 依檔名排序載入，未知欄位、不存在的 version_scheme 或別名衝突都拒絕載入（結束碼 2）。

`version_scheme` 實際比較邏輯在 04 實作，02 只需在匯入時呼叫 `parse()` 檢查可否解析，不可解析者給警告。

### SBOM / 套件匯入

`cvewatch asset import-packages <asset_tag> <file> [--format auto|cyclonedx|spdx|dpkg|rpm] [--distro debian-12]`

- CycloneDX / SPDX：直接讀 component 的 purl
- `dpkg-query -W -f='${Package}\t${Version}\n'` 輸出 → `pkg:deb/<發行版名稱>/<name>@<version>?distro=<distro>`，例如 `pkg:deb/debian/openssl@3.0.15-1~deb12u1?distro=debian-12`；`name:arch` 轉為 `arch` qualifier；版本為空（已移除）的列略過
- `rpm -qa --qf '%{NAME}\t%{EPOCH}:%{VERSION}-%{RELEASE}\n'` → `pkg:rpm/<namespace>/<name>@<version>-<release>?distro=<distro>[&epoch=N]`（purl 規範的 epoch qualifier，與 syft 一致）；namespace 對應 rhel→redhat、alma→almalinux、ol→oracle、amzn→amazon，其餘沿用發行版名稱
- 元件以**不含版本**、並去掉 `epoch` / `upstream` qualifier 的 purl 識別（版本變更時更新同一元件並寫入 history）；完整 purl 另存 `purl_full`；`version_raw` 存完整版本，rpm 為 EVR（例如 `1:3.0.7-27.el9`），供 04 比較
- 格式自動判斷：JSON 依 `bomFormat = CycloneDX` / `spdxVersion`；文字檔須每行為兩欄 tab 分隔，第二欄全部帶 `(none):` 或 `數字:` 前綴時視為 rpm，否則為 dpkg；判斷錯誤時以 `--format` 指定
- dpkg/rpm 文字檔需 `--distro`（OSV 查詢需要 ecosystem，例如 `Debian:12`）
- 匯入是**整批取代**該資產的 package component（不在新清單中的套件刪除），版本變更寫入 history

### 匯入流程

```
cvewatch asset import inventory.xlsx --dry-run
  新增 3、更新 5（版本變更 2）、未變 40、未辨識產品 1、清冊中消失 2（未加 --prune 不處理）
cvewatch asset import inventory.xlsx
```

整個匯入在一個交易內；任何一列格式錯誤（缺必填欄位）→ 整批中止，列出錯誤列號。未辨識產品不算格式錯誤。

## Risks / Trade-offs

- 產品別名表需要持續維護 → 用 `catalog.d` 讓使用者不改程式就能擴充；`asset catalog --unresolved` 列出待補別名
- ESXi 版本用「8.0 U3c」與 build number 兩種寫法，CVE 資料多半用前者、VMSA 用 build → scheme `esxi` 需要內建 Update 名稱 ↔ build 對照表（由 04 任務 1.3 實作版本比較時從 Broadcom KB 整理並放入種子檔；02 只檢查可否解析）
