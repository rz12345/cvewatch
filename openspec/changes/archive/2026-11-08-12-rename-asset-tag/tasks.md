# Tasks

前提：11-polish-asset-ux 已 archive。測試不呼叫真實外部 API、不存取真實 Credential Manager；core 測試使用記憶體資料庫與 `tests/fixtures/`，GUI 測試使用 `egui_kittest::Harness` 與 `cvewatch_frontend::test_support` 替身。

## 1. core：更名操作

- [x] 1.1 `AssetRepo::rename(id, new_tag)`，以及 `edit.rs` 的 `rename_in`：檢查空白、與原編號相同、`COLLATE NOCASE` 且排除自身 id 的重複；錯誤訊息區分 active 與 retired（design D1）。完成條件：`crates/core/tests/asset_rename.rs` 斷言重複（active、retired、只差大小寫的其他資產）、空白、相同編號都回傳 `CoreError::Invalid` 且資產不變
- [x] 1.2 公開 `rename_asset(conn, tag, new_tag)`（不寫時間欄位，故不需 `now`）：沿用 `active_asset`，用自己的交易且不呼叫 `mark_match_stale`（design D2），並寫入 info 日誌（from、to）。完成條件：測試斷言 `FW-01` 更名為 `FW-11` 後命中數、處理事件、元件版本歷史、未辨識產品都在新編號下；只改大小寫（`FW-01` → `fw-01`）成功；retired 資產被拒；更名後「比對待補」未被設定
- [x] 1.3 `EditOp::Rename(&str)`：`apply_edits` 中依序執行，與其他步驟同一交易。完成條件：測試斷言「更名＋改名稱」一起成功；「更名為已使用的編號＋改名稱」時編號與名稱都不變
- [x] 1.4 外部對應不認舊編號（spec Old asset tag is not remembered）。完成條件：core 測試（`crates/core/tests/asset_rename.rs`，沿用 VEX 測試的 `FW-01` 資料）在記憶體資料庫上重現三個 scenario：更名為 `FW-11` 後以 `--prune` 匯入舊清冊（新增 `FW-01`、`FW-11` 退役且命中保留）、匯入 bom-ref 為 `cvewatch:FW-01:paloalto-panos` 的 VEX（列為未對應）、新增舊編號 `FW-01`（與 `FW-11` 各自獨立）
- [x] 1.5 core 測試：`cargo test -p cvewatch-core` 通過（含 1.1～1.4，並把 `asset_edit.rs` 中「asset_tag 不可修改」的斷言改為說明 `Fields` 不使用 `input.asset_tag`）

## 2. CLI：`cvewatch asset rename`

- [x] 2.1 `cli.rs` 新增 `AssetAction::Rename { asset_tag, new_tag }` 與中文說明；`commands/asset.rs` 呼叫 `rename_asset` 並輸出 design D5 的兩行訊息；更新模組開頭的子命令清單註解。完成條件：`cvewatch asset rename --help` 顯示「更改資產編號」與兩個參數說明
- [x] 2.2 CLI 測試：`crates/cvewatch/tests/asset_rename.rs` 以 `assert_cmd` 驗證 `--help` 內容、少給參數時以錯誤結束、尚未 init 時提示 `cvewatch init`；`tests/cli.rs` 的 asset 子命令 help 清單加入 `rename`。更名的成功與失敗邏輯由 1.x 的 core 測試涵蓋（測試不能開啟加密資料庫，與 `asset_export.rs` 相同作法）。完成條件：`cargo test -p cvewatch --test asset_rename` 通過

## 3. frontend：表單更名

- [x] 3.1 `FormMode::Edit { original_tag }` 與 `AssetForm::renamed_to()`（design D3）；更新所有 `FormMode::Edit` 的使用處。完成條件：`asset_form.rs` 單元測試斷言編號未改、只有前後空白、空白時都回傳 None，`NAS-11` 回傳 `Some("NAS-11")`，`nas-01` → `NAS-01` 回傳 Some
- [x] 3.2 `Workspace::save_asset_form`：以 `original_tag` 找資產；`edit_steps` 在第一步加入「更名 OLD → NEW」；`fields_changed` 比較前把編號換回原值；只更名時改呼叫 `rename_asset`，且 `needs_match = false`（design D2、D3）。完成條件：`crates/frontend/tests/asset_rename.rs` 斷言只更名時 `needs_match` 為 false，訊息為「已儲存 NAS-11（更名 NAS-01 → NAS-11）」；更名＋改名稱時兩項都列出且 `needs_match` 為 true；只改名稱時不出現「更名」
- [x] 3.3 frontend 測試：`cargo test -p cvewatch-frontend` 通過（含 3.1～3.2，並加一個測試：更名為已使用的編號時回報 Error、資料不變）

## 4. GUI：編輯對話框

- [x] 4.1 `dialogs/asset_edit.rs`：編輯模式開放編號欄位；`renamed_to` 有值時，欄位下方以警示色顯示公開函式 `rename_note(原編號)` 的提示；標題維持「編輯 <原編號>」；更新模組開頭的說明註解（design D4）。完成條件：kittest（`crates/gui/tests/asset_rename.rs`，種子資料 `FW-EDGE`）斷言編號欄位可輸入，改為 `FW-BORDER` 後出現提示、標題維持「編輯 FW-EDGE」，改回 `FW-EDGE` 後提示消失
- [x] 4.2 GUI 測試：`crates/gui/tests/asset_rename.rs` 重現 spec 的 Renaming in the edit dialog（以 `FW-EDGE` → `FW-BORDER`：儲存後對話框關閉、清單選取新編號、名稱更新、命中仍在、背景比對一次；只更名時不比對）與 Renaming to a tag in use（對話框保留、顯示已被使用、編號與名稱不變）。完成條件：`cargo test -p cvewatch-gui` 全部通過

## 5. 文件與整合驗證

- [x] 5.1 README：資產編修段落移除「資產編號建立後不可修改」，改為說明可更名與影響；CLI 範例加上 `cvewatch asset rename NAS-01 NAS-11`；「Excel 往返」流程加註更名後先重新匯出清冊；「每次編修後背景比對」註明只更名時不需要。完成條件：README 中不再出現「不可修改」，並出現 `asset rename`
- [x] 5.2 全專案檢查：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`openspec validate 12-rename-asset-tag --strict`。完成條件：四個指令皆成功

## Workflow follow-up

- archive 時同步主 spec：`asset-inventory`（修改 Editing asset fields，新增 Renaming an asset、Old asset tag is not remembered、Import rejects tags differing only in letter case、Renaming from the command line）、`gui-console`（新增 GUI asset renaming；原規劃為修改 GUI asset editing，封存時拆出以符合需求描述 500 字上限）
- archive 後執行 `openspec validate --all --strict` 確認通過
