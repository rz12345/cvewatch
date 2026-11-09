# Tasks

前提：19-system-code-edit 已 archive。apply 前先依前一個 change 封存後的主 spec 重新核對 MODIFIED 區塊（`asset-inventory` 的 Blank template export、Inventory export、Creating an asset、Editing asset fields），以主 spec 的最新內容為底重新套上本 change 的修改。測試不呼叫真實外部 API、不存取真實 Credential Manager；core 測試使用記憶體資料庫與 `tests/fixtures/`，GUI 測試使用 `egui_kittest::Harness` 與 `cvewatch_frontend::test_support` 替身。fixture 與測試只用虛構的資產編號、型號、序號與文件保留位址（`FW-EDGE`、`SRV-01`、`XYZ-100`、`SN0001`、`P-0001`、192.0.2.x、2001:db8::），commit 前執行 `scripts/check-sensitive.ps1`。每完成一個任務群組就執行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。

## 1. core：資料模型、migration 與欄位驗證

- [x] 1.1 `migrations/0009_asset_identity.sql`（design D1）並在 `migrate.rs` 登錄；`AssetRecord` 新增 `brand`、`model`、`serial_number`、`property_number`、`hostname`、`management_url`，`repo.rs` 的讀取、`insert`、`update_record` 一併處理。完成條件：新增 `tests/db_migration_identity.rs`，以 0008 版本資料庫建立 `FW-01`（2 筆命中）與 retired 的 `NAS-02`，升級後斷言兩者的 6 個新欄位都是 None、`asset_ip` 表存在且為空、命中數不變；寫入兩筆財產編號只差大小寫的資產時，被唯一索引拒絕
- [x] 1.2 新檔 `assets/record_identity.rs`：主機名稱與管理介面位址的解析（design D2）；`FieldErrorKind` 新增 `Format`、`Credentials`；`RecordInput` 新增 6 個欄位，`validate_with` 呼叫新解析函式。完成條件：單元測試重現 spec 的 Recording identity fields（去除前後空白）、Valid hostname、URL in the hostname field、HTTPS management address、Address without a scheme、Credentials in the address，並斷言 `ftp://192.0.2.1` 與 254 字元的主機名稱被拒絕；`record.rs` 低於 800 行
- [x] 1.3 新檔 `assets/ips.rs`：`AssetIp`、`IpInput`、`IpError`／`IpErrorKind`、`validate_ips`，以及 `AssetIpRepo`（`for_asset`、`all_by_asset`、`replace`；design D4）。完成條件：單元測試重現 spec 的 Several addresses on one asset（順序保留、IPv6 以小寫壓縮寫法儲存）、IPv6 written differently、Network instead of an address；`192.000.002.010`、`fe80::1%eth0` 被拒絕；`replace` 後依輸入順序讀回，兩個資產可存相同位址；永久刪除資產後該資產的 `asset_ip` 列消失
- [x] 1.4 core 測試：`cargo test -p cvewatch-core` 通過（同時更新既有測試中建立 `AssetRecord` 的地方）

## 2. core：畫面編修

- [x] 2.1 把 `edit.rs` 的 `retire_asset`、`restore_asset`、`purge_preview`、`purge_asset` 與其私有函式原樣移到 `assets/edit_lifecycle.rs`，由 `edit` 重新匯出（design D9）。完成條件：呼叫端不需修改，既有的 `tests/asset_edit.rs` 等測試全部通過，`edit.rs` 少於 680 行
- [x] 2.2 財產編號：`AssetRepo::property_number_in_use`、`clear_property_number`，`plan` 共用的 `property_in_use_message`，以及 `edit.rs` 的 `ensure_property_number_free`（新增與修改欄位時檢查；design D3）。完成條件：`tests/asset_edit.rs` 重現 spec 的 Property number used by a retired asset 與 Serial number may repeat
- [x] 2.3 `EditOp::SetIps`（`affects_matching` 為 false）與 `create_asset` 的 `ips` 參數（design D9）。完成條件：`tests/asset_edit.rs` 重現 spec 的 Adding an asset with addresses、Replacing addresses、One invalid address saves nothing，並斷言只有 `SetIps` 一個步驟時沒有記錄「比對待補」
- [x] 2.4 core 測試：`cargo test -p cvewatch-core` 通過（含 2.1～2.3）

## 3. core：清冊匯入、匯出與範本

