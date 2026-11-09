# Tasks

前提：19-system-code-edit 與 20-asset-identity-network 已 archive。本 change 只有 ADDED requirement，沒有 MODIFIED 區塊；但 20 可能修改 GUI 資產詳情（GUI asset presentation）與 TUI 資產詳情的版面，apply 前先依 20 封存後的主 spec 與程式重新核對本 change 的 GUI／TUI requirement 與 design D5、D6 的插入位置，必要時更新 design.md 並記錄。測試不呼叫真實外部 API、不存取真實 Credential Manager；core 測試使用 `Catalog::from_sources` 與記憶體資料庫，GUI 測試使用 `egui_kittest::Harness` 與 `cvewatch_frontend::test_support` 替身，範例只用虛構的產品 key（`example-fw`、`example-bmc`）與資產編號（`FW-EDGE`、`SRV-01`）。

## 1. core：角色定義與目錄載入

- [ ] 1.1 新增 `crates/core/src/catalog/role.rs`：`ComponentRole`（11 個值，列舉順序即 design D1 的組順序）、`ALL`、`as_str`、`label`、`parse`、`names`、`Default = Other`，由 `catalog` 重新匯出。完成條件：單元測試斷言 11 個值的 `as_str`／`label` 與 design D1 表格一致、`parse("OS")` 與 `parse("database")` 為 None、`ALL` 依序排列
- [ ] 1.2 `ProductDef` 新增 `role: Option<String>`，`Product` 新增 `role: ComponentRole`；`merge_into` 以「有寫覆寫、沒寫沿用」合併；`from_sources` 在合併前逐來源驗證 `role`，錯誤訊息格式依 design D2；`build_product` 未宣告時為 `Other`；`fingerprint` 不納入角色。完成條件：`cargo build -p cvewatch-core` 成功，既有 `crates/core/tests/catalog.rs` 全部通過
- [ ] 1.3 core 測試：`crates/core/tests/catalog.rs` 加入 spec 的 Product without a role、Unknown role value（錯誤訊息含使用者檔名、`example-fw`、`database` 與可用值；覆寫種子 key 時錯誤也指向使用者檔而非「內建種子目錄」）、Overriding a seed role、Override without a role keeps the seed role、Role declared for a local product、兩個使用者檔依檔名順序最後者為準、`role = "other"` 可把種子產品改回其他，以及只改角色時 `fingerprint()` 不變（Role change does not rematch）。完成條件：`cargo test -p cvewatch-core --test catalog` 通過

## 2. 種子目錄角色

- [ ] 2.1 `seed.toml` 檔頭欄位說明加入 `role` 與可用值（含中文名稱）；各區段依 design D3 為網路／資安、虛擬化／伺服器、儲存三區段共 37 項加上 `role`。完成條件：目錄載入成功，`cvewatch asset catalog list` 中這 37 項的角色與 D3 表格一致
- [ ] 2.2 依 design D3 為備份、網站與資料庫、執行環境與框架、前端函式庫、系統元件、作業系統六區段共 41 項加上 `role`。完成條件：目錄載入成功，78 項都有 `role` 行（`grep -c '^role = ' crates/core/catalog/seed.toml` 為 78），`scripts/check-sensitive.ps1` 對暫存的 `seed.toml` 無命中
- [ ] 2.3 種子測試：`crates/core/tests/catalog.rs` 加入 spec 的 Network and storage firmware、Server components、Server software、Every seed product has a role（沒有產品為 `Other`），並以 D3 表格逐項斷言 78 個 key 的角色與各角色的數量（firmware 28、bios 1、bmc 1、os 7、dbms 6、web 3、tool 6、security 2、suite 6、app 18）。完成條件：`cargo test -p cvewatch-core --test catalog` 通過

## 3. frontend：依角色分組

- [ ] 3.1 新增 `crates/frontend/src/component_groups.rs`：`ComponentGroup`、`PACKAGES_LABEL`、`group_of`、`group_components`、`group_drafts`（design D4），由 `lib.rs` 匯出。完成條件：`cargo clippy -p cvewatch-frontend --all-targets -- -D warnings` 通過
- [ ] 3.2 frontend 測試：`crates/frontend/tests/component_groups.rs` 斷言 spec 的 Server components in role order（輸入 MySQL、Ubuntu、PowerEdge BIOS、nginx、iDRAC9 與兩個套件，輸出六組且順序正確、組內維持輸入順序、空組不輸出）、Role changed locally（以 `catalog.d` 覆寫 `nginx` 為 `tool` 的目錄重新分組）、Product removed from the catalog（目錄沒有的 key 歸入其他）、Pending component keeps its group，以及 `group_drafts` 回傳原索引且套件草稿歸入「套件」。完成條件：`cargo test -p cvewatch-frontend --test component_groups` 通過

