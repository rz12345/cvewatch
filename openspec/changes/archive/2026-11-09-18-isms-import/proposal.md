# Proposal

## Why

17 讓使用者能從 ISMS 資訊資產清冊產生 cvewatch 清冊草稿，但仍要走三步：產生草稿、在 Excel 補版本、再用「匯入清冊」匯入。ISMS 清冊每年覆核後都會改版，每次都得繞一份草稿檔。草稿必須多這一步，是因為現在每個元件都要有版本，而 ISMS 清冊沒有版本欄。

把「還不知道版本」變成元件可以明確標示的狀態，就能直接從 ISMS 清冊匯入：ISMS 已有的資料直接進資料庫，版本之後再在畫面補上。這段期間，系統必須明確提示這些元件還沒有納入比對，避免使用者以為已經受到監控。

## What Changes

- **版本待補元件**：清冊元件與未辨識產品可以沒有版本，狀態為「版本待補」
  - 版本待補的元件不參與比對；原有的命中保留，但不再更新
  - 資產清單與詳情要標示。GUI 與 TUI 的 Dashboard 顯示待補的元件數與資產數；GUI 點選數字後，資產畫面只列出有待補元件的資產
  - 每日摘要 email、`cvewatch match` 與 `cvewatch sync` 的結束摘要，在有待補元件時提醒
  - 在編輯對話框填上版本後，元件轉為一般元件並自動比對
- **一般清冊匯入**：**BREAKING** `version` 空白不再中止整批，而是視為版本待補
  - 資料庫已有版本、檔案中為空白時，元件改為版本待補，預覽列為版本變更（`11.1.6 → 待補`）
  - 預覽與完成摘要列出待補的元件數
  - 匯出時待補元件的 version 寫空白，因此匯出後立刻重匯的結果仍是「新增 0、更新 0」
- **手動編修**：新增資產與加入元件仍須填版本；既有版本也不能清成待補，只能把待補填上
- **從 ISMS 清冊匯入**：新增 `cvewatch asset import-isms <ISMS清冊.xlsx> [--dry-run]`，GUI 新增「從 ISMS 清冊匯入」（先預覽、確認後才寫入）
  - 讀取、略過、欄位對應、描述比對與沿用既有資產的規則與 17 相同
  - 新資產：直接建立。描述比對到的產品成為版本待補元件，比對不到的行成為版本待補的未辨識產品
  - 既有資產（依 ISMS 編號對應）：預覽列出 ISMS 欄位（類別、資產說明、C/I/A、管理欄位）的差異，確認後整批覆寫。asset_tag、名稱、曝露度、地點、備註與既有元件不動；描述中有、資產還沒有的產品加入為版本待補元件
  - 無效的列略過並列出原因，不中止整批；資產描述空白、沒有可建立元件的列也略過
  - 不提供退役選項；不在 ISMS 清冊中的資產只列在報表
- **移除 ISMS 草稿**：**BREAKING** 移除 `cvewatch asset isms-draft` 與 GUI「由 ISMS 清冊產生草稿」。以往在草稿裡拆多設備列的做法，改為匯入後在畫面新增資產

## Capabilities

### New Capabilities

（無）

### Modified Capabilities

- `isms-inventory`：草稿指令與草稿報表改為直接匯入與匯入報表；欄位對應、描述比對、沿用既有資產與重複產品的規則改寫為匯入用語
- `asset-inventory`：元件與未辨識產品可以是版本待補；匯入時空白 version 視為待補；匯出、元件編修與未辨識產品改為目錄產品時如何處理待補
- `vuln-matching`：版本待補的元件不參與比對；比對結束摘要提醒待補數
- `gui-console`：移除「GUI ISMS draft」，新增「GUI ISMS import」；Dashboard 與資產畫面標示版本待補
- `tui-console`：Dashboard 與資產畫面標示版本待補
- `email-alerts`：每日摘要列出版本待補的元件數

## Out of scope

- 從 ISMS 描述或其他來源推測版本、推測曝露度
- 把 ISMS 一列拆成多個資產；同一資產有多個相同產品的元件
- 匯出 ISMS 格式（改為 22-isms-export）
- 從 ISMS 清冊匯入時把資產標為退役（`--prune`）
- 把 ISMS 清冊拖進視窗自動判斷為 ISMS 匯入（拖放仍只走一般清冊匯入）
- 手動新增資產、加入元件時留空版本，或把既有版本清成待補
- 版本待補元件的「即將到期」或期限提醒
- PE、IF、SE 類資產；讀取「選項(勿刪)」工作表

## Impact

- 依賴：17-isms-draft-import（已 archive）。原 18–21 順延為 19-asset-identity-network、20-component-roles、21-asset-lifecycle-eol、22-isms-export
- `crates/core/src/assets/`：`validate.rs`（version 可空白）、`plan.rs`／`import.rs`（待補計數、版本變更顯示「待補」、歷史紀錄）、`edit.rs`（不得清空版本、填上待補）、`export.rs`
- `crates/core/src/assets/isms/`：`output.rs` 與 `isms_report` 工作表移除；草稿結果轉為匯入用的清冊；報表改為匯入報表
- `crates/core/src/matching/`：載入元件時排除待補；Dashboard 新增待補計數；來源矛盾不計待補元件
- `crates/core/src/notify/`：摘要內容
- `crates/cvewatch`：移除 `asset isms-draft`，新增 `asset import-isms`；`match`／`sync` 摘要
- `crates/frontend`、`crates/gui`、`crates/tui`：Dashboard、資產清單與詳情的待補標示與篩選；GUI 工具列按鈕與 companion 匯入流程
- 資料庫：不新增欄位（待補 = `version_raw` 為空字串），不需要 migration
- `docs/user-guide/`：「由 ISMS 清冊建立資產」改寫為匯入；說明版本待補
