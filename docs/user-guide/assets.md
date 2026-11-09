# 資產清冊

資產以 Excel／CSV 清冊匯入，也可以在視窗版直接新增與修改；**資料庫是目前狀態，清冊是交換格式**。

```powershell
cvewatch asset template assets.xlsx              # 空白範本（assets、components 工作表與範例列，classes 子類別說明）
cvewatch asset import assets.xlsx --dry-run      # 預覽差異（新增、更新、版本變更、清冊中消失），不寫入
cvewatch asset import assets.xlsx                # 匯入並自動比對
cvewatch asset export inventory.xlsx             # 匯出目前使用中的資產（格式與匯入相同）
cvewatch asset rename NAS-01 NAS-11              # 更改資產編號（元件、命中與處理紀錄保留）
```

## 清冊欄位（`assets` 工作表）

| 欄位 | 必填 | 說明 |
|---|---|---|
| `asset_tag` | ✓ | 資產編號（cvewatch 的識別，不分大小寫唯一） |
| `name` | ✓ | 名稱 |
| `category` | ✓ | ISMS 子類別代碼（例如 `HW01`，不分大小寫、全形字亦可）或子類別名稱（例如「伺服器」）；匯出寫代碼 |
| `product`、`version` | ✓ | 第一個元件（其餘元件寫在 `components` 工作表） |
| `exposure` | | `internet`、`dmz`、`internal`、`mgmt`，空白為 `internal` |
| `confidentiality`、`integrity`、`availability` | | 機密性、完整性、可用性（1–4）；空白帶入子類別的範本值。**資產價值 = MAX(C, I, A)**，用於同等級內的風險排序 |
| `isms_id` | | ISMS 資產編號 `系統代碼(3 英數字)-大類(HW/SW)-流水號(3 數字)`，例如 `ALL-HW-001`；大類須與子類別一致，不分大小寫唯一（含已退役資產）；留空表示沒有 ISMS 編號，整欄不存在時保留既有資產的編號 |
| `location`、`owner`、`notes` | | 位置、負責人、備註 |

- `criticality`（舊的重要性 1–3）**已停用**：匯入時忽略，報表給出一次提醒
- 舊版 category 值仍可匯入並轉換（報表逐列提醒）：network → `HW02`、security → `HW03`、virtualization → `HW01`、server → `HW01`、storage → `HW05`、host → `HW01`（host 若是個人電腦請改為 `HW06`）
- 畫面編修只能選已定義的子類別；修改類別不會改變已設定的 C/I/A

## ISMS 子類別

內建 ISMS 資訊資產清冊 v2.1 的實體（HW）與軟體（SW）子類別；人員（PE）、資訊（IF）、服務（SE）不納入 cvewatch。C/I/A 為範本值：

| 代碼 | 名稱 | C/I/A | 代碼 | 名稱 | C/I/A |
|---|---|---|---|---|---|
| HW01 | 伺服器 | 2/3/4 | SW01 | 應用系統 | 無（必填） |
| HW02 | 通訊設備 | 2/3/4 | SW02 | 作業系統 | 2/3/3 |
| HW03 | 資安設備 | 2/3/4 | SW03 | 資料庫管理系統 | 2/3/3 |
| HW04 | 週邊設備 | 2/3/3 | SW04 | 網站服務系統 | 2/3/3 |
| HW05 | 儲存設備 | 2/3/4 | SW05 | 應用工具軟體 | 2/3/3 |
| HW06 | 個人電腦 | 2/3/3 | SW06 | 資通安全系統 | 2/3/3 |
| HW07 | 公共使用電腦 | 2/3/2 | SW07 | 一般套裝軟體 | 2/2/2 |
| HW08 | 日常作業事務機器 | 2/3/3 | | | |
| HW09 | 行動裝置 | 2/3/3 | | | |
| HW10 | 物聯網設備 | 2/2/3 | | | |
| HW11 | 基礎設施 | 3/3/4 | | | |