## 4. GUI：詳情與編輯對話框

- [ ] 4.1 `screens/assets.rs` 的 `components_list` 依 `group_components` 輸出組名與縮排的元件、版本歷史（design D5）。完成條件：`cargo clippy -p cvewatch-gui --all-targets -- -D warnings` 通過，既有 `crates/gui/tests/asset_present.rs`（Product shown by display name 等）仍通過
- [ ] 4.2 `dialogs/asset_components.rs` 的 `components` 依 `group_drafts` 插入組名列，`component_row`、欄位錯誤、焦點與移除沿用原索引；更新模組說明註解（design D5）。完成條件：既有 `crates/gui/tests/asset_edit.rs` 的 `asset_edit_keyboard_flow_when_adding_a_component` 與元件錯誤相關測試仍通過
- [ ] 4.3 `docs/user-guide/gui.md` 說明資產詳情與編輯對話框的元件依角色分組、套件另成一組、角色可在 `catalog.d` 覆寫。完成條件：文件中的組名與順序與 design D1 一致
- [ ] 4.4 GUI 測試：新增 `crates/gui/tests/asset_component_groups.rs`，以 kittest 重現 spec 的 Grouped components in the detail pane（組名依序出現且元件位於對應組名之下）、Added component joins its group（nginx 版本欄在「網站服務系統」組內並取得焦點、其他元件已輸入的版本不變）、Package group in the edit dialog（套件列沒有版本欄與移除按鈕）。完成條件：`cargo test -p cvewatch-gui` 全部通過

## 5. TUI：資產詳情的角色欄

- [ ] 5.1 `ui/screens.rs` 資產詳情的元件表格加入第一欄「角色」並依 `group_components` 排列（design D6）。完成條件：`cargo clippy -p cvewatch-tui --all-targets -- -D warnings` 通過
- [ ] 5.2 TUI 測試：更新快照 `assets_pending__asset_detail_shows_components_findings_and_unresolved.snap`（Single firmware component：`FW-EDGE` 的 PAN-OS 列角色為「韌體」），並在 `crates/tui/tests/assets_pending.rs` 新增 Role column in the asset detail 的測試（`SRV-01` 依序為 BMC、作業系統、套件；寬字元不使欄位錯位）。完成條件：`cargo test -p cvewatch-tui` 通過，快照差異只有角色欄

## 6. CLI：`asset show` 與目錄輸出

- [ ] 6.1 `asset_view.rs`：`show` 載入目錄並以純函式 `component_lines` 依組輸出元件與版本歷史，未辨識產品在最後（design D7）。完成條件：單元測試斷言 spec 的 Grouped output on the command line 與 Unresolved products after the groups 的輸出行
- [ ] 6.2 `catalog_list` 加入 `role` 欄、`print_product` 輸出「角色：中文名稱（值）」（design D7）。完成條件：單元測試或 `assert_cmd` 測試斷言 spec 的 Role in the product list（`paloalto-panos` 為 `firmware`、`mysql` 為 `dbms`）、Role in product details（`dell-idrac9` 輸出「角色：BMC（bmc）」）與 Local product without a role in details（「角色：其他（other）」）
- [ ] 6.3 `docs/user-guide/assets.md`：`catalog.d` 一節加入 `role` 欄位、可用值與中文名稱、覆寫範例（虛構 key）、未宣告為其他、寫錯值使目錄載入失敗、回復舊版前需刪除 `role` 行；產品清單說明以 `asset catalog list` 查看角色。完成條件：文件中的值與名稱與 design D1 一致，範例可被目錄載入（以 1.3 的測試寫法驗證同一段 TOML）
- [ ] 6.4 CLI 測試：`cargo test -p cvewatch` 通過（含 6.1、6.2；`asset catalog` 測試只用內建種子或暫存目錄的 `catalog.d`，不開啟加密資料庫）

## 7. 整合驗證

- [ ] 7.1 全專案檢查：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`openspec validate 21-component-roles --strict`，暫存全部變更後 `scripts/check-sensitive.ps1` 無命中。完成條件：五項皆成功

## Workflow follow-up

- archive 時同步主 spec：`product-catalog`（新增 Component role declaration、Local role overrides、Seed catalog roles、Component role in catalog output）、`asset-inventory`（新增 Component grouping by role、Asset show groups components by role）、`gui-console`（新增 GUI component groups）、`tui-console`（新增 TUI component roles）
- archive 後執行 `openspec validate --all --strict` 確認通過
- 每次 commit 前執行 `scripts/check-sensitive.ps1`
- 之後依序 propose 22-asset-lifecycle-eol、23-isms-export
