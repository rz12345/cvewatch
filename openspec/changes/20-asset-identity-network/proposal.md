# Proposal

## Why

使用者目前只能用資產編號與名稱辨識資產；設備的廠牌、型號、序號、財產編號、主機名稱、IP 與管理介面位址，仍要另外在財產清冊或試算表查。CVE 命中後要找到那台設備、登入管理介面處理時，得在多個檔案間來回對照。後續的 22-asset-lifecycle-eol 也需要「廠牌＋型號」才能對應硬體 EoL，23-isms-export 則需要廠牌與型號才能寫出 ISMS「資產描述」。

## What Changes

- 新增資產的識別欄位（全部選填，前後空白會去除）：
  - 廠牌 `brand`、型號 `model`、序號 `serial_number`：純文字。序號不檢查重複
  - 財產編號 `property_number`：不可與其他資產（含 retired，不分大小寫）重複。重複時清冊匯入整批中止，畫面編修拒絕該筆
  - 主機名稱 `hostname`：只接受英數字、`-`、`.`、`_`，最長 253 字元
  - 管理介面位址 `management_url`：只接受 `http://` 或 `https://` 網址，且不可包含帳號或密碼
- 新增資產的 **IP 位址**（可有多筆）：每筆是一個 IPv4 或 IPv6 位址，加上選填的用途／備註。同一資產的位址不可重複，不同資產可以相同
  - 清冊新增 `ips` 工作表（`asset_tag`、`ip`、`purpose`），一列一筆，作法比照 `components` 工作表。CSV 清冊以 `cvewatch asset import --ips <CSV>` 另外指定 IP 檔
  - 清冊有 `ips` 工作表時，清冊中每個資產的 IP 以工作表內容為準（沒有列出的資產會清空 IP）
  - 清冊沒有 `ips` 工作表時，既有資產的 IP 保留不變，匯入報表給出一次提醒
- 舊清冊沒有識別欄位時，既有資產的值保留不變，併入現有的「清冊沒有 … 欄」提醒（比照 16 的 `kept_fields`）
- 範本與匯出：`assets` 工作表加入 6 個識別欄位，並新增 `ips` 工作表。匯出後再匯入，預覽結果仍為「新增 0、更新 0」
- 從 ISMS 清冊匯入（18）時，不寫入、也不清除識別欄位與 IP
- 編修：GUI 資產對話框新增「識別與網路」區塊，含 6 個欄位與 IP 清單（可新增與移除列）。錯誤顯示在對應欄位或 IP 列旁。frontend 表單同步支援
- 詳情：GUI 資產詳情顯示識別欄位與 IP，管理介面位址顯示成連結，點擊後以預設瀏覽器開啟。TUI 資產詳情與 CLI `asset show` 也列出這些欄位
- GUI 資產清單上方新增搜尋框：可依編號、名稱、主機名稱、IP、序號、財產編號篩選（部分相符、不分大小寫），並可與「顯示已退役」「只顯示版本待補」同時使用
- GUI 匯入 CSV 清冊時，除了元件檔，也可另外選擇 IP 檔

## Capabilities

### New Capabilities

（無）

### Modified Capabilities

- `asset-inventory`：新增 Asset identity fields、Unique property number、Hostname format、Management address、Asset IP addresses、Inventory ips sheet、Inventory without identity columns、Showing asset identity；修改 Blank template export、Inventory export（`ips` 工作表與識別欄位），以及 Creating an asset、Editing asset fields（新欄位與 IP）
- `gui-console`：新增 GUI asset identity input、GUI asset identity detail、GUI asset list search、GUI CSV IP file
- `tui-console`：新增 Asset identity in the assets screen
- `isms-inventory`：新增 ISMS import keeps identity fields

## Out of scope

- 依廠牌＋型號對應硬體 EoL（22-asset-lifecycle-eol）
- 把廠牌與型號寫進 ISMS「資產描述」並匯出 ISMS 格式（23-isms-export）
- 元件角色（21-component-roles）
- 廠牌、型號與產品目錄的連動或自動帶出（例如由型號推斷產品）；廠牌、型號的選項清單
- 網段（CIDR）、位址範圍、IPv6 zone ID（例如 `%eth0`）、MAC 位址、VLAN
- 跨資產的 IP 或主機名稱重複檢查、DNS 解析與連線檢查（ping、掃描）
- 序號重複檢查
- 依 IP 或主機名稱比對 CVE，或匯入網路掃描結果
- 在 TUI 開啟管理介面位址；TUI 與 CLI 編修識別欄位（沿用清冊或 GUI）
- 搜尋廠牌、型號、管理介面位址；在命中畫面依 IP 搜尋
- 儲存管理介面的帳號或密碼

## Impact

- `crates/core/src/db/migrations/0009_asset_identity.sql`：`asset` 新增 6 欄，以及財產編號的部分唯一索引；新增 `asset_ip` 表（`ON DELETE CASCADE` 參照 `asset`）
- `crates/core/src/assets/`：`model.rs`（`AssetRecord` 新欄位）、新增 `record_identity.rs`（欄位格式驗證）與 `ips.rs`（IP 型別、驗證、`AssetIpRepo`）、`record.rs`、`columns.rs`（新欄位、`SheetKind::Ips`）、`sheet.rs`（`ips` 工作表、CSV IP 檔）、新增 `validate_ips.rs`、`validate.rs`、`plan.rs`／新增 `plan_identity.rs`（財產編號衝突、IP 差異、保留提醒）、`plan_systems.rs`（`keep_existing`）、`import.rs`、`repo.rs`、`edit.rs`（`EditOp::SetIps`；退役、恢復、永久刪除移到 `edit_lifecycle.rs` 以控制檔案大小）、`export.rs`、`template.rs`、`isms/`（不覆寫識別欄位）
- `crates/frontend`：`asset_form/`（新欄位、IP 草稿、錯誤用語移到 `errors.rs`）、`data.rs`（`AssetEntry.ips`、`AssetListFilter.search`）、`display.rs`（識別欄位的呈現）、`workspace/assets.rs`、`workspace/links.rs`（開啟管理介面位址）
- `crates/gui`：`dialogs/asset_edit.rs`、新增 `dialogs/asset_identity.rs`、`screens/assets.rs`、`screens/asset_list.rs`（搜尋框）、`dialogs/import.rs`、`companion.rs`、`files.rs`（選擇 IP 檔）
- `crates/tui`：`ui/screens.rs` 資產詳情
- `crates/cvewatch`：`cli.rs`（`asset import --ips`）、`commands/asset_view.rs`（`asset show`）
- `crates/core/examples/gen_inventory_fixtures.rs` 與 `tests/fixtures` 的清冊；`docs/user-guide/assets.md`、`gui.md`
- 不新增外部相依：IP 以 Rust 標準函式庫解析，網址以 core 現有的 `url` crate 解析，開啟瀏覽器沿用 frontend 現有的 `open` crate
- 既有使用者：舊清冊仍可匯入（識別欄位與 IP 保留既有值）；建議升級後匯出一次新格式的清冊