- [x] 3.1 `columns.rs`：新增 6 個識別欄位與 `Ip`、`Purpose`，依 design D1 調整 `ASSET_COLUMNS`（24 欄）與 `StandardField::ALL`，`KEEPABLE_FIELDS` 加入識別欄位；新增 `SheetKind::Ips`；`RowRef` 顯示「ips 工作表第 N 列」；`plan_fields::field_label` 與 `record::RecordInput::value` 補上新欄位。完成條件：單元測試斷言 `ips` 工作表缺少 `ip` 欄時回報表頭錯誤，`[import.columns]` 的自訂欄名可對應到 `property_number`
- [x] 3.2 `sheet.rs` 讀取選填的 `ips` 工作表，`read_inventory` 新增 `ips_csv` 參數（Excel 清冊又指定時以錯誤結束）；新檔 `validate_ips.rs` 與 `Inventory.ips: IpsInput`（design D5）。完成條件：測試重現 spec 的 Asset not in the assets sheet，並斷言 `ips` 工作表的無效 IP 與同資產重複位址的錯誤含工作表與列號；沒有 `ips` 工作表時為 `Missing`
- [x] 3.3 清冊內財產編號重複（`SeenValues`）、新檔 `plan_identity.rs`（與清冊以外資產的財產編號衝突、`AssetUpdate.ip_change`、`IPS_KEPT_NOTICE`）、`keep_existing` 涵蓋識別欄位（design D3、D6、D7）。完成條件：測試重現 spec 的 Duplicate property number in the sheet、Re-importing an inventory from before this version（提醒列出 6 欄）、Inventory without an ips sheet；IP 沒有變更時不列為更新
- [x] 3.4 `import.rs`：`load_snapshot` 讀入 IP、寫入前執行 `release_changed_property_numbers`、`planned.ips` 為 Some 時取代 IP。完成條件：`tests/assets_import_identity.rs` 重現 spec 的 Importing addresses（含 `--dry-run` 不寫入且預覽列出 `ip`）、Asset left out of the ips sheet、Swapping property numbers by import、CSV inventory with an IP file
- [x] 3.5 ISMS 匯入：`isms/conversion.rs` 的 `convert_asset` 對既有資產沿用 6 個識別欄位；`to_inventory` 維持 `IpsInput::Untouched`（design D8）。完成條件：`tests/isms_import.rs` 重現 isms-inventory 的 Updating an asset that has identity data 與 New asset from the ISMS workbook，並斷言報表的提醒與欄位變更都沒有識別欄位與 IP
- [x] 3.6 `export.rs`、`template.rs`：寫出 6 欄與 `ips` 工作表，範本加入範例值（design D13）。完成條件：測試重現 spec 的 Generating a template（五個工作表與新表頭）、Addresses are exported；匯出後 `--dry-run` 重匯為「新增 0、更新 0」，沒有 IP 變更
- [x] 3.7 fixtures：`gen_inventory_fixtures.rs` 產出 `inventory_identity.xlsx`（含識別欄位與 `ips` 工作表）與 `inventory_ips.csv`，並在 3.4 的測試中使用。完成條件：重新產生後 `git diff --stat tests/fixtures` 只列出這兩個新檔，`scripts/check-sensitive.ps1` 沒有命中
- [x] 3.8 core 測試：`cargo test -p cvewatch-core` 全部通過

## 4. frontend

- [x] 4.1 把 `asset_form/mod.rs` 的 `FormErrors`、`field_label`、`field_message` 與錯誤用語常數原樣移到 `asset_form/errors.rs`，由 `asset_form` 重新匯出（design D10）。完成條件：呼叫端不需修改，`cargo test -p cvewatch-frontend` 通過，`mod.rs` 少於 700 行
- [x] 4.2 新檔 `asset_form/identity.rs`：`AssetForm` 的 6 個欄位與 `ips: Vec<IpDraft>`、`from_entry` 與 `record_input`、IP 列的驗證（空白列忽略）與 `FormErrors.ips`、`property_number_conflict`，以及新的介面用語。完成條件：`crates/frontend/tests/asset_edit.rs` 斷言無效 IP 的錯誤標在對應列、空白 IP 列不產生錯誤也不送出、財產編號衝突訊息為「已被使用（已退役的 NAS-02）」、主機名稱與管理介面錯誤不含英文欄名
- [x] 4.3 `data.rs`：`AssetEntry.ips`、`AssetListFilter.search`；`filter.rs` 的 `matches_asset_search`（design D10）。完成條件：單元測試斷言依編號、名稱、主機名稱、序號、財產編號、IP 部分相符且不分大小寫；`2001:DB8:0::1` 找得到 `2001:db8::1`；與 `include_retired`、`pending_only` 同時套用；空白查詢不篩選
- [x] 4.4 `workspace/assets.rs` 的「修改 IP」步驟與新增資產的 IP；`workspace/links.rs` 的 `open_management_url`；`display.rs` 的 `identity_rows`、`ip_text`。完成條件：`crates/frontend/tests/asset_edit.rs` 斷言只改 IP 時狀態列為「已儲存 SRV-01（修改 IP）」且 `needs_match` 為 false；以 `test_support` 的 Opener 替身斷言 `http://192.0.2.1/` 會被開啟、`ftp://192.0.2.1` 被拒絕；`identity_rows` 未填時為「-」
- [x] 4.5 frontend 測試：`cargo test -p cvewatch-frontend` 通過（含 4.1～4.4）

