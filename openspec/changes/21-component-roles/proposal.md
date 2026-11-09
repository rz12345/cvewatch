# Proposal

## Why

一個資產常有多個元件（伺服器的作業系統、資料庫、網站服務、BMC、BIOS，主機另有大量套件），目前詳情與編輯對話框只依資料庫順序平鋪列出，元件一多就看不出哪個是作業系統、哪個是韌體，主機套件也和清冊元件混在一起。ISMS 清冊以 SW02–SW07 區分軟體種類，產品目錄宣告元件角色後，畫面可以用同一套分類把元件分組，讓檢視與編修更清楚。

## What Changes

- 產品目錄新增選填欄位 `role`，宣告產品作為元件時的角色：
  - `os`、`dbms`、`web`、`tool`、`security`、`suite`：分別對應 ISMS 子類別 SW02 作業系統、SW03 資料庫管理系統、SW04 網站服務系統、SW05 應用工具軟體、SW06 資通安全系統、SW07 一般套裝軟體，中文顯示名稱與子類別名稱相同
  - `firmware`（韌體）、`bios`（BIOS）、`bmc`（BMC）、`app`（應用程式元件），以及可明確寫出的 `other`（其他）
  - 未宣告時為「其他」；值不在清單中時目錄載入失敗，錯誤訊息指出檔案、產品 key 與可用值（比照 `version_scheme`）
  - 本機 `catalog.d\*.toml` 可為新產品宣告角色，或覆寫既有產品的角色（未寫時沿用）
- 內建種子目錄的 78 項產品全部指定角色（design D3 逐項列出）
- **角色只由目錄決定**：元件本身不記錄角色、不能個別覆寫，不新增資料庫欄位或 migration；角色不影響比對、匯入與匯出，修改角色不會觸發重新比對
- 元件依角色分組顯示，組的順序固定（韌體、BIOS、BMC、作業系統、資料庫管理系統、網站服務系統、應用工具軟體、資通安全系統、一般套裝軟體、應用程式元件、其他），主機套件（SBOM／dpkg／rpm）另成最後一組「套件」；沒有元件的組不顯示：
  - GUI 資產詳情與資產編輯對話框的元件區塊
  - TUI 資產詳情的元件表格（新增「角色」欄，依組的順序排列）
  - CLI `cvewatch asset show`
- `cvewatch asset catalog list` 新增角色欄，`asset catalog show` 顯示角色的中文名稱與值
- 使用說明補上 `role` 欄位、可用值與 `catalog.d` 覆寫範例

## Capabilities

### New Capabilities

（無）

### Modified Capabilities

- `product-catalog`：新增「Component role declaration」「Local role overrides」「Seed catalog roles」「Component role in catalog output」
- `asset-inventory`：新增「Component grouping by role」「Asset show groups components by role」
- `gui-console`：新增「GUI component groups」
- `tui-console`：新增「TUI component roles」

## Out of scope

- 依元件角色建議或檢查資產的 SW 子類別（例如作業系統元件建議 SW02）
- ISMS 匯出（23-isms-export）依角色排序或篩選元件
- 命中清單、Dashboard、通知依角色篩選或分組
- 個別元件覆寫角色、在資料庫保存角色、在清冊（Excel／CSV）匯入或匯出角色
- GUI 產品選擇器、命中表或資產清單顯示角色；TUI 元件表格改用產品顯示名稱
- 以角色影響比對（版本格式、識別資料、關聯性篩選）或優先等級
- GUI 設定畫面或 CLI 子命令編輯目錄角色（只透過 `catalog.d` 檔案）

## Impact

- `crates/core/src/catalog/`：新增 `role.rs`（`ComponentRole` 列舉、值、中文名稱、固定順序）；`file.rs` 的 `ProductDef` 新增 `role`；`mod.rs` 的 `Product` 新增 `role`、`merge_into` 覆寫規則、逐來源驗證；`fingerprint` 不納入角色
- `crates/core/catalog/seed.toml`：檔頭欄位說明與 78 項產品的 `role`
- `crates/frontend`：新增 `component_groups.rs`（元件與編輯草稿依角色分組，套件另成一組）
- `crates/gui/src/screens/assets.rs`（`components_list`）、`crates/gui/src/dialogs/asset_components.rs`（元件表格）
- `crates/tui/src/ui/screens.rs`（資產詳情元件表格）與對應快照
- `crates/cvewatch/src/commands/asset_view.rs`（`show`、`catalog_list`、`print_product`）
- `docs/user-guide/assets.md`、`docs/user-guide/gui.md`
- 不需要 migration；不新增外部 API 呼叫
