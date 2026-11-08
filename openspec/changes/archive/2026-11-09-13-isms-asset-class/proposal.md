# Proposal

## Why

使用者的資產同時要登錄在機構的 ISMS「資訊資產清冊」（B390-I305-01 v2.1）。這份表單以 ISMS 子類別（如 HW01 伺服器、HW03 資安設備、SW01 應用系統）分類資產，並以機密性、完整性、可用性（C/I/A，1–4）評定資產價值。cvewatch 的 6 種 category（network、security…）與重要性（1–3）跟它對不上，使用者得維護兩套分類與兩種價值評分。這個 change 是「對齊 ISMS 清冊」系列的第一步：先把類別與價值模型對齊，後續欄位與匯出都以它為基礎。

## What Changes

- **BREAKING** 資產類別改為兩層：大類（HW 實體、SW 軟體）＋ ISMS 子類別代碼。category 欄存放子類別代碼（例如 `HW01`），大類由代碼前兩碼推得
  - 內建 v2.1 的 18 個子類別（HW01–HW11、SW01–SW07），各有名稱、描述與 C/I/A 範本值
  - 設定檔可覆寫內建子類別的名稱、描述、範本值，也可新增子類別（代碼須為 `HW` 或 `SW` 加兩位數字）
  - 只收 HW 與 SW；PE（人員）、IF（資訊）、SE（服務）不納入 cvewatch
  - 清冊匯入的 category 可填代碼（`HW01`）、子類別名稱（`伺服器`），或舊值（`server` 等）；舊值會轉成對應代碼並在報表給出警告
- **BREAKING** 以 C/I/A 取代重要性：每個資產有機密性、完整性、可用性（1–4）。清冊留空時帶入子類別的範本值。資產價值 = MAX(C, I, A)，取代重要性用於風險排序
  - 清冊的 `criticality` 欄停用：匯入時忽略，並給出一次警告
- 新增選填的 ISMS 資產編號（`isms_id`，如 `ALL-HW-001`）：驗證 `系統代碼(3)-大類(2)-流水號(3)` 格式，大類須與子類別一致，不分大小寫唯一；`asset_tag` 不變
- 既有資料轉換（migration）：舊 category 依對照表轉成子類別代碼；C/I/A 帶入子類別的內建範本值；舊重要性附記在 notes（「舊重要性：N」）供對照
- CLI、TUI、GUI 的資產顯示、編輯、篩選與清冊範本、匯出改用子類別、C/I/A、資產價值與 ISMS 編號；範本另附一張子類別說明工作表

## Capabilities

### New Capabilities

（無）

### Modified Capabilities

- `asset-inventory`：以「Asset class」「CIA rating and asset value」「ISMS asset ID」「Exposure attribute」「Migrating existing assets」取代「Exposure and criticality attributes」；修改「Creating an asset」「Editing asset fields」「Blank template export」中與重要性、category 有關的規則
- `risk-scoring`：「Ordering within a tier」改以資產價值排序
- `gui-console`：「GUI asset editing」改為子類別、C/I/A 與 ISMS 編號的輸入；「GUI asset presentation」改為以「代碼 名稱」呈現子類別，並顯示資產價值

## Out of scope

- PE、IF、SE 三個大類的資產
- ISMS 的權責單位、保管人、使用單位、法規遵循性、防護等級、是否含個資（14-isms-admin-fields）
- 廠牌、型號、序號、財產編號、IP、主機名稱、管理介面（15-asset-identity-network）
- 元件角色（16-component-roles）
- 保固、維護合約到期與 EoL（17-asset-lifecycle-eol）
- 匯出 ISMS v2.1 格式的清冊（18-isms-export）
- 依 ISMS 編號自動產生流水號
- 在 GUI 設定頁編輯子類別清單（只能改設定檔）
- 依 ISMS 備註 2「底層支撐資產價值 ≧ 承載資產最高價值」自動檢查資產間的相依

## Impact

- `crates/core/src/assets/`：`model.rs`（`Category` 列舉改為子類別代碼與子類別表）、`validate.rs`、`columns.rs`（新增 `confidentiality`、`integrity`、`availability`、`isms_id`，`criticality` 改為停用欄）、`repo.rs`、`plan.rs`、`export.rs`、`template.rs`
- `crates/core/src/config.rs`：新增子類別覆寫區段
- `crates/core/src/db/migrations/0007_isms_class.sql`：新增欄位、轉換舊資料、重建 `active_component` view；`criticality` 欄保留在資料表但不再讀寫
- `crates/core/src/matching/`：`load.rs`、`query.rs`、`scoring.rs` 改用資產價值
- `crates/frontend`：`display.rs`、`asset_form.rs`、`filter.rs`；`crates/gui`：資產畫面與編輯對話框；`crates/tui`：命中清單的欄位；`crates/cvewatch`：`asset list/show`、`finding` 輸出
- `tests/fixtures` 的清冊與 `gen_inventory_fixtures.rs` 改用新欄位；README 的清冊欄位說明
- 既有使用者的清冊：舊 category 值仍可匯入（有警告），`criticality` 欄被忽略；建議匯出一次新格式清冊

## 後續 change（ISMS 清冊對齊系列）

| 編號 | 內容 | 依賴 |
|---|---|---|
| 14-isms-admin-fields | 權責單位、保管人（取代 owner）、使用單位、法規遵循性 D、防護等級 MAX(C,I,A,D)、是否含個資（子類別提供預設） | 13 |
| 15-asset-identity-network | 廠牌、型號、序號、財產編號；主機名稱、多筆 IP、管理介面位址 | 13 |
| 16-component-roles | 產品目錄宣告元件角色（os、dbms、web、tool、security、suite 對應 SW02–SW07，另有 firmware、bios、bmc、app），畫面依角色分組 | 13 |
| 17-asset-lifecycle-eol | 保固／維護合約到期；軟體 EoL 由 sync 從 endoflife.date 依產品版本分支取得；硬體型號與 DSM 等未涵蓋產品的 EoL 寫在目錄（附來源網址，catalog.d 可增修），資產可手動覆寫；畫面標示與篩選、email 摘要提醒（預設 90 天） | 15 |
| 18-isms-export | `asset export --isms` 產出 v2.1 欄位的「資訊資產清冊」工作表 | 13、14、15 |

後續 change 依專案流程在前一個 archive 後再以 propose 建立，屆時依實作結果調整範圍。