## 5. CLI、TUI 與文件

- [x] 5.1 CLI：`asset import` 新增 `--ips <CSV>` 並傳給 `read_inventory`；`asset template` 的說明文字加入 `ips` 工作表。完成條件：`crates/cvewatch/tests/cli.rs` 斷言 `cvewatch asset import --help` 列出 `--ips`
- [x] 5.2 CLI `asset show`：在 `admin_lines` 之後輸出 `identity_lines`（只列有值的欄位與每筆 IP；design D12）。完成條件：`asset_view.rs` 的單元測試重現 spec 的 Showing a firewall with identity data
- [x] 5.3 TUI 資產詳情加入識別欄位與 IP 兩行。完成條件：`crates/tui` 的畫面測試重現 tui-console 的 Identity in the asset detail；既有的資產畫面快照依新增的兩行更新
- [x] 5.4 文件：`docs/user-guide/assets.md` 的清冊欄位（6 個新欄位與規則）、新的「IP（`ips` 工作表）」段落（含 CSV `--ips`、有工作表時以工作表為準、沒有時保留）、升級說明（舊清冊保留既有值，「財產編號」自訂欄名的提醒）；`gui.md` 的資產編修與搜尋框。完成條件：文件中的欄名與 `cvewatch asset template` 產出的表頭一致
- [x] 5.5 測試：`cargo test -p cvewatch -p cvewatch-tui` 通過

## 6. GUI

- [x] 6.1 新檔 `dialogs/asset_identity.rs`：資產對話框的「識別與網路」區塊（6 個欄位、IP 清單、「新增 IP」與「移除」、列旁錯誤；design D11）。完成條件：`crates/gui/tests/asset_identity.rs` 重現 spec 的 Adding addresses in the dialog、Invalid address on its row、Property number in use、Management address without a scheme；`asset_edit.rs` 低於 800 行
- [x] 6.2 資產詳情的識別欄位、IP 與管理介面連結（`ui.link` → `Workspace::open_management_url`）。完成條件：kittest 重現 spec 的 Identity in the detail pane、Opening the management address（Opener 替身收到 `http://192.0.2.1/`、狀態列顯示已開啟）、Asset without a management address
- [x] 6.3 資產清單搜尋框（`assets_search`、搜尋中各組以另一個 id 展開、沒有結果的訊息）。完成條件：kittest 重現 spec 的 Searching by address、Searching by property number with letter case、Search combined with the retired filter、Collapsed group while searching、No matching asset
- [x] 6.4 匯入對話框的「選擇 IP 檔」：`ImportArgs.ips` → `--ips`、`FileDialogs::open_ips` 與測試替身。完成條件：`companion.rs` 單元測試斷言產生的參數含 `--ips C:\in\ips.csv`；`crates/gui/tests/import.rs` 重現 spec 的 Choosing an IP file（對話框顯示 IP 檔路徑並以含 `--ips` 的參數重新預覽）
- [x] 6.5 全專案檢查：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`openspec validate 20-asset-identity-network --strict`。完成條件：四個指令皆成功

## Workflow follow-up

- 使用者手動啟動 `cvewatchw.exe`，確認資產對話框的「識別與網路」區塊與 IP 清單中文顯示正常、版面沒有被截斷（視窗較窄時是否需要改成可收合區塊），以及資產清單的搜尋框與管理介面連結
- 依 `/opsx-auto` 流程：code review 一輪 → 判斷架構變更（新增 `asset_ip` 表，屬資料模型變更；沒有 crate 或外部整合的增減，視需要以 Archify 更新 `docs/architecture/`）→ commit → archive → commit
- archive 時同步主 spec：`asset-inventory`（新增 Asset identity fields、Unique property number、Hostname format、Management address、Asset IP addresses、Inventory ips sheet、Inventory without identity columns、Showing asset identity；修改 Blank template export、Inventory export、Creating an asset、Editing asset fields）、`gui-console`（新增 GUI asset identity input、GUI asset identity detail、GUI asset list search、GUI CSV IP file）、`tui-console`（新增 Asset identity in the assets screen）、`isms-inventory`（新增 ISMS import keeps identity fields）
- archive 後執行 `openspec validate --all --strict` 確認通過
- 之後依序 propose 21-component-roles、22-asset-lifecycle-eol（以廠牌＋型號對應硬體 EoL）、23-isms-export（廠牌與型號寫入 ISMS「資產描述」）