設定檔可覆寫內建子類別的名稱、描述與範本值，或新增子類別（代碼為 `HW`／`SW` 加兩位數字；新增時 `name` 必填，C/I/A 三項全寫或全不寫）。範本值只影響之後新增或留空匯入的資產，**不會改變既有資產的 C/I/A**；範本的 `classes` 工作表依目前設定列出：

```toml
[asset_classes.HW05]
confidentiality = 3          # 只覆寫有寫的欄位

[asset_classes.HW12]
name = "實驗儀器"
description = "示波器、量測儀器等具網路功能的設備"
confidentiality = 2
integrity = 3
availability = 3
```

## 從舊版升級（類別與重要性改為 ISMS 子類別與 C/I/A）

1. **替換執行檔之前**，先以目前（舊版）的 cvewatch 備份：`cvewatch db backup D:\backup\cvewatch-before-13.db`。新版第一次開啟資料庫時就會轉換（包含新版的 `db backup` 與排程執行的 `sync`），因此以新版執行的備份已是轉換後的內容；轉換後的資料庫無法以舊版開啟，回復請以舊版的 `cvewatch db restore` 還原上述備份並改回舊版執行檔
2. 轉換規則：舊 category 依上方對照表轉為子類別代碼（含已退役資產）；C/I/A 帶入轉換後子類別的**內建**範本值（不套用設定檔的覆寫）；舊重要性附記在備註末尾（「舊重要性：3」）供對照。元件、版本歷史、命中與處理紀錄不變
3. 升級後執行一次 `cvewatch asset export inventory-new.xlsx` 取得新格式清冊，在其中調整類別（例如 host → `HW06`）與 C/I/A，之後以新格式清冊維護。舊清冊仍可匯入，但沒有 C/I/A 欄時會以範本值覆寫畫面上調整過的值（`--dry-run` 的欄位變更會列出）；沒有 `isms_id` 欄時則保留資料庫中既有的 ISMS 編號（報表給出一次提醒）

## 匯出與更名

- `asset export`：每個資產一列 `assets`（第一個元件寫在該列的 product / version），其餘元件與未辨識產品列在 `components`；產品寫目錄的顯示名稱、版本寫原始字串，因此匯出後直接重匯為「新增 0、更新 0」。不含已退役資產與主機套件（套件仍以 `asset import-packages` 管理）；目的檔已存在時不覆蓋；匯出檔未加密，含資產清單，請妥善保管
- `asset rename <舊編號> <新編號>`：只能更名使用中資產；新編號不可與其他資產（含已退役、不分大小寫）重複，只改大小寫可以（之後匯入的清冊須使用新的寫法：與既有編號只差大小寫的列會被拒絕）。元件、版本歷史、命中與處理紀錄都保留，比對結果不變。**系統不記錄舊編號**：更名後，舊清冊與已匯出的 VEX 中的舊編號不會對應到此資產（匯入舊清冊會把舊編號當成新資產，搭配 `--prune` 時更名後的資產會被標為退役；VEX 的判定列為未對應），舊編號可再被新增使用。已送出的通知與既有 Redmine 議題內容保留舊編號，之後的通知與註記使用新編號
- 匯入會以檔案內容**覆寫**資產欄位與清冊元件（檔案中沒有的清冊元件會被移除）。在視窗版修改過資產後，請依「**先匯出、再編輯、再匯入**」的順序，避免舊清冊蓋掉畫面上的修改（更名後也一樣，舊清冊中的舊編號不會對應到更名後的資產）：

```powershell
cvewatch asset export inventory-new.xlsx         # 1. 匯出目前狀態
#                                                  2. 用 Excel 編輯 inventory-new.xlsx
cvewatch asset import inventory-new.xlsx --dry-run   # 3. 確認差異只有剛才的編輯
cvewatch asset import inventory-new.xlsx
```
